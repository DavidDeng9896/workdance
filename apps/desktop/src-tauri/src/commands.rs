use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use workdance_core::{
    ensure_notes_dir, probe_permissions, save_config, search_memos, AppConfig, AppMode,
    CalibrationProfile, MemoHit, PermissionsSnapshot,
};

use crate::{show_window, vision_bridge, AppState};

#[derive(Serialize)]
pub struct ModeView {
    pub mode: AppMode,
    pub label_zh: String,
    pub tray_title_zh: String,
    pub recording_seconds: u32,
    pub manual_override: bool,
    pub voice_only: bool,
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.config.lock().clone()
}

#[tauri::command]
pub fn get_config_path() -> String {
    workdance_core::config_path().to_string_lossy().into_owned()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: AppConfig,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock();
        *cfg = patch;
        if cfg.voice_only {
            cfg.gesture_enabled = false;
        }
        if cfg.asr_language.trim().is_empty() {
            cfg.asr_language = "zh".into();
        }
        ensure_notes_dir(&cfg.notes_path).map_err(|e| e.to_string())?;
        save_config(&cfg).map_err(|e| e.to_string())?;
    }
    let cfg = state.config.lock().clone();
    if let Some(bridge) = state.input.lock().as_ref() {
        bridge.apply_policy(cfg.gesture_enabled, cfg.voice_only);
    }
    // Refresh tray tooltip for 仅语音.
    {
        let mut rt = state.runtime.lock();
        if cfg.voice_only && rt.mode == AppMode::GestureActive && !rt.manual_override {
            rt.mode = AppMode::Sleep;
        }
    }
    crate::tray::refresh_tray(&app)?;
    Ok(cfg)
}

#[tauri::command]
pub fn get_mode(state: State<'_, AppState>) -> ModeView {
    mode_view_from_state(&state)
}

fn mode_view_from_state(state: &State<'_, AppState>) -> ModeView {
    let rt = state.runtime.lock();
    let voice_only = state.config.lock().voice_only;
    let tray_title_zh = if voice_only {
        rt.mode.tray_title_voice_only()
    } else if rt.mode == AppMode::Recording {
        format!("WorkDance · 录音 · {}s", rt.recording_seconds)
    } else {
        rt.mode.tray_title_zh()
    };
    ModeView {
        mode: rt.mode,
        label_zh: rt.mode.label_zh().into(),
        tray_title_zh,
        recording_seconds: rt.recording_seconds,
        manual_override: rt.manual_override,
        voice_only,
    }
}

#[tauri::command]
pub fn set_mode(app: AppHandle, state: State<'_, AppState>, mode: AppMode) -> Result<ModeView, String> {
    {
        let mut rt = state.runtime.lock();
        rt.mode = mode;
        rt.manual_override = true;
        if mode != AppMode::Recording {
            rt.recording_seconds = 0;
        } else if rt.recording_seconds == 0 {
            rt.recording_seconds = 1;
        }
    }
    crate::tray::refresh_tray(&app)?;
    Ok(get_mode(state))
}

#[tauri::command]
pub fn cycle_mode(app: AppHandle, state: State<'_, AppState>) -> Result<ModeView, String> {
    {
        let mut rt = state.runtime.lock();
        rt.mode = rt.mode.cycle();
        rt.manual_override = true;
        if rt.mode == AppMode::Recording {
            rt.recording_seconds = 47;
        } else {
            rt.recording_seconds = 0;
        }
    }
    crate::tray::refresh_tray(&app)?;
    Ok(get_mode(state))
}

#[tauri::command]
pub fn clear_manual_override(app: AppHandle, state: State<'_, AppState>) -> Result<ModeView, String> {
    {
        let mut rt = state.runtime.lock();
        rt.manual_override = false;
        if rt.mode == AppMode::Recording {
            rt.mode = AppMode::Sleep;
            rt.recording_seconds = 0;
        }
    }
    crate::tray::refresh_tray(&app)?;
    Ok(get_mode(state))
}

#[tauri::command]
pub fn get_permissions() -> PermissionsSnapshot {
    probe_permissions()
}

#[tauri::command]
pub fn open_os_permission_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy"])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("当前平台无系统权限深链（开发/CI）。".into())
    }
}

