//! WorkDance desktop shell (WP0–WP2). No ASR / memo (WP3+).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod input_bridge;
mod tray;
mod vision_bridge;

use parking_lot::Mutex;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use workdance_core::{load_config, AppConfig, RuntimeState};
use workdance_vision::VisionWorker;

use crate::input_bridge::InputBridge;

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub runtime: Mutex<RuntimeState>,
    pub vision: Mutex<Option<VisionWorker>>,
    pub input: Mutex<Option<InputBridge>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = load_config().unwrap_or_default();
    let show_first_run = !config.first_run_done;

    let state = AppState {
        config: Mutex::new(config),
        runtime: Mutex::new(RuntimeState::default()),
        vision: Mutex::new(None),
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
                900.0,
                640.0,
            )?;

            *app.state::<AppState>().input.lock() = Some(InputBridge::start());
            vision_bridge::start_vision_worker(app.handle())?;

            if show_first_run {
                if let Some(w) = app.get_webview_window("permissions") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
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

pub fn show_window(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
