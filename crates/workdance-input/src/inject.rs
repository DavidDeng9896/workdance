//! Serial inject queue — one worker thread, never parallel OS inject.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use thiserror::Error;

use crate::engine::InjectCommand;

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("inject backend: {0}")]
    Backend(String),
    #[error("queue closed")]
    Closed,
}

pub trait InjectBackend: Send {
    fn name(&self) -> &'static str;
    fn execute(&mut self, cmd: &InjectCommand) -> Result<(), InjectError>;
}

/// Records commands for tests / Linux CI (no OS side effects).
#[derive(Debug, Default)]
pub struct RecordingInjector {
    pub log: Vec<InjectCommand>,
}

impl InjectBackend for RecordingInjector {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn execute(&mut self, cmd: &InjectCommand) -> Result<(), InjectError> {
        self.log.push(cmd.clone());
        Ok(())
    }
}

/// Null backend for Linux CI / headless — accepts and drops.
#[derive(Debug, Default)]
pub struct NullInjector;

impl InjectBackend for NullInjector {
    fn name(&self) -> &'static str {
        "null"
    }

    fn execute(&mut self, _cmd: &InjectCommand) -> Result<(), InjectError> {
        Ok(())
    }
}

/// Platform backend when available; otherwise null.
pub fn create_default_injector() -> Box<dyn InjectBackend> {
    #[cfg(target_os = "windows")]
    {
        return Box::new(windows::SendInputInjector::new());
    }
    #[cfg(target_os = "macos")]
    {
        return Box::new(macos::CgEventInjector::new());
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Box::new(NullInjector)
    }
}

