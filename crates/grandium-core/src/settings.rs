//! User settings, saved as JSON in `%APPDATA%\Grandium\settings.json`.

use serde::{Deserialize, Serialize};

/// Which keys open Grandium.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenWith {
    #[default]
    AltSpace,
    /// The Windows key pressed and released on its own. The Start menu
    /// stays reachable with Ctrl+Esc or by clicking Start.
    WindowsKey,
    Both,
}

impl OpenWith {
    pub const ALL: [OpenWith; 3] = [OpenWith::AltSpace, OpenWith::WindowsKey, OpenWith::Both];

    pub fn uses_alt_space(self) -> bool {
        matches!(self, OpenWith::AltSpace | OpenWith::Both)
    }

    pub fn uses_windows_key(self) -> bool {
        matches!(self, OpenWith::WindowsKey | OpenWith::Both)
    }

    /// The keys, as shown to people.
    pub fn keys(self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.uses_alt_space() {
            keys.push(crate::DEFAULT_HOTKEY);
        }
        if self.uses_windows_key() {
            keys.push("Windows key");
        }
        keys
    }

    /// Name of the choice in menus.
    pub fn label(self) -> &'static str {
        match self {
            OpenWith::AltSpace => "Alt+Space",
            OpenWith::WindowsKey => "Windows key",
            OpenWith::Both => "Both",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub open_with: OpenWith,
    /// While paused, nothing new is added to clipboard history.
    pub clipboard_paused: bool,
    /// Where web searches and links open: an installed browser's ID, or
    /// `None` for Windows' default browser.
    pub browser: Option<String>,
    /// The everyday search engine, by keyword (see `web::SEARCH_ENGINES`).
    pub search_engine: String,
    /// Whether the first-run setup has been completed.
    pub setup_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open_with: OpenWith::default(),
            clipboard_paused: false,
            browser: None,
            search_engine: crate::web::GOOGLE.keyword.to_string(),
            setup_done: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_alt_space_only() {
        let settings = Settings::default();
        assert_eq!(settings.open_with, OpenWith::AltSpace);
        assert_eq!(settings.open_with.keys(), vec!["Alt+Space"]);
    }

    #[test]
    fn both_uses_both_keys() {
        assert!(OpenWith::Both.uses_alt_space() && OpenWith::Both.uses_windows_key());
        assert!(!OpenWith::WindowsKey.uses_alt_space());
        assert_eq!(OpenWith::Both.keys(), vec!["Alt+Space", "Windows key"]);
    }

    #[test]
    fn loads_partial_and_older_files() {
        let settings: Settings = serde_json::from_str(r#"{"openWith":"windowsKey"}"#).unwrap();
        assert_eq!(settings.open_with, OpenWith::WindowsKey);
        assert!(!settings.clipboard_paused);
        assert_eq!(settings.search_engine, "g");
        assert!(!settings.setup_done);
        assert_eq!(
            serde_json::from_str::<Settings>("{}").unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn round_trips_through_json() {
        let settings = Settings {
            open_with: OpenWith::Both,
            clipboard_paused: true,
            browser: Some("Firefox-308046B0AF4A39CB".into()),
            search_engine: "ddg".into(),
            setup_done: true,
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
    }
}
