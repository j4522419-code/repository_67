//! Platform-independent logic for Grandium. Nothing in this crate touches
//! Windows APIs, so it builds and tests on any OS.

pub mod layout;

/// Hotkey used when the user hasn't picked one.
pub const DEFAULT_HOTKEY: &str = "Alt+Space";
