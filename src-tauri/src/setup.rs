//! The setup screen: shown on first run, after installing, and from the
//! tray's "Settings…". It's where every setting is changed.

use grandium_core::settings::{
    Settings, Theme, CLIPBOARD_DAY_CHOICES, CLIPBOARD_ITEM_CHOICES, HOTKEYS,
};
use grandium_core::web;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::Win32::Foundation::HWND;

use crate::clipboard::ClipboardStore;
use crate::platform::{self, Com};
use crate::settings::SettingsStore;
use crate::{file_search, launcher, tray};

#[derive(Serialize)]
pub struct Choice {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupOptions {
    /// Installed browsers; choosing none means Windows' default browser.
    browsers: Vec<Choice>,
    search_engines: Vec<Choice>,
    hotkeys: Vec<String>,
    themes: Vec<Choice>,
    clipboard_items: Vec<usize>,
    clipboard_days: Vec<u32>,
    /// The folders file search always looks in, by name.
    user_folders: Vec<String>,
    current: SetupChoices,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupChoices {
    browser: Option<String>,
    search_engine: String,
    hotkey: String,
    theme: Theme,
    start_with_windows: bool,
    clipboard_max_items: usize,
    clipboard_max_days: u32,
    extra_folders: Vec<String>,
}

/// Opens the launcher on the setup screen.
pub fn open(app: &AppHandle) {
    launcher::show(app);
    let _ = app.emit("show-setup", ());
}

#[tauri::command]
pub fn setup_options(app: AppHandle) -> SetupOptions {
    let settings = app.state::<SettingsStore>().get();
    let browsers = platform::installed_browsers()
        .into_iter()
        .map(|browser| Choice {
            id: browser.id,
            name: browser.name,
        })
        .collect();
    let search_engines = web::SEARCH_ENGINES
        .iter()
        .map(|engine| Choice {
            id: engine.keyword.into(),
            name: engine.name.into(),
        })
        .collect();
    let themes = Theme::ALL
        .iter()
        .map(|&theme| Choice {
            id: serde_json::to_value(theme)
                .ok()
                .and_then(|id| id.as_str().map(String::from))
                .unwrap_or_default(),
            name: theme.label().into(),
        })
        .collect();
    SetupOptions {
        browsers,
        search_engines,
        hotkeys: HOTKEYS.iter().map(|&h| h.to_string()).collect(),
        themes,
        clipboard_items: CLIPBOARD_ITEM_CHOICES.to_vec(),
        clipboard_days: CLIPBOARD_DAY_CHOICES.to_vec(),
        user_folders: platform::user_folders()
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect(),
        current: SetupChoices {
            browser: settings.browser,
            search_engine: settings.search_engine,
            hotkey: settings.hotkey,
            theme: settings.theme,
            start_with_windows: settings.start_with_windows,
            clipboard_max_items: settings.clipboard_max_items,
            clipboard_max_days: settings.clipboard_max_days,
            extra_folders: settings.extra_folders,
        },
    }
}

/// Saves the setup screen's choices and puts them into effect.
#[tauri::command]
pub fn save_setup(app: AppHandle, choices: SetupChoices) -> Result<(), String> {
    let store = app.state::<SettingsStore>();
    let before = store.get();
    let after = store.update(|settings| {
        *settings = Settings {
            browser: choices.browser,
            search_engine: web::search_engine(&choices.search_engine)
                .keyword
                .to_string(),
            hotkey: choices.hotkey,
            theme: choices.theme,
            start_with_windows: choices.start_with_windows,
            clipboard_max_items: choices.clipboard_max_items,
            clipboard_max_days: choices.clipboard_max_days,
            extra_folders: choices.extra_folders,
            setup_done: true,
            ..std::mem::take(settings)
        }
        .cleaned();
    });
    platform::installer::clear_setup_request();
    apply(&app, Some(&before), &after)
}

/// Puts settings into effect: at startup (`before` is `None`) and after
/// they change.
pub fn apply(app: &AppHandle, before: Option<&Settings>, after: &Settings) -> Result<(), String> {
    if before.is_none_or(|b| b.hotkey != after.hotkey) {
        launcher::register_hotkey(app, &after.hotkey);
        tray::refresh_tooltip(app);
    }
    launcher::apply_theme(app, after.theme);
    if let Some(before) = before {
        app.state::<ClipboardStore>()
            .set_limits(after.clipboard_limits());
        if before.extra_folders != after.extra_folders {
            file_search::set_extra_folders(app, &after.extra_folders);
        }
    }
    // Grandium is only added to the startup list from the setup screen,
    // where it's asked. At startup, an entry is just kept pointing here.
    let startup = if before.is_some() {
        platform::startup::set_start_with_windows(after.start_with_windows)
    } else {
        platform::startup::keep_in_step(after.start_with_windows)
    };
    startup.map_err(|e| format!("Couldn't change whether Grandium starts with Windows: {e}"))
}

/// Asks for a folder to add to file search. `None` if none was picked.
#[tauri::command]
pub async fn pick_folder(app: AppHandle, window: WebviewWindow) -> Result<Option<String>, String> {
    // Window handles can't cross threads; their numbers can.
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    launcher::set_dialog_open(&app, true);
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let _com = Com::init();
        platform::dialog::pick_folder(HWND(owner as *mut _), "Add a folder to file search")
    })
    .await
    .map_err(|e| e.to_string());
    launcher::set_dialog_open(&app, false);
    let _ = window.set_focus();
    picked?
}
