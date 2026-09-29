//! The launcher window: the global hotkey, showing and hiding, and placement.

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use grandium_core::layout::{launcher_origin, Rect};
use grandium_core::settings::OpenWith;
use grandium_core::DEFAULT_HOTKEY;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::apps;
use crate::platform::windows_key::{self, WindowsKeyListener};

/// Label of the launcher window in `tauri.conf.json`.
const LABEL: &str = "launcher";
/// Logical size limits; the UI picks the height to fit its content.
const WIDTH: f64 = 680.0;
const MAX_HEIGHT: f64 = 640.0;
/// Focus changes this soon after showing are part of activating the window,
/// not the user clicking away, so they don't hide it.
const BLUR_GRACE: Duration = Duration::from_millis(250);

pub struct LauncherState {
    open_with: Mutex<OpenWith>,
    /// Why Alt+Space couldn't be claimed, if it couldn't.
    hotkey_error: Mutex<Option<String>>,
    windows_key_error: Mutex<Option<String>>,
    windows_key: Mutex<Option<WindowsKeyListener>>,
    /// Where taps of the Windows key arrive.
    windows_key_taps: Sender<()>,
    shown_at: Mutex<Option<Instant>>,
}

impl LauncherState {
    /// Also starts the thread that turns Windows key taps into toggles.
    pub fn new(app: &AppHandle) -> Self {
        let (taps, tapped) = mpsc::channel();
        let app = app.clone();
        std::thread::spawn(move || {
            for () in tapped {
                toggle(&app);
            }
        });
        Self {
            open_with: Mutex::default(),
            hotkey_error: Mutex::default(),
            windows_key_error: Mutex::default(),
            windows_key: Mutex::default(),
            windows_key_taps: taps,
            shown_at: Mutex::default(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    version: String,
    /// The keys that open Grandium right now.
    pub keys: Vec<&'static str>,
    /// Set when Alt+Space was chosen but another app has it.
    hotkey_error: Option<String>,
    /// Set when the Windows key was chosen but couldn't be used.
    windows_key_error: Option<String>,
}

/// Claims the keys chosen in settings and lets go of the others. Problems
/// (usually another app owning Alt+Space) are kept for the UI and tray.
pub fn apply_open_with(app: &AppHandle, open_with: OpenWith) {
    let state = app.state::<LauncherState>();
    *state.open_with.lock().unwrap() = open_with;

    let shortcut: Shortcut = DEFAULT_HOTKEY.parse().expect("the default hotkey is valid");
    let shortcuts = app.global_shortcut();
    let hotkey_error = if !open_with.uses_alt_space() {
        let _ = shortcuts.unregister(shortcut);
        None
    } else if shortcuts.is_registered(shortcut) {
        None
    } else {
        shortcuts.register(shortcut).err().map(|e| e.to_string())
    };
    *state.hotkey_error.lock().unwrap() = hotkey_error;

    let mut listener = state.windows_key.lock().unwrap();
    let mut windows_key_error = None;
    if !open_with.uses_windows_key() {
        if let Some(running) = listener.take() {
            running.stop();
        }
    } else if listener.is_none() {
        match windows_key::start(state.windows_key_taps.clone()) {
            Ok(started) => *listener = Some(started),
            Err(error) => windows_key_error = Some(error),
        }
    }
    *state.windows_key_error.lock().unwrap() = windows_key_error;
}

pub fn status(app: &AppHandle) -> AppStatus {
    let state = app.state::<LauncherState>();
    let hotkey_error = state.hotkey_error.lock().unwrap().clone();
    let windows_key_error = state.windows_key_error.lock().unwrap().clone();
    let open_with = *state.open_with.lock().unwrap();
    let keys = open_with
        .keys()
        .into_iter()
        .filter(|&key| {
            if key == DEFAULT_HOTKEY {
                hotkey_error.is_none()
            } else {
                windows_key_error.is_none()
            }
        })
        .collect();
    AppStatus {
        version: app.package_info().version.to_string(),
        keys,
        hotkey_error,
        windows_key_error,
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
    }
    if let Some(state) = app.try_state::<LauncherState>() {
        *state.shown_at.lock().unwrap() = Some(Instant::now());
    }
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
