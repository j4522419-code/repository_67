//! User settings, saved as JSON in `%APPDATA%\Grandium\settings.json`.

use serde::{Deserialize, Serialize};

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
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
    fn loads_partial_and_older_files() {
        // Older versions saved which keys opened Grandium; that's ignored now.
        let settings: Settings =
            serde_json::from_str(r#"{"openWith":"windowsKey","setupDone":true}"#).unwrap();
        assert!(settings.setup_done);
        assert!(!settings.clipboard_paused);
        assert_eq!(settings.search_engine, "g");
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
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
    }
}
