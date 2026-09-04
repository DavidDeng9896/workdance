//! Bridges `workdance-vision` dual-tier events into tray AppMode.

use tauri::{AppHandle, Manager};
use workdance_core::AppMode;
use workdance_vision::{VisionEvent, VisionWorker, VisionWorkerConfig};

use crate::{tray, AppState};

pub fn start_vision_worker(app: &AppHandle) -> Result<(), String> {
    let handle = app.clone();
    let cfg = VisionWorkerConfig {
        // CI / headless: WORKDANCE_VISION_STUB=1 forces scripted palm enter/leave.
        force_stub: VisionWorkerConfig::default().force_stub,
        stub_script: None,
        low_res_camera: true,
    };

    let worker = VisionWorker::spawn(cfg, move |ev| {
        match ev {
            VisionEvent::TierChanged { to, .. } => apply_vision_tier(&handle, to),
        }
    })
    .map_err(|e| e.to_string())?;

    *app.state::<AppState>().vision.lock() = Some(worker);
    Ok(())
}

fn apply_vision_tier(app: &AppHandle, tier: workdance_core::VisionTier) {
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
            // Recording is WP3; vision must not clobber it.
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