#[tauri::command]
pub fn mark_first_run_done(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut cfg = state.config.lock();
        cfg.first_run_done = true;
        save_config(&cfg).map_err(|e| e.to_string())?;
    }
    // Best-effort: start vision after wizard (stub / Granted / Unknown).
    let _ = vision_bridge::try_start_vision_worker(&app)?;
    Ok(())
}

#[tauri::command]
pub fn save_calibration(
    state: State<'_, AppState>,
    sensitivity: f32,
    dead_zone: f32,
    confirm: bool,
) -> Result<AppConfig, String> {
    let mut cfg = state.config.lock();
    cfg.sensitivity = sensitivity.clamp(0.0, 1.0);
    cfg.dead_zone = dead_zone.clamp(0.0, 1.0);
    if confirm {
        cfg.calibration = CalibrationProfile {
            confirmed: true,
            corners: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        };
    }
    save_config(&cfg).map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

#[tauri::command]
pub fn open_named_window(app: AppHandle, label: String) {
    show_window(&app, &label);
}

/// List / filter markdown memos under configured `notes_path` (WP4).
#[tauri::command]
pub fn search_notes(state: State<'_, AppState>, query: String) -> Result<Vec<MemoHit>, String> {
    let notes = state.config.lock().notes_path.clone();
    search_memos(&notes, &query).map_err(|e| e.to_string())
}

/// Ensure notes directory exists (creates parents). Returns absolute path.
#[tauri::command]
pub fn ensure_notes_directory(state: State<'_, AppState>) -> Result<String, String> {
    let notes = state.config.lock().notes_path.clone();
    let path = ensure_notes_dir(&notes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// WP5: arm software listen without fist (仅语音).
#[tauri::command]
pub fn start_voice_listen(app: AppHandle) -> Result<ModeView, String> {
    let state = app.state::<AppState>();
    {
        let mut cfg = state.config.lock();
        cfg.voice_only = true;
        cfg.gesture_enabled = false;
        save_config(&cfg).map_err(|e| e.to_string())?;
    }
    if let Some(bridge) = state.input.lock().as_ref() {
        bridge.apply_policy(false, true);
        bridge.start_voice_listen();
    }
    crate::tray::refresh_tray(&app)?;
    Ok(mode_view(&app))
}

#[tauri::command]
pub fn stop_voice_listen(app: AppHandle) -> Result<ModeView, String> {
    let state = app.state::<AppState>();
    if let Some(bridge) = state.input.lock().as_ref() {
        bridge.stop_voice_listen();
    }
    {
        let mut rt = state.runtime.lock();
        if !rt.manual_override && rt.mode == AppMode::Recording {
            rt.mode = AppMode::Sleep;
            rt.recording_seconds = 0;
        }
    }
    crate::tray::refresh_tray(&app)?;
    Ok(mode_view(&app))
}

fn mode_view(app: &AppHandle) -> ModeView {
    let state = app.state::<AppState>();
    let rt = state.runtime.lock();
    let voice_only = state.config.lock().voice_only;
    let tray_title_zh = if voice_only {
        rt.mode.tray_title_voice_only()
    } else if rt.mode == AppMode::Recording {
        format!("WorkDance · 录音 · {}s", rt.recording_seconds)
    } else {
        rt.mode.tray_title_zh()
    };
    ModeView {
        mode: rt.mode,
        label_zh: rt.mode.label_zh().into(),
        tray_title_zh,
        recording_seconds: rt.recording_seconds,
        manual_override: rt.manual_override,
        voice_only,
    }
}

/// WP-M1: vision backend readiness for settings banner (never silent fake).
#[tauri::command]
pub fn get_vision_status(app: AppHandle) -> workdance_vision::VisionBackendStatus {
    vision_bridge::current_vision_status(&app)
}

/// WP-A1: ASR model installed / missing for settings banner (never Stub fake).
#[tauri::command]
pub fn get_asr_status() -> workdance_input::AsrBackendStatus {
    workdance_input::AsrBackendStatus::probe()
}

/// Used by tray menu callbacks (manual override).
pub fn apply_mode(app: &AppHandle, mode: AppMode) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut rt = state.runtime.lock();
        rt.mode = mode;
        rt.manual_override = true;
        if mode == AppMode::Recording {
            rt.recording_seconds = 47;
        } else {
            rt.recording_seconds = 0;
        }
    }
    crate::tray::refresh_tray(app)
}
