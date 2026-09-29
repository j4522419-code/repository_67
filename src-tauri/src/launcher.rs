//! The launcher window: the global hotkey, showing and hiding, and placement.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use grandium_core::layout::{launcher_origin, Rect};
use grandium_core::DEFAULT_HOTKEY;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::apps;
use crate::platform::clipboard::foreground_window;
use crate::settings::SettingsStore;

/// Label of the launcher window in `tauri.conf.json`.
const LABEL: &str = "launcher";
/// Logical size limits; the UI picks the height to fit its content.
const WIDTH: f64 = 680.0;
const MAX_HEIGHT: f64 = 720.0;
/// Focus changes this soon after showing are part of activating the window,
/// not the user clicking away, so they don't hide it.
const BLUR_GRACE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct LauncherState {
    /// Why Alt+Space couldn't be claimed, if it couldn't.
    hotkey_error: Mutex<Option<String>>,
    shown_at: Mutex<Option<Instant>>,
    /// The window that was in front before the launcher opened, where
    /// pasting goes.
    previous_window: Mutex<isize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    version: String,
    /// The key that opens Grandium.
    hotkey: &'static str,
    /// Set when another app has Alt+Space.
    pub hotkey_error: Option<String>,
    /// The first-run setup hasn't been completed yet.
    setup_needed: bool,
}

/// Claims Alt+Space. A problem (usually another app owning it) is kept for
/// the UI and tray.
pub fn register_hotkey(app: &AppHandle) {
    let shortcut: Shortcut = DEFAULT_HOTKEY.parse().expect("the default hotkey is valid");
    let shortcuts = app.global_shortcut();
    let error = if shortcuts.is_registered(shortcut) {
        None
    } else {
        shortcuts.register(shortcut).err().map(|e| e.to_string())
    };
    *app.state::<LauncherState>().hotkey_error.lock().unwrap() = error;
}

pub fn status(app: &AppHandle) -> AppStatus {
    AppStatus {
        version: app.package_info().version.to_string(),
        hotkey: DEFAULT_HOTKEY,
        hotkey_error: app
            .state::<LauncherState>()
            .hotkey_error
            .lock()
            .unwrap()
            .clone(),
        setup_needed: !app.state::<SettingsStore>().get().setup_done,
    }
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

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    apps::refresh_if_stale(app);
    if !window.is_visible().unwrap_or(false) {
        place_on_cursor_monitor(app, &window);
        remember_previous_window(app, &window);
    }
    if let Some(state) = app.try_state::<LauncherState>() {
        *state.shown_at.lock().unwrap() = Some(Instant::now());
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit("launcher-shown", ());
}

fn remember_previous_window(app: &AppHandle, window: &WebviewWindow) {
    let front = foreground_window();
    let ours = window.hwnd().map_or(0, |hwnd| hwnd.0 as isize);
    if let (Some(state), true) = (app.try_state::<LauncherState>(), front != ours) {
        *state.previous_window.lock().unwrap() = front;
    }
}

/// The window that was in front before the launcher opened.
pub fn previous_window(app: &AppHandle) -> isize {
    *app.state::<LauncherState>().previous_window.lock().unwrap()
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
            let Some(state) = window.try_state::<LauncherState>() else {
                return;
            };
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
pub fn app_status(app: AppHandle) -> AppStatus {
    status(&app)
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
