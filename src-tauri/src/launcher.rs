//! The launcher window: the global hotkey, showing and hiding, and placement.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use grandium_core::layout::{launcher_origin, Rect};
use grandium_core::DEFAULT_HOTKEY;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow, Window,
    WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

/// Label of the launcher window in `tauri.conf.json`.
const LABEL: &str = "launcher";
/// Logical size limits; the UI picks the height to fit its content.
const WIDTH: f64 = 680.0;
const MAX_HEIGHT: f64 = 640.0;
/// Focus changes this soon after showing are part of activating the window,
/// not the user clicking away, so they don't hide it.
const BLUR_GRACE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct LauncherState {
    hotkey_error: Mutex<Option<String>>,
    shown_at: Mutex<Option<Instant>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    version: String,
    hotkey: String,
    hotkey_error: Option<String>,
}

/// Registers the global hotkey. On failure (usually another app owns the
/// same keys) the error is kept so the UI and tray can explain it.
pub fn register_hotkey(app: &AppHandle) {
    let result = DEFAULT_HOTKEY
        .parse::<Shortcut>()
        .map_err(|e| e.to_string())
        .and_then(|shortcut| {
            app.global_shortcut()
                .register(shortcut)
                .map_err(|e| e.to_string())
        });
    *app.state::<LauncherState>().hotkey_error.lock().unwrap() = result.err();
}

pub fn hotkey_error(app: &AppHandle) -> Option<String> {
    app.state::<LauncherState>()
        .hotkey_error
        .lock()
        .unwrap()
        .clone()
}

pub fn on_hotkey(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() == ShortcutState::Pressed {
        toggle(app);
    }
}

/// Hides the launcher if it's in use, otherwise brings it up.
fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let in_use = window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false);
    if in_use {
        let _ = window.hide();
    } else {
        show(app);
    }
}

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        place_on_cursor_monitor(app, &window);
    }
    *app.state::<LauncherState>().shown_at.lock().unwrap() = Some(Instant::now());
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit("launcher-shown", ());
}

/// Moves the window to the monitor the mouse is on, so it opens where
/// you're looking.
fn place_on_cursor_monitor(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|cursor| app.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size), Ok(scale)) =
        (monitor, window.outer_size(), window.scale_factor())
    else {
        return;
    };

    // The size is in the current monitor's pixels; convert it to the
    // target monitor's, which may use a different display scale.
    let size = size
        .to_logical::<f64>(scale)
        .to_physical::<u32>(monitor.scale_factor());
    let area = monitor.work_area();
    let area = Rect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    };
    let (x, y) = launcher_origin(area, size.width, size.height);
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    match event {
        // Clicking anywhere else hides the launcher.
        WindowEvent::Focused(false) => {
            let state = window.state::<LauncherState>();
            let just_shown = state
                .shown_at
                .lock()
                .unwrap()
                .is_some_and(|at| at.elapsed() < BLUR_GRACE);
            if !just_shown {
                let _ = window.hide();
            }
        }
        // Alt+F4 hides the launcher instead of quitting the app.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
        }
        _ => {}
    }
}

#[tauri::command]
pub fn app_status(app: AppHandle, state: State<LauncherState>) -> AppStatus {
    AppStatus {
        version: app.package_info().version.to_string(),
        hotkey: DEFAULT_HOTKEY.to_string(),
        hotkey_error: state.hotkey_error.lock().unwrap().clone(),
    }
}

#[tauri::command]
pub fn hide_launcher(window: WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
pub fn set_launcher_height(window: WebviewWindow, height: f64) -> Result<(), String> {
    window
        .set_size(LogicalSize::new(WIDTH, height.clamp(1.0, MAX_HEIGHT)))
        .map_err(|e| e.to_string())
}
