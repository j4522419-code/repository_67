//! The setup screen: shown on first run and from the tray's "Settings…",
//! where the browser and search engine are chosen.

use grandium_core::web;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::launcher;
use crate::platform;
use crate::settings::SettingsStore;

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
    current: SetupChoices,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupChoices {
    browser: Option<String>,
    search_engine: String,
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
    SetupOptions {
        browsers,
        search_engines,
        current: SetupChoices {
            browser: settings.browser,
            search_engine: settings.search_engine,
        },
    }
}

#[tauri::command]
pub fn save_setup(app: AppHandle, choices: SetupChoices) {
    app.state::<SettingsStore>().update(|settings| {
        settings.browser = choices.browser;
        settings.search_engine = web::search_engine(&choices.search_engine)
            .keyword
            .to_string();
        settings.setup_done = true;
    });
    platform::installer::clear_setup_request();
}
