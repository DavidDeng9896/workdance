use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use workdance_core::AppMode;

use crate::{commands, show_window, AppState};

const TRAY_ID: &str = "workdance-tray";

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = tray_icon_for_mode(AppMode::Sleep)?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("WorkDance · 休眠")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_settings" => show_window(app, "settings"),
            "open_calibration" => show_window(app, "calibration"),
            "open_permissions" => show_window(app, "permissions"),
            "voice_listen_start" => {
                let _ = commands::start_voice_listen(app.clone());
            }
            "voice_listen_stop" => {
                let _ = commands::stop_voice_listen(app.clone());
            }
            "mode_sleep" => {
                let _ = commands::apply_mode(app, AppMode::Sleep);
            }
            "mode_gesture" => {
                let _ = commands::apply_mode(app, AppMode::GestureActive);
            }
            "mode_recording" => {
                let _ = commands::apply_mode(app, AppMode::Recording);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                show_window(app, "settings");
            }
        })
        .build(app)?;

    let _ = refresh_tray(app);
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let open_settings = MenuItem::with_id(app, "open_settings", "打开设置", true, None::<&str>)?;
    let open_calibration =
        MenuItem::with_id(app, "open_calibration", "打开校准", true, None::<&str>)?;
    let open_permissions =
        MenuItem::with_id(app, "open_permissions", "打开权限", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let voice_start =
        MenuItem::with_id(app, "voice_listen_start", "仅语音 · 开始听写", true, None::<&str>)?;
    let voice_stop =
        MenuItem::with_id(app, "voice_listen_stop", "仅语音 · 结束听写", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let mode_sleep = MenuItem::with_id(app, "mode_sleep", "休眠", true, None::<&str>)?;
    let mode_gesture = MenuItem::with_id(app, "mode_gesture", "手势开", true, None::<&str>)?;
    let mode_recording = MenuItem::with_id(app, "mode_recording", "录音", true, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &open_settings,
            &open_calibration,
            &open_permissions,
            &sep1,
            &voice_start,
            &voice_stop,
            &sep2,
            &mode_sleep,
            &mode_gesture,
            &mode_recording,
            &sep3,
            &quit,
        ],
    )
}

pub fn refresh_tray(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (mode, recording_seconds) = {
        let rt = state.runtime.lock();
        (rt.mode, rt.recording_seconds)
    };
    let voice_only = state.config.lock().voice_only;
    let title = if voice_only {
        mode.tray_title_voice_only()
    } else if mode == AppMode::Recording {
        format!("WorkDance · 录音 · {recording_seconds}s")
    } else {
        mode.tray_title_zh()
    };

    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "tray missing".to_string())?;

    tray.set_tooltip(Some(&title)).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let _ = tray.set_title(Some(&title));
    }
    let icon = tray_icon_for_mode(mode).map_err(|e| e.to_string())?;
    tray.set_icon(Some(icon)).map_err(|e| e.to_string())?;
    crate::sync_pip_window(app);
    Ok(())
}

/// Solid-color RGBA tray glyph: grey sleep, blue gesture, red recording (Lumen).
fn tray_icon_for_mode(mode: AppMode) -> tauri::Result<Image<'static>> {
    let (r, g, b) = match mode {
        AppMode::Sleep => (70, 70, 80),
        AppMode::GestureActive => (37, 99, 235),
        AppMode::Recording => (220, 38, 38),
    };
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for _ in 0..size * size {
        rgba.extend_from_slice(&[r, g, b, 255]);
    }
    Ok(Image::new_owned(rgba, size, size))
}
