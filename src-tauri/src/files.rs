//! Where Grandium keeps its files, and reading and writing JSON there.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// `%APPDATA%\Grandium`, if Windows can tell us where that is.
pub fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().data_dir().ok().map(|dir| dir.join("Grandium"))
}

/// Reads a JSON file, falling back to the default if it's missing or
/// unreadable.
pub fn load_json<T: DeserializeOwned + Default>(path: Option<&Path>) -> T {
    path.and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes JSON to a temporary file first, so a crash mid-write can't leave
/// a half-written file behind.
pub fn save_json(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    write_atomically(path, &serde_json::to_vec(value)?)
}

pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    fs::write(&temp, bytes)?;
    fs::rename(temp, path)
}
