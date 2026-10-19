//! Camera + MediaPipe Hands → [`workdance_gesture`] → OpenLess dictation edges.
//!
//! Pointer injection and text insertion share [`workdance_gesture::with_io_serial`].
//! Keyboard hotkeys stay on the existing OpenLess monitors.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use workdance_gesture::{
    apply_decision, default_hand_model_path, ApplyOutcome, CameraCapture, Decision, DictationEdges,
    EdgeSession, GateSnapshot, GestureGate, HandObservation, MediaPipeHands, PointerInject, Screen,
    TrayState,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkdanceStatus {
    pub gate: GateSnapshot,
    pub camera_open: bool,
    pub tracker_error: Option<String>,
    pub front_camera_mirror_x: bool,
}

struct Shared {
    app: AppHandle,
    enabled: AtomicBool,
    stop: AtomicBool,
    mirror_x: bool,
    gate: Mutex<GestureGate>,
    session: Mutex<EdgeSession>,
    status: Mutex<WorkdanceStatus>,
    announced_tray: Mutex<Option<TrayState>>,
    join: Mutex<Option<JoinHandle<()>>>,
}

pub struct WorkdanceRuntime {
    shared: Arc<Shared>,
}

impl WorkdanceRuntime {
    pub fn spawn(app: AppHandle) -> Arc<Self> {
        let enabled = std::env::var("WORKDANCE_GESTURE").ok().as_deref() != Some("0");
        let mirror_x = std::env::var("WORKDANCE_FRONT_CAMERA").ok().as_deref() != Some("0");
        let gate = GestureGate::default();
        let status = WorkdanceStatus {
            gate: gate.snapshot(),
            camera_open: false,
            tracker_error: None,
            front_camera_mirror_x: mirror_x,
        };
        let shared = Arc::new(Shared {
            app,
            enabled: AtomicBool::new(enabled),
            stop: AtomicBool::new(false),
            mirror_x,
            gate: Mutex::new(gate),
            session: Mutex::new(EdgeSession::default()),
            status: Mutex::new(status),
            announced_tray: Mutex::new(None),
            join: Mutex::new(None),
        });
        if !enabled {
            let _ = shared.gate.lock().set_enabled(false);
            shared.status.lock().gate = shared.gate.lock().snapshot();
        }
        let worker = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("workdance-gesture".into())
            .spawn(move || run_loop(worker))
            .expect("spawn workdance gesture thread");
        *shared.join.lock() = Some(handle);
        Arc::new(Self { shared })
    }

    pub fn set_enabled(&self, enabled: bool) -> bool {
        self.shared.enabled.store(enabled, Ordering::SeqCst);
        let decisions = self.shared.gate.lock().set_enabled(enabled);
        apply_decisions(&self.shared, decisions);
        publish(&self.shared, false);
        enabled
    }

    pub fn status(&self) -> WorkdanceStatus {
        self.shared.status.lock().clone()
    }
}

impl Drop for WorkdanceRuntime {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.shared.join.lock().take() {
            let _ = handle.join();
        }
    }
}

struct Vision {
    camera: CameraCapture,
    hands: MediaPipeHands,
}

impl Vision {
    fn open() -> Result<Self, String> {
        let camera = CameraCapture::open_default().map_err(|e| e.to_string())?;
        let model = default_hand_model_path();
        let hands = MediaPipeHands::open(&model)?;
        Ok(Self { camera, hands })
    }
}

struct HostEdges {
    app: AppHandle,
}

impl DictationEdges for HostEdges {
    fn alloc_press_id(&mut self) -> u64 {
        crate::hotkey::next_press_id()
    }

    fn edge(&mut self, pressed: bool, press_id: u64) -> bool {
        let coord = self.app.state::<Arc<crate::coordinator::Coordinator>>();
        coord.dispatch_workdance_dictation_edge(pressed, press_id)
    }
}

struct HostPointer;

impl PointerInject for HostPointer {
    fn move_to(&mut self, x: i32, y: i32) {
        use enigo::{Coordinate, Enigo, Mouse, Settings};
        match Enigo::new(&Settings::default()) {
            Ok(mut enigo) => {
                if let Err(err) = enigo.move_mouse(x, y, Coordinate::Abs) {
                    log::warn!("[workdance] pointer move: {err}");
                }
            }
            Err(err) => log::warn!("[workdance] pointer backend: {err}"),
        }
    }

