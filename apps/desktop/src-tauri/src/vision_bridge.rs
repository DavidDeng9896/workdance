//! Bridges `workdance-vision` dual-tier events into tray AppMode + input inject.

use tauri::{AppHandle, Manager};
use workdance_core::{probe_permissions, AppMode, PermissionStatus, VisionTier};
use workdance_vision::{VisionBackendStatus, VisionEvent, VisionWorker, VisionWorkerConfig};

use crate::{tray, AppState};

/// Best-effort gate: stub env, camera Granted, or post-wizard Unknown (not Missing).
pub fn may_start_vision(first_run_done: bool) -> bool {
    if std::env::var("WORKDANCE_VISION_STUB")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    let snap = probe_permissions();
    match snap.camera {
        PermissionStatus::Granted => true,
        PermissionStatus::Missing => false,
        PermissionStatus::Unknown => first_run_done,
    }
}

pub fn try_start_vision_worker(app: &AppHandle) -> Result<bool, String> {
    {
        let state = app.state::<AppState>();
        if state.vision.lock().is_some() {
            return Ok(true);
        }
        let first_run_done = state.config.lock().first_run_done;
        if !may_start_vision(first_run_done) {
            eprintln!(
                "[workdance] vision deferred: camera not ready (open 权限 / set WORKDANCE_VISION_STUB=1)"
            );
            return Ok(false);
        }
    }
    start_vision_worker(app)?;
    Ok(true)
}

pub fn start_vision_worker(app: &AppHandle) -> Result<(), String> {
    let handle = app.clone();
    let low_res = {
        let mode = app.state::<AppState>().config.lock().camera_mode.clone();
        mode != "hd"
    };
    let cfg = VisionWorkerConfig {
        force_stub: VisionWorkerConfig::default().force_stub,
        stub_script: None,
        low_res_camera: low_res,
    };

    let worker = VisionWorker::spawn(cfg, move |ev| match ev {
        VisionEvent::TierChanged { to, .. } => {
            apply_vision_tier(&handle, to);
            notify_input(&handle, to);
        }
        VisionEvent::BackendStatus(status) => {
            *handle.state::<AppState>().vision_status.lock() = Some(status);
        }
        VisionEvent::HandFrame(frame) => {
            notify_hand_frame(&handle, frame);
        }
    })
    .map_err(|e| e.to_string())?;

    *app.state::<AppState>().vision.lock() = Some(worker);
    Ok(())
}

pub fn current_vision_status(app: &AppHandle) -> VisionBackendStatus {
    app.state::<AppState>()
        .vision_status
        .lock()
        .clone()
        .unwrap_or_else(|| VisionBackendStatus {
            backend: "unknown".into(),
            ok: false,
            message: "视觉尚未启动".into(),
        })
}

fn notify_input(app: &AppHandle, tier: VisionTier) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().clone();
    let effective = if !cfg.gesture_enabled || cfg.voice_only {
        VisionTier::Sleep
    } else {
        tier
    };
    let guard = state.input.lock();
    if let Some(bridge) = guard.as_ref() {
        bridge.set_tier(effective);
        bridge.apply_policy(cfg.gesture_enabled, cfg.voice_only);
    }
    drop(guard);
}

fn notify_hand_frame(app: &AppHandle, frame: workdance_core::HandFrame) {
    let state = app.state::<AppState>();
    let guard = state.input.lock();
    if let Some(bridge) = guard.as_ref() {
        bridge.push_hand_frame(frame);
    }
}

fn apply_vision_tier(app: &AppHandle, tier: VisionTier) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().clone();
    if !cfg.gesture_enabled || cfg.voice_only {
        // Voice-only: tray stays Sleep unless Recording from software listen / G07.
        {
            let mut rt = state.runtime.lock();
            if rt.manual_override {
                return;
            }
            if rt.mode == AppMode::Recording {
                return;
            }
            if rt.mode != AppMode::Sleep {
                rt.mode = AppMode::Sleep;
                rt.recording_seconds = 0;
                drop(rt);
                let _ = tray::refresh_tray(app);
            }
        }
        return;
    }

    let next = AppMode::from_vision_tier(tier);
    {
        let mut rt = state.runtime.lock();
        if rt.manual_override {
            return;
        }
        if rt.mode == AppMode::Recording {
            return;
        }
        if rt.mode == next {
            return;
        }
        rt.mode = next;
        rt.recording_seconds = 0;
    }
    let _ = tray::refresh_tray(app);
}
