//! Camera + MediaPipe Hands → [`workdance_gesture`] → OpenLess dictation edges.
//!
//! Pointer injection and text insertion share [`workdance_gesture::with_io_serial`].
//! Keyboard hotkeys stay on the existing OpenLess monitors.
//! Continuity PiP (`workdance-pip`) shows camera + blue 21-point skeleton.
//!
//! The Rust gesture thread owns the webcam exclusively. PiP receives a BMP
//! preview + landmarks over `workdance:hand` (emit_to the pip label) so the
//! webview never calls getUserMedia — dual capture on macOS otherwise starves
//! MediaPipe of live frames (camera visible, skeleton/cursor dead).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::Engine;
use enigo::{Enigo, Settings};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use workdance_gesture::{
    apply_decision, default_hand_model_path, ApplyOutcome, CameraCapture, Decision, DictationEdges,
    EdgeSession, FrameBuffer, GateSnapshot, GestureGate, HandObservation, Landmark, MediaPipeHands,
    PointerInject, Screen, TrayState, HAND_LANDMARKS,
};

const PIP_LABEL: &str = "workdance-pip";
const PIP_W: f64 = 180.0;
const PIP_H: f64 = 120.0;
const PIP_PREVIEW_W: u32 = 180;
const PIP_PREVIEW_H: u32 = 120;
const LANDMARK_SMOOTH: f32 = 0.4;
const CURSOR_SMOOTH: f32 = 0.45;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkdanceStatus {
    pub gate: GateSnapshot,
    pub camera_open: bool,
    pub tracker_error: Option<String>,
    pub front_camera_mirror_x: bool,
    pub accessibility_denied: bool,
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
    accessibility_denied: AtomicBool,
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
            accessibility_denied: false,
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
            accessibility_denied: AtomicBool::new(false),
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
        apply_decisions(&self.shared, decisions, &mut None, &mut None);
        publish(&self.shared, false, None, None);
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

struct HostPointer<'a> {
    shared: &'a Shared,
    last_xy: &'a mut Option<(f32, f32)>,
    enigo: &'a mut Option<Enigo>,
}

impl PointerInject for HostPointer<'_> {
    fn move_to(&mut self, x: i32, y: i32) {
        // Do not pre-gate on AXIsProcessTrusted — it can lag after the user
        // grants Accessibility, leaving the cursor dead while the OS toggle is ON.
        // Enigo errors still set accessibility_denied for the PiP badge.
        let (sx, sy) = match *self.last_xy {
            Some((px, py)) => (
                px + (x as f32 - px) * CURSOR_SMOOTH,
                py + (y as f32 - py) * CURSOR_SMOOTH,
            ),
            None => (x as f32, y as f32),
        };
        *self.last_xy = Some((sx, sy));
        let ix = sx.round() as i32;
        let iy = sy.round() as i32;
        use enigo::{Coordinate, Mouse};
        if let Some(enigo) = ensure_enigo(self.shared, self.enigo) {
            if let Err(err) = enigo.move_mouse(ix, iy, Coordinate::Abs) {
                log::warn!("[workdance] pointer move: {err}");
                mark_inject_failure(self.shared, &err.to_string());
            } else if self.shared.accessibility_denied.load(Ordering::SeqCst) {
                self.shared
                    .accessibility_denied
                    .store(false, Ordering::SeqCst);
            }
        }
    }

    fn left_click(&mut self) {
        use enigo::{Button, Direction, Mouse};
        if let Some(enigo) = ensure_enigo(self.shared, self.enigo) {
            if let Err(err) = enigo.button(Button::Left, Direction::Click) {
                log::warn!("[workdance] pointer click: {err}");
                mark_inject_failure(self.shared, &err.to_string());
            } else if self.shared.accessibility_denied.load(Ordering::SeqCst) {
                self.shared
                    .accessibility_denied
                    .store(false, Ordering::SeqCst);
            }
        }
    }
}

fn ensure_enigo<'a>(shared: &Shared, slot: &'a mut Option<Enigo>) -> Option<&'a mut Enigo> {
    if slot.is_none() {
        match Enigo::new(&Settings::default()) {
            Ok(enigo) => *slot = Some(enigo),
            Err(err) => {
                log::warn!("[workdance] pointer backend: {err}");
                mark_inject_failure(shared, &err.to_string());
                return None;
            }
        }
    }
    slot.as_mut()
}

fn mark_inject_failure(shared: &Shared, err: &str) {
    let lower = err.to_ascii_lowercase();
    if lower.contains("access")
        || lower.contains("permission")
        || lower.contains("not trusted")
        || lower.contains("ax")
    {
        shared
            .accessibility_denied
            .store(true, Ordering::SeqCst);
        publish(shared, shared.status.lock().camera_open, None, None);
    }
}