    fn left_click(&mut self) {
        use enigo::{Button, Direction, Enigo, Mouse, Settings};
        match Enigo::new(&Settings::default()) {
            Ok(mut enigo) => {
                if let Err(err) = enigo.button(Button::Left, Direction::Click) {
                    log::warn!("[workdance] pointer click: {err}");
                }
            }
            Err(err) => log::warn!("[workdance] pointer backend: {err}"),
        }
    }
}

fn run_loop(shared: Arc<Shared>) {
    let mut vision: Option<Vision> = None;
    let mut announced_error = false;
    let origin = Instant::now();
    while !shared.stop.load(Ordering::SeqCst) {
        let tick = Instant::now();
        if !shared.enabled.load(Ordering::SeqCst) {
            vision = None;
            let decisions = {
                let mut gate = shared.gate.lock();
                if gate.snapshot().enabled {
                    gate.set_enabled(false)
                } else {
                    Vec::new()
                }
            };
            apply_decisions(&shared, decisions);
            publish(&shared, false);
            thread::sleep(Duration::from_millis(200));
            continue;
        }
        if vision.is_none() {
            match Vision::open() {
                Ok(opened) => {
                    vision = Some(opened);
                    announced_error = false;
                    shared.status.lock().tracker_error = None;
                    log::info!("[workdance] hand tracker running");
                }
                Err(err) => {
                    if !announced_error {
                        log::warn!("[workdance] hand tracker unavailable: {err}");
                        announced_error = true;
                    }
                    shared.status.lock().tracker_error = Some(err);
                    publish(&shared, false);
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
            }
        }
        let t_ms = origin.elapsed().as_millis() as u64;
        let obs = match vision.as_mut().unwrap().camera.next_frame() {
            Ok(frame) => vision.as_mut().unwrap().hands.detect(&frame, t_ms),
            Err(err) => {
                log::warn!("[workdance] camera frame: {err}");
                HandObservation::absent(t_ms)
            }
        };
        let external = dictation_busy(&shared.app);
        let decisions = shared.gate.lock().push(obs, external);
        apply_decisions(&shared, decisions);
        publish(&shared, true);
        let rest = Duration::from_millis(33).saturating_sub(tick.elapsed());
        if !rest.is_zero() {
            thread::sleep(rest);
        }
    }
}

fn dictation_busy(app: &AppHandle) -> bool {
    let Some(coord) = app.try_state::<Arc<crate::coordinator::Coordinator>>() else {
        return false;
    };
    matches!(
        coord.backend().snapshot().dictation.phase,
        openless_core::DictationPhase::Starting
            | openless_core::DictationPhase::Recording
            | openless_core::DictationPhase::Transcribing
            | openless_core::DictationPhase::Polishing
            | openless_core::DictationPhase::Inserting
    )
}

fn apply_decisions(shared: &Shared, decisions: Vec<Decision>) {
    if decisions.is_empty() {
        return;
    }
    let mut edges = HostEdges {
        app: shared.app.clone(),
    };
    let mut pointer = HostPointer;
    let screen = current_screen(&shared.app, shared.mirror_x);
    let mut session = shared.session.lock();
    for decision in decisions {
        if apply_decision(decision, &mut edges, &mut pointer, screen, &mut session)
            == ApplyOutcome::AbandonRecording
        {
            shared.gate.lock().abandon_recording();
        }
    }
}

fn current_screen(app: &AppHandle, mirror_x: bool) -> Screen {
    let (width, height) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let size = monitor.size();
            (size.width as i32, size.height as i32)
        })
        .unwrap_or((1920, 1080));
    Screen {
        width,
        height,
        mirror_x,
    }
}

fn publish(shared: &Shared, camera_open: bool) {
    let snapshot = shared.gate.lock().snapshot();
    let tray = snapshot.tray;
    let tooltip = tray.tooltip();
    {
        let mut status = shared.status.lock();
        status.gate = snapshot;
        status.camera_open = camera_open && shared.enabled.load(Ordering::SeqCst);
        status.front_camera_mirror_x = shared.mirror_x;
    }
    let changed = {
        let mut announced = shared.announced_tray.lock();
        let changed = *announced != Some(tray);
        *announced = Some(tray);
        changed
    };
    if !changed {
        return;
    }
    if let Some(icon) = shared.app.tray_by_id("main-tray") {
        let _ = icon.set_tooltip(Some(tooltip.clone()));
        let _ = icon.set_title(Some(tray.label_zh()));
    }
    let _ = shared.app.emit(
        "workdance:tray",
        TrayEvent {
            state: tray,
            label: tooltip,
        },
    );
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayEvent {
    state: TrayState,
    label: String,
}
