//! Platform-independent logic for Grandium. Nothing in this crate touches
//! Windows APIs, so it builds and tests on any OS.

pub mod apps;
pub mod calc;
pub mod clipboard;
pub mod file_index;
pub mod icon;
pub mod image;
pub mod layout;
pub mod notes;
pub mod query;
pub mod rank;
pub mod run;
pub mod settings;
pub mod snippets;
pub mod system;
pub mod uninstall;
pub mod usage;
pub mod web;

/// Hotkey used when the user hasn't picked one.
pub const DEFAULT_HOTKEY: &str = "Alt+Space";
