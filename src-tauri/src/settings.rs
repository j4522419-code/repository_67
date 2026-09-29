//! The user's settings, loaded at startup and saved on every change.

use std::path::PathBuf;
use std::sync::Mutex;

use grandium_core::settings::Settings;

use crate::files;

pub struct SettingsStore {
    settings: Mutex<Settings>,
    file: Option<PathBuf>,
}

impl SettingsStore {
    pub fn load(file: Option<PathBuf>) -> Self {
        Self {
            settings: Mutex::new(files::load_json(file.as_deref())),
            file,
        }
    }

    pub fn get(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    /// Changes settings and saves them, returning the new settings.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Settings {
        let mut settings = self.settings.lock().unwrap();
        change(&mut settings);
        if let Some(file) = &self.file {
            // A failed save only means the change won't survive a restart.
            let _ = files::save_json(file, &*settings);
        }
        settings.clone()
    }
}
