//! Bridges `workdance-vision` dual-tier events into tray AppMode + input inject.

use tauri::{AppHandle, Manager};
use workdance_core::{AppMode, VisionTier};
use workdance_vision::{VisionEvent, VisionWorker, VisionWorkerConfig};

use crate::{tray, AppState};

pub fn start_vision_worker(app: &AppHandle) -> Result<(), String> {
    let handle = app.clone();
    let cfg = VisionWorkerConfig {
        force_stub: VisionWorkerConfig::default().force_stub,
        stub_script: None,
        low_res_camera: true,
    };

    let worker = VisionWorker::spawn(cfg, move |ev| {
        match ev {
            VisionEvent::TierChanged { to, .. } => {
                apply_vision_tier(&handle, to);
                notify_input(&handle, to);
            }
        }
    })
    .map_err(|e| e.to_string())?;

    *app.state::<AppState>().vision.lock() = Some(worker);
    Ok(())
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
    }
    drop(guard);
}

fn apply_vision_tier(app: &AppHandle, tier: VisionTier) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().clone();
    if !cfg.gesture_enabled || cfg.voice_only {
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
