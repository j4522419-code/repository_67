//! User settings, saved as JSON in `%APPDATA%\Grandium\settings.json`.

use serde::{Deserialize, Serialize};

use crate::clipboard::Limits;

/// Key combinations that can open Grandium. Win+Space and friends belong
/// to Windows, so they aren't offered.
pub const HOTKEYS: &[&str] = &[
    "Alt+Space",
    "Ctrl+Space",
    "Ctrl+Alt+Space",
    "Alt+Shift+Space",
    "Ctrl+Shift+Space",
];

/// How many clipboard history entries can be kept (pinned ones don't count).
pub const CLIPBOARD_ITEM_CHOICES: &[usize] = &[100, 500, 1000, 2000];
/// For how many days clipboard history is kept.
pub const CLIPBOARD_DAY_CHOICES: &[u32] = &[1, 7, 30, 90, 365];

const DAY: u64 = 24 * 60 * 60;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// Light or dark, like Windows.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "Like Windows",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }
}

/// Something hidden from search results, like an app never wanted there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenItem {
    /// The result's ID, like `app:<id>` or `file:<path>`.
    pub key: String,
    /// What it's called, for showing in settings.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// While paused, nothing new is added to clipboard history.
    pub clipboard_paused: bool,
    /// Where web searches and links open: an installed browser's ID, or
    /// `None` for Windows' default browser.
    pub browser: Option<String>,
    /// The everyday search engine, by keyword (see `web::SEARCH_ENGINES`).
    pub search_engine: String,
    /// Whether the first-run setup has been completed.
    pub setup_done: bool,
    /// What opens Grandium; one of [`HOTKEYS`].
    pub hotkey: String,
    pub theme: Theme,
    /// Start (quietly, in the tray) when you sign in to Windows.
    pub start_with_windows: bool,
    pub clipboard_max_items: usize,
    pub clipboard_max_days: u32,
    /// Folders file search looks in besides Desktop, Documents and
    /// Downloads.
    pub extra_folders: Vec<String>,
    /// Never shown in search results.
    pub hidden: Vec<HiddenItem>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clipboard_paused: false,
            browser: None,
            search_engine: crate::web::GOOGLE.keyword.to_string(),
            setup_done: false,
            hotkey: crate::DEFAULT_HOTKEY.to_string(),
            theme: Theme::System,
            start_with_windows: true,
            clipboard_max_items: 500,
            clipboard_max_days: 30,
            extra_folders: Vec::new(),
            hidden: Vec::new(),
        }
    }
}

impl Settings {
    /// The settings with anything unknown (a hand-edited file, say) put
    /// back to the closest thing that works.
    pub fn cleaned(mut self) -> Self {
        let defaults = Settings::default();
        if !HOTKEYS.contains(&self.hotkey.as_str()) {
            self.hotkey = defaults.hotkey;
        }
        if !CLIPBOARD_ITEM_CHOICES.contains(&self.clipboard_max_items) {
            self.clipboard_max_items = defaults.clipboard_max_items;
        }
        if !CLIPBOARD_DAY_CHOICES.contains(&self.clipboard_max_days) {
            self.clipboard_max_days = defaults.clipboard_max_days;
        }
        let mut seen = Vec::new();
        self.extra_folders.retain(|folder| {
            let key = folder.trim_end_matches(['\\', '/']).to_lowercase();
            let new = !folder.trim().is_empty() && !seen.contains(&key);
            seen.push(key);
            new
        });
        let mut seen = Vec::new();
        self.hidden.retain(|item| {
            let new = !seen.contains(&item.key);
            seen.push(item.key.clone());
            new
        });
        self
    }

    /// Hides a result from now on (once, however often it's asked).
    pub fn hide(&mut self, key: &str, name: &str) {
        if !self.hidden.iter().any(|item| item.key == key) {
            self.hidden.push(HiddenItem {
                key: key.to_string(),
                name: name.to_string(),
            });
        }
    }

    pub fn clipboard_limits(&self) -> Limits {
        Limits {
            max_entries: self.clipboard_max_items,
            max_age_secs: u64::from(self.clipboard_max_days) * DAY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_partial_and_older_files() {
        // Older versions saved which keys opened Grandium; that's ignored now.
        let settings: Settings =
            serde_json::from_str(r#"{"openWith":"windowsKey","setupDone":true}"#).unwrap();
        assert!(settings.setup_done);
        assert!(!settings.clipboard_paused);
        assert_eq!(settings.search_engine, "g");
        assert_eq!(settings.hotkey, "Alt+Space");
        assert!(settings.start_with_windows);
        assert_eq!(
            serde_json::from_str::<Settings>("{}").unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn round_trips_through_json() {
        let settings = Settings {
            clipboard_paused: true,
            browser: Some("Firefox-308046B0AF4A39CB".into()),
            search_engine: "ddg".into(),
            setup_done: true,
            hotkey: "Ctrl+Space".into(),
            theme: Theme::Dark,
            start_with_windows: false,
            clipboard_max_items: 2000,
            clipboard_max_days: 365,
            extra_folders: vec![r"D:\Projects".into()],
            hidden: vec![HiddenItem {
                key: "app:Microsoft.Edge".into(),
                name: "Microsoft Edge".into(),
            }],
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
    }

    #[test]
    fn cleans_up_unknown_values() {
        let settings = Settings {
            hotkey: "Win+Space".into(),
            clipboard_max_items: 7,
            clipboard_max_days: 0,
            extra_folders: vec![
                r"D:\Projects".into(),
                r"d:\projects\".into(),
                "  ".into(),
                r"E:\Music".into(),
            ],
            ..Settings::default()
        }
        .cleaned();
        assert_eq!(settings.hotkey, "Alt+Space");
        assert_eq!(settings.clipboard_max_items, 500);
        assert_eq!(settings.clipboard_max_days, 30);
        assert_eq!(settings.extra_folders, [r"D:\Projects", r"E:\Music"]);
    }

    #[test]
    fn hides_things_once() {
        let mut settings = Settings::default();
        settings.hide("app:x", "X");
        settings.hide("app:x", "X again");
        settings.hide("file:C:\\a.txt", "a.txt");
        assert_eq!(settings.hidden.len(), 2);
        assert_eq!(settings.hidden[0].name, "X");
        settings.hidden.push(settings.hidden[0].clone());
        assert_eq!(settings.cleaned().hidden.len(), 2);
    }

    #[test]
    fn clipboard_limits_follow_the_settings() {
        let settings = Settings {
            clipboard_max_items: 100,
            clipboard_max_days: 7,
            ..Settings::default()
        };
        assert_eq!(
            settings.clipboard_limits(),
            Limits {
                max_entries: 100,
                max_age_secs: 7 * DAY
            }
        );
        assert_eq!(Settings::default().clipboard_limits(), Limits::default());
    }
}