fn run_loop(shared: Arc<Shared>) {
    let mut vision: Option<Vision> = None;
    let mut announced_error = false;
    let mut last_cursor: Option<(f32, f32)> = None;
    let mut enigo: Option<Enigo> = None;
    let mut smooth_landmarks: Option<[Landmark; HAND_LANDMARKS]> = None;
    let mut preview_tick: u32 = 0;
    let origin = Instant::now();
    while !shared.stop.load(Ordering::SeqCst) {
        let tick = Instant::now();
        if !shared.enabled.load(Ordering::SeqCst) {
            vision = None;
            smooth_landmarks = None;
            last_cursor = None;
            hide_pip(&shared.app);
            let decisions = {
                let mut gate = shared.gate.lock();
                if gate.snapshot().gesture_enabled {
                    gate.set_enabled(false)
                } else {
                    Vec::new()
                }
            };
            apply_decisions(&shared, decisions, &mut last_cursor, &mut enigo);
            publish(&shared, false, None, None);
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
                    ensure_pip_window(&shared.app);
                }
                Err(err) => {
                    if !announced_error {
                        log::warn!("[workdance] hand tracker unavailable: {err}");
                        announced_error = true;
                    }
                    shared.status.lock().tracker_error = Some(err);
                    hide_pip(&shared.app);
                    publish(&shared, false, None, None);
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
            }
        }
        let t_ms = origin.elapsed().as_millis() as u64;
        let (obs, frame_for_pip) = match vision.as_mut().unwrap().camera.next_frame() {
            Ok(frame) => {
                let obs = vision.as_mut().unwrap().hands.detect(&frame, t_ms);
                (obs, Some(frame))
            }
            Err(err) => {
                log::warn!("[workdance] camera frame: {err}");
                let lower = err.to_string().to_ascii_lowercase();
                if lower.contains("permission")
                    || lower.contains("denied")
                    || lower.contains("not authorized")
                    || lower.contains("not trusted")
                {
                    shared.status.lock().tracker_error =
                        Some(format!("camera unavailable: {err}"));
                }
                (HandObservation::absent(t_ms), None)
            }
        };

        let smoothed = smooth_observation(&obs, &mut smooth_landmarks);
        let external = dictation_busy(&shared.app);
        let decisions = shared.gate.lock().push(smoothed.clone(), external);
        apply_decisions(&shared, decisions, &mut last_cursor, &mut enigo);
        // ~15fps preview is enough for Continuity; landmarks still emit every tick.
        preview_tick = preview_tick.wrapping_add(1);
        let preview = if preview_tick % 2 == 0 {
            frame_for_pip.as_ref()
        } else {
            None
        };
        publish(&shared, true, Some(&smoothed), preview);
        let rest = Duration::from_millis(33).saturating_sub(tick.elapsed());
        if !rest.is_zero() {
            thread::sleep(rest);
        }
    }
}

