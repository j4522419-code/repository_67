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

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub open_with: OpenWith,
    /// While paused, nothing new is added to clipboard history.
    pub clipboard_paused: bool,
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
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
    }
}
