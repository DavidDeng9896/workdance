//! WorkDance desktop shell (WP0–WP5).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod input_bridge;
mod tray;
mod vision_bridge;

use parking_lot::Mutex;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use workdance_core::{ensure_notes_dir, load_config, AppConfig, RuntimeState};
use workdance_vision::{VisionBackendStatus, VisionWorker};

use crate::input_bridge::InputBridge;

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub runtime: Mutex<RuntimeState>,
    pub vision: Mutex<Option<VisionWorker>>,
    pub vision_status: Mutex<Option<VisionBackendStatus>>,
    pub input: Mutex<Option<InputBridge>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = load_config().unwrap_or_default();
    let show_first_run = !config.first_run_done;
    if let Err(err) = ensure_notes_dir(&config.notes_path) {
        eprintln!("[workdance] notes dir: {err}");
    }

    let state = AppState {
        config: Mutex::new(config),
        runtime: Mutex::new(RuntimeState::default()),
        vision: Mutex::new(None),
        vision_status: Mutex::new(None),
        input: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_settings,
            commands::get_mode,
            commands::set_mode,
            commands::cycle_mode,
            commands::clear_manual_override,
            commands::get_permissions,
            commands::open_os_permission_settings,
            commands::mark_first_run_done,
            commands::save_calibration,
            commands::get_config_path,
            commands::open_named_window,
            commands::search_notes,
            commands::ensure_notes_directory,
            commands::start_voice_listen,
            commands::stop_voice_listen,
            commands::get_vision_status,
            commands::get_asr_status,
        ])
        .setup(move |app| {
            tray::build_tray(app.handle())?;

            ensure_window(
                app.handle(),
                "permissions",
                "首次设置",
                "permissions.html",
                440.0,
                520.0,
            )?;
            ensure_window(
                app.handle(),
                "calibration",
                "WorkDance 校准",
                "calibration.html",
                960.0,
                720.0,
            )?;
            ensure_pip_window(app.handle())?;

            // Input always starts (voice-only listen needs it); vision is gated.
            *app.state::<AppState>().input.lock() =
                Some(InputBridge::start(app.handle().clone()));
            {
                let cfg = app.state::<AppState>().config.lock().clone();
                if let Some(bridge) = app.state::<AppState>().input.lock().as_ref() {
                    bridge.apply_policy(cfg.gesture_enabled, cfg.voice_only);
                }
            }

            if show_first_run {
                if let Some(w) = app.get_webview_window("permissions") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                // Defer vision until wizard completes (mark_first_run_done) unless stub.
                if vision_bridge::may_start_vision(false) {
                    let _ = vision_bridge::try_start_vision_worker(app.handle())?;
                }
            } else {
                let _ = vision_bridge::try_start_vision_worker(app.handle())?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running WorkDance");
}

fn ensure_window(
    app: &tauri::AppHandle,
    label: &str,
    title: &str,
    path: &str,
    width: f64,
    height: f64,
) -> tauri::Result<()> {
    if app.get_webview_window(label).is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(path.into()))
        .title(title)
        .inner_size(width, height)
        .resizable(true)
        .visible(false)
        .center()
        .build()?;
    Ok(())
}

/// Continuity PiP (~180×120): gesture-active / recording chrome only.
fn ensure_pip_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("pip").is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "pip", WebviewUrl::App("pip.html".into()))
        .title("WorkDance Continuity")
        .inner_size(180.0, 120.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    Ok(())
}

pub fn show_window(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Show Continuity PiP for gesture/recording; hide on sleep. UI-only chrome.
pub fn sync_pip_window(app: &tauri::AppHandle) {
    let mode = app.state::<AppState>().runtime.lock().mode;
    let Some(w) = app.get_webview_window("pip") else {
        return;
    };
    match mode {
        workdance_core::AppMode::Sleep => {
            let _ = w.hide();
        }
        workdance_core::AppMode::GestureActive | workdance_core::AppMode::Recording => {
            // Bottom-right-ish; best-effort (multi-monitor ignored).
            if let Ok(Some(m)) = app.primary_monitor() {
                let size = m.size();
                let scale = m.scale_factor();
                let w_px = (180.0 * scale) as i32;
                let h_px = (120.0 * scale) as i32;
                let margin = (28.0 * scale) as i32;
                let x = size.width as i32 - w_px - margin;
                let y = size.height as i32 - h_px - margin - (40.0 * scale) as i32;
                let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
            }
            let _ = w.show();
        }
    }
}