fn smooth_observation(
    obs: &HandObservation,
    prev: &mut Option<[Landmark; HAND_LANDMARKS]>,
) -> HandObservation {
    let Some(raw) = obs.landmarks else {
        *prev = None;
        return HandObservation::absent(obs.t_ms);
    };
    let smoothed = match prev {
        Some(p) => {
            let mut out = raw;
            for i in 0..HAND_LANDMARKS {
                out[i].x = p[i].x + (raw[i].x - p[i].x) * LANDMARK_SMOOTH;
                out[i].y = p[i].y + (raw[i].y - p[i].y) * LANDMARK_SMOOTH;
                out[i].z = p[i].z + (raw[i].z - p[i].z) * LANDMARK_SMOOTH;
            }
            out
        }
        None => raw,
    };
    *prev = Some(smoothed);
    HandObservation::with_landmarks(obs.t_ms, smoothed, obs.confidence)
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

fn apply_decisions(
    shared: &Shared,
    decisions: Vec<Decision>,
    last_cursor: &mut Option<(f32, f32)>,
    enigo: &mut Option<Enigo>,
) {
    if decisions.is_empty() {
        return;
    }
    let mut edges = HostEdges {
        app: shared.app.clone(),
    };
    let mut pointer = HostPointer {
        shared,
        last_xy: last_cursor,
        enigo,
    };
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

/// Nearest-neighbour downscale + 24-bit BMP (no extra crate). Same pixels MediaPipe sees.
fn encode_pip_preview(frame: &FrameBuffer) -> Option<String> {
    let tw = PIP_PREVIEW_W;
    let th = PIP_PREVIEW_H;
    if frame.width == 0 || frame.height == 0 || frame.rgb.len() < (frame.width * frame.height * 3) as usize {
        return None;
    }
    let row_stride = ((tw * 3 + 3) / 4) * 4;
    let pixel_size = (row_stride * th) as usize;
    let file_size = 54 + pixel_size;
    let mut bmp = vec![0u8; file_size];
    bmp[0] = b'B';
    bmp[1] = b'M';
    bmp[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&(tw as i32).to_le_bytes());
    bmp[22..26].copy_from_slice(&(th as i32).to_le_bytes()); // bottom-up
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    let sw = frame.width as usize;
    let sh = frame.height as usize;
    for ty in 0..th as usize {
        // BMP rows are bottom-up; sample source top-down so preview matches landmarks.
        let src_y = ((th as usize - 1 - ty) * sh) / th as usize;
        let dst_row = 54 + ty * row_stride as usize;
        for tx in 0..tw as usize {
            let src_x = (tx * sw) / tw as usize;
            let si = (src_y * sw + src_x) * 3;
            let di = dst_row + tx * 3;
            // BMP is BGR.
            bmp[di] = frame.rgb[si + 2];
            bmp[di + 1] = frame.rgb[si + 1];
            bmp[di + 2] = frame.rgb[si];
        }
    }
    Some(base64::engine::general_purpose::STANDARD.encode(bmp))
}

fn publish(
    shared: &Shared,
    camera_open: bool,
    obs: Option<&HandObservation>,
    frame: Option<&FrameBuffer>,
) {
    let snapshot = shared.gate.lock().snapshot();
    let tracker_error = shared.status.lock().tracker_error.clone();
    let access_denied = shared.accessibility_denied.load(Ordering::SeqCst);
    let tray = TrayState::effective(
        snapshot.gate_state,
        tracker_error.as_deref(),
        access_denied,
    );
    let tooltip = tray.tooltip();
    {
        let mut status = shared.status.lock();
        status.gate = snapshot.clone();
        status.camera_open = camera_open && shared.enabled.load(Ordering::SeqCst);
        status.front_camera_mirror_x = shared.mirror_x;
        status.accessibility_denied = access_denied;
    }
    let changed = {
        let mut announced = shared.announced_tray.lock();
        let changed = *announced != Some(tray);
        *announced = Some(tray);
        changed
    };
    if changed {
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

    // Continuity PiP: landmarks whenever MediaPipe returned a hand (gate still
    // applies its own confidence). Frame preview keeps the overlay on the same
    // capture path as detection.
    let (hand_count, landmarks) = match obs.and_then(|o| o.landmarks.as_ref()) {
        Some(ls) => {
            let pts: Vec<LandmarkPt> = ls
                .iter()
                .map(|l| LandmarkPt {
                    x: l.x,
                    y: l.y,
                    z: l.z,
                })
                .collect();
            (1u32, Some(pts))
        }
        None => (0u32, None),
    };
    let frame_jpeg = frame.and_then(encode_pip_preview);
    let event = HandEvent {
        hand_count,
        gate_state: tray,
        landmarks,
        accessibility_denied: access_denied,
        label_zh: tray.label_zh().to_string(),
        frame_jpeg,
    };
    // OpenLess secondary windows consistently use emit_to(label). Global emit
    // alone has left the Continuity PiP without landmarks on device.
    let _ = shared.app.emit_to(PIP_LABEL, "workdance:hand", &event);
    let _ = shared.app.emit("workdance:hand", &event);
    sync_pip_visibility(
        &shared.app,
        camera_open && shared.enabled.load(Ordering::SeqCst),
        tray,
    );
}

fn ensure_pip_window(app: &AppHandle) {
    if app.get_webview_window(PIP_LABEL).is_some() {
        return;
    }
    let built = WebviewWindowBuilder::new(
        app,
        PIP_LABEL,
        WebviewUrl::App("index.html?window=workdance-pip".into()),
    )
    .title("WorkDance Hand")
    .inner_size(PIP_W, PIP_H)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .build();
    match built {
        Ok(w) => {
            position_pip_bottom_right(app, &w);
            let _ = w.show();
        }
        Err(err) => log::warn!("[workdance] pip window create failed: {err}"),
    }
}

fn position_pip_bottom_right(app: &AppHandle, w: &tauri::WebviewWindow) {
    if let Ok(Some(m)) = app.primary_monitor() {
        let size = m.size();
        let scale = m.scale_factor();
        let w_px = (PIP_W * scale) as i32;
        let h_px = (PIP_H * scale) as i32;
        let margin = (28.0 * scale) as i32;
        let x = size.width as i32 - w_px - margin;
        let y = size.height as i32 - h_px - margin - (40.0 * scale) as i32;
        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    }
}

fn sync_pip_visibility(app: &AppHandle, camera_open: bool, tray: TrayState) {
    let Some(w) = app.get_webview_window(PIP_LABEL) else {
        if camera_open {
            ensure_pip_window(app);
        }
        return;
    };
    match tray {
        TrayState::NotInstalled => {
            let _ = w.hide();
        }
        _ if camera_open => {
            position_pip_bottom_right(app, &w);
            let _ = w.show();
        }
        _ => {
            let _ = w.hide();
        }
    }
}

fn hide_pip(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PIP_LABEL) {
        let _ = w.hide();
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayEvent {
    state: TrayState,
    label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LandmarkPt {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandEvent {
    hand_count: u32,
    gate_state: TrayState,
    landmarks: Option<Vec<LandmarkPt>>,
    accessibility_denied: bool,
    label_zh: String,
    /// Base64 BMP of the Rust-owned camera frame (same pixels MediaPipe sees).
    /// Field name kept as frameJpeg for the existing PiP listener.
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_jpeg: Option<String>,
}