pub struct InjectQueue {
    tx: Option<Sender<InjectCommand>>,
    stop: std::sync::Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl InjectQueue {
    pub fn spawn(mut backend: Box<dyn InjectBackend>) -> Self {
        let (tx, rx) = mpsc::channel::<InjectCommand>();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        eprintln!("[workdance-input] inject backend={}", backend.name());
        let handle = thread::Builder::new()
            .name("workdance-inject".into())
            .spawn(move || {
                while !stop_flag.load(Ordering::SeqCst) {
                    match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                        Ok(cmd) => {
                            if let Err(err) = backend.execute(&cmd) {
                                eprintln!("[workdance-input] inject error: {err}");
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                // Drain remaining serially.
                while let Ok(cmd) = rx.try_recv() {
                    let _ = backend.execute(&cmd);
                }
            })
            .expect("spawn inject thread");
        Self {
            tx: Some(tx),
            stop,
            handle: Some(handle),
        }
    }

    pub fn enqueue(&self, cmd: InjectCommand) -> Result<(), InjectError> {
        self.tx
            .as_ref()
            .ok_or(InjectError::Closed)?
            .send(cmd)
            .map_err(|_| InjectError::Closed)
    }

    pub fn enqueue_all<I: IntoIterator<Item = InjectCommand>>(
        &self,
        cmds: I,
    ) -> Result<(), InjectError> {
        for c in cmds {
            self.enqueue(c)?;
        }
        Ok(())
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for InjectQueue {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub struct SendInputInjector;

    impl SendInputInjector {
        pub fn new() -> Self {
            Self
        }
    }

    impl InjectBackend for SendInputInjector {
        fn name(&self) -> &'static str {
            "win-sendinput"
        }

        fn execute(&mut self, cmd: &InjectCommand) -> Result<(), InjectError> {
            // SAFETY: Win32 SendInput is the locked path-A API for WP2.
            // Full bindings stay behind cfg(windows); Linux CI never compiles this.
            unsafe { send_input(cmd) }
        }
    }

    unsafe fn send_input(cmd: &InjectCommand) -> Result<(), InjectError> {
        use std::mem::size_of;

        // Minimal FFI without extra crates — declare only what we need.
        #[repr(C)]
        struct INPUT {
            type_: u32,
            union_: INPUTUNION,
        }
        #[repr(C)]
        union INPUTUNION {
            mi: MOUSEINPUT,
            ki: KEYBDINPUT,
        }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct MOUSEINPUT {
            dx: i32,
            dy: i32,
            mouseData: u32,
            dwFlags: u32,
            time: u32,
            dwExtraInfo: usize,
        }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct KEYBDINPUT {
            wVk: u16,
            wScan: u16,
            dwFlags: u32,
            time: u32,
            dwExtraInfo: usize,
        }

        const INPUT_MOUSE: u32 = 0;
        const INPUT_KEYBOARD: u32 = 1;
        const MOUSEEVENTF_MOVE: u32 = 0x0001;
        const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;
        const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
        const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
        const MOUSEEVENTF_WHEEL: u32 = 0x0800;
        const KEYEVENTF_KEYUP: u32 = 0x0002;
        const VK_ESCAPE: u16 = 0x1B;

        #[link(name = "user32")]
        extern "system" {
            fn SendInput(cInputs: u32, pInputs: *mut INPUT, cbSize: i32) -> u32;
            fn GetSystemMetrics(nIndex: i32) -> i32;
        }

        let mut inputs: Vec<INPUT> = Vec::new();
        match cmd {
            InjectCommand::MoveAbs { x, y } => {
                let sx = GetSystemMetrics(0).max(1) as i64;
                let sy = GetSystemMetrics(1).max(1) as i64;
                let ax = ((*x as i64) * 65535 / (sx - 1)).clamp(0, 65535) as i32;
                let ay = ((*y as i64) * 65535 / (sy - 1)).clamp(0, 65535) as i32;
                inputs.push(INPUT {
                    type_: INPUT_MOUSE,
                    union_: INPUTUNION {
                        mi: MOUSEINPUT {
                            dx: ax,
                            dy: ay,
                            mouseData: 0,
                            dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
            InjectCommand::MoveDelta { dx, dy } => {
                inputs.push(INPUT {
                    type_: INPUT_MOUSE,
                    union_: INPUTUNION {
                        mi: MOUSEINPUT {
                            dx: *dx,
                            dy: *dy,
                            mouseData: 0,
                            dwFlags: MOUSEEVENTF_MOVE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
            InjectCommand::ClickLeft => {
                for flag in [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP] {
                    inputs.push(INPUT {
                        type_: INPUT_MOUSE,
                        union_: INPUTUNION {
                            mi: MOUSEINPUT {
                                dx: 0,
                                dy: 0,
                                mouseData: 0,
                                dwFlags: flag,
                                time: 0,
                                dwExtraInfo: 0,
                            },
                        },
                    });
                }
            }
            InjectCommand::Scroll { dy } => {
                inputs.push(INPUT {
                    type_: INPUT_MOUSE,
                    union_: INPUTUNION {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: (*dy as i16) as u32,
                            dwFlags: MOUSEEVENTF_WHEEL,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
            InjectCommand::KeyEscape => {
                for flags in [0u32, KEYEVENTF_KEYUP] {
                    inputs.push(INPUT {
                        type_: INPUT_KEYBOARD,
                        union_: INPUTUNION {
                            ki: KEYBDINPUT {
                                wVk: VK_ESCAPE,
                                wScan: 0,
                                dwFlags: flags,
                                time: 0,
                                dwExtraInfo: 0,
                            },
                        },
                    });
                }
            }
            InjectCommand::AppendText { text } => {
                // AX/UIA SetValue unavailable in WP3 path A → Unicode keyboard fallback.
                return unicode_type_text(text);
            }
        }

        if inputs.is_empty() {
            return Ok(());
        }
        let n = SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            size_of::<INPUT>() as i32,
        );
        if n as usize != inputs.len() {
            return Err(InjectError::Backend(format!(
                "SendInput returned {n}/{}",
                inputs.len()
            )));
        }
        Ok(())
    }

    unsafe fn unicode_type_text(text: &str) -> Result<(), InjectError> {
        use std::mem::size_of;

        #[repr(C)]
        struct INPUT {
            type_: u32,
            union_: INPUTUNION,
        }
        #[repr(C)]
        union INPUTUNION {
            ki: KEYBDINPUT,
        }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct KEYBDINPUT {
            wVk: u16,
            wScan: u16,
            dwFlags: u32,
            time: u32,
            dwExtraInfo: usize,
        }

        const INPUT_KEYBOARD: u32 = 1;
        const KEYEVENTF_KEYUP: u32 = 0x0002;
        const KEYEVENTF_UNICODE: u32 = 0x0004;

        #[link(name = "user32")]
        extern "system" {
            fn SendInput(cInputs: u32, pInputs: *mut INPUT, cbSize: i32) -> u32;
        }

        let mut inputs: Vec<INPUT> = Vec::new();
        for ch in text.encode_utf16() {
            for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
                inputs.push(INPUT {
                    type_: INPUT_KEYBOARD,
                    union_: INPUTUNION {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: ch,
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
        }
        if inputs.is_empty() {
            return Ok(());
        }
        let n = SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            size_of::<INPUT>() as i32,
        );
        if n as usize != inputs.len() {
            return Err(InjectError::Backend(format!(
                "SendInput unicode returned {n}/{}",
                inputs.len()
            )));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub struct CgEventInjector;

    impl CgEventInjector {
        pub fn new() -> Self {
            Self
        }
    }

    impl InjectBackend for CgEventInjector {
        fn name(&self) -> &'static str {
            "mac-cgevent"
        }

        fn execute(&mut self, cmd: &InjectCommand) -> Result<(), InjectError> {
            unsafe { cg_execute(cmd) }
        }
    }

    unsafe fn cg_execute(cmd: &InjectCommand) -> Result<(), InjectError> {
        // CoreGraphics FFI (path A). Only compiled on macOS.
        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        type CGEventRef = *mut std::ffi::c_void;
        type CGEventSourceRef = *mut std::ffi::c_void;
        type CGMouseButton = u32;
        type CGEventType = u32;
        type CGEventFlags = u64;
        type CGKeyCode = u16;

        const K_CG_EVENT_MOUSE_MOVED: CGEventType = 5;
        const K_CG_EVENT_LEFT_MOUSE_DOWN: CGEventType = 1;
        const K_CG_EVENT_LEFT_MOUSE_UP: CGEventType = 2;
        const K_CG_EVENT_SCROLL_WHEEL: CGEventType = 22;
        const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
        const K_CG_EVENT_KEY_UP: CGEventType = 11;
        const K_CG_MOUSE_BUTTON_LEFT: CGMouseButton = 0;
        const K_VK_ESCAPE: CGKeyCode = 53;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
            fn CGEventCreateMouseEvent(
                source: CGEventSourceRef,
                mouseType: CGEventType,
                mouseCursorPosition: CGPoint,
                mouseButton: CGMouseButton,
            ) -> CGEventRef;
            fn CGEventCreateScrollWheelEvent(
                source: CGEventSourceRef,
                units: u32,
                wheelCount: u32,
                wheel1: i32,
            ) -> CGEventRef;
            fn CGEventCreateKeyboardEvent(
                source: CGEventSourceRef,
                virtualKey: CGKeyCode,
                keyDown: bool,
            ) -> CGEventRef;
            fn CGEventPost(tap: u32, event: CGEventRef);
            fn CFRelease(cf: *mut std::ffi::c_void);
            fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
            fn CGEventSetLocation(event: CGEventRef, position: CGPoint);
        }
        const K_CG_HID_EVENT_TAP: u32 = 0;
        const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;

        let post = |ev: CGEventRef| {
            if ev.is_null() {
                return Err(InjectError::Backend("null CGEvent".into()));
            }
            CGEventPost(K_CG_HID_EVENT_TAP, ev);
            CFRelease(ev);
            Ok(())
        };

        match cmd {
            InjectCommand::MoveAbs { x, y } => {
                let pt = CGPoint {
                    x: *x as f64,
                    y: *y as f64,
                };
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null_mut(),
                    K_CG_EVENT_MOUSE_MOVED,
                    pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                post(ev)
            }
            InjectCommand::MoveDelta { dx, dy } => {
                let cur = CGEventCreate(std::ptr::null_mut());
                if cur.is_null() {
                    return Err(InjectError::Backend("CGEventCreate failed".into()));
                }
                let loc = CGEventGetLocation(cur);
                CFRelease(cur);
                let pt = CGPoint {
                    x: loc.x + *dx as f64,
                    y: loc.y + *dy as f64,
                };
                let ev = CGEventCreateMouseEvent(
                    std::ptr::null_mut(),
                    K_CG_EVENT_MOUSE_MOVED,
                    pt,
                    K_CG_MOUSE_BUTTON_LEFT,
                );
                post(ev)
            }
            InjectCommand::ClickLeft => {
                let cur = CGEventCreate(std::ptr::null_mut());
                if cur.is_null() {
                    return Err(InjectError::Backend("CGEventCreate failed".into()));
                }
                let loc = CGEventGetLocation(cur);
                CFRelease(cur);
                for ty in [K_CG_EVENT_LEFT_MOUSE_DOWN, K_CG_EVENT_LEFT_MOUSE_UP] {
                    let ev = CGEventCreateMouseEvent(
                        std::ptr::null_mut(),
                        ty,
                        loc,
                        K_CG_MOUSE_BUTTON_LEFT,
                    );
                    post(ev)?;
                }
                Ok(())
            }
            InjectCommand::Scroll { dy } => {
                let ev = CGEventCreateScrollWheelEvent(
                    std::ptr::null_mut(),
                    K_CG_SCROLL_EVENT_UNIT_LINE,
                    1,
                    -*dy, // natural: positive sample dy (down) → scroll down
                );
                post(ev)
            }
            InjectCommand::KeyEscape => {
                for down in [true, false] {
                    let ev =
                        CGEventCreateKeyboardEvent(std::ptr::null_mut(), K_VK_ESCAPE, down);
                    post(ev)?;
                }
                Ok(())
            }
            InjectCommand::AppendText { text } => {
                // AX SetValue unavailable → Unicode keyboard inject fallback.
                unicode_type_text(text)
            }
        }
    }

    unsafe fn unicode_type_text(text: &str) -> Result<(), InjectError> {
        type CGEventRef = *mut std::ffi::c_void;
        type CGEventSourceRef = *mut std::ffi::c_void;
        type CGKeyCode = u16;
        type UniChar = u16;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventCreateKeyboardEvent(
                source: CGEventSourceRef,
                virtualKey: CGKeyCode,
                keyDown: bool,
            ) -> CGEventRef;
            fn CGEventKeyboardSetUnicodeString(
                event: CGEventRef,
                length: usize,
                string: *const UniChar,
            );
            fn CGEventPost(tap: u32, event: CGEventRef);
            fn CFRelease(cf: *mut std::ffi::c_void);
        }
        const K_CG_HID_EVENT_TAP: u32 = 0;

        let utf16: Vec<UniChar> = text.encode_utf16().collect();
        for down in [true, false] {
            let ev = CGEventCreateKeyboardEvent(std::ptr::null_mut(), 0, down);
            if ev.is_null() {
                return Err(InjectError::Backend("null CGEvent unicode".into()));
            }
            CGEventKeyboardSetUnicodeString(ev, utf16.len(), utf16.as_ptr());
            CGEventPost(K_CG_HID_EVENT_TAP, ev);
            CFRelease(ev);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct SharedRec {
        inner: Arc<Mutex<Vec<InjectCommand>>>,
    }

    impl InjectBackend for SharedRec {
        fn name(&self) -> &'static str {
            "shared-rec"
        }
        fn execute(&mut self, cmd: &InjectCommand) -> Result<(), InjectError> {
            self.inner.lock().unwrap().push(cmd.clone());
            Ok(())
        }
    }

    #[test]
    fn queue_is_serial() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let q = InjectQueue::spawn(Box::new(SharedRec {
            inner: log.clone(),
        }));
        q.enqueue(InjectCommand::ClickLeft).unwrap();
        q.enqueue(InjectCommand::KeyEscape).unwrap();
        q.enqueue(InjectCommand::AppendText {
            text: "实验".into(),
        })
        .unwrap();
        q.stop();
        let got = log.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![
                InjectCommand::ClickLeft,
                InjectCommand::KeyEscape,
                InjectCommand::AppendText {
                    text: "实验".into()
                }
            ]
        );
    }

    #[test]
    fn null_injector_accepts_append_without_files() {
        let tmp = std::env::temp_dir().join("workdance-inject-append-probe");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let before: Vec<_> = std::fs::read_dir(&tmp).unwrap().collect();
        let mut inj = NullInjector;
        inj.execute(&InjectCommand::AppendText {
            text: "实验记录已追加。".into(),
        })
        .unwrap();
        let after: Vec<_> = std::fs::read_dir(&tmp).unwrap().collect();
        assert_eq!(before.len(), after.len());
    }
}
