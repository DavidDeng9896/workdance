use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use workdance_core::{
    probe_permissions, save_config, AppConfig, AppMode, CalibrationProfile, PermissionsSnapshot,
};

use crate::{show_window, AppState};

#[derive(Serialize)]
pub struct ModeView {
    pub mode: AppMode,
    pub label_zh: String,
    pub tray_title_zh: String,
    pub recording_seconds: u32,
    pub manual_override: bool,
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
pub fn save_settings(state: State<'_, AppState>, patch: AppConfig) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock();
        *cfg = patch;
        if cfg.voice_only {
            cfg.gesture_enabled = false;
        }
        save_config(&cfg).map_err(|e| e.to_string())?;
    }
    let cfg = state.config.lock().clone();
    Ok(cfg)
}

#[tauri::command]
pub fn get_mode(state: State<'_, AppState>) -> ModeView {
    let rt = state.runtime.lock();
    ModeView {
        mode: rt.mode,
        label_zh: rt.mode.label_zh().into(),
        tray_title_zh: rt.mode.tray_title_zh(),
        recording_seconds: rt.recording_seconds,
        manual_override: rt.manual_override,
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
pub fn mark_first_run_done(state: State<'_, AppState>) -> Result<(), String> {
    let mut cfg = state.config.lock();
    cfg.first_run_done = true;
    save_config(&cfg).map_err(|e| e.to_string())?;
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

/// Used by tray menu callbacks (manual debug override).
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
