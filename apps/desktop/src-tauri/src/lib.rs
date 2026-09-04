//! WorkDance desktop shell (WP0).
//! Tray + settings / permissions / calibration windows. No vision/ASR/inject.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod tray;

use parking_lot::Mutex;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use workdance_core::{load_config, AppConfig, RuntimeState};

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub runtime: Mutex<RuntimeState>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = load_config().unwrap_or_default();
    let show_first_run = !config.first_run_done;

    let state = AppState {
        config: Mutex::new(config),
        runtime: Mutex::new(RuntimeState::default()),
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
            commands::get_permissions,
            commands::open_os_permission_settings,
            commands::mark_first_run_done,
            commands::save_calibration,
            commands::get_config_path,
            commands::open_named_window,
        ])
        .setup(move |app| {
            tray::build_tray(app.handle())?;

            // Settings window is declared in tauri.conf.json (starts hidden).
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
