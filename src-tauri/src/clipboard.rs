//! Clipboard history: recording what's copied, keeping it encrypted on
//! disk, and putting entries back on the clipboard.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use grandium_core::clipboard::{Content, Entry, History};
use grandium_core::image::{self, ClipImage};
use tauri::http::Request;
use tauri::{AppHandle, Manager, UriSchemeResponder};
use windows::Win32::Foundation::HWND;

use crate::files;
use crate::icons::{not_found, png_response};
use crate::platform::clipboard::{self as system_clipboard, Copied};
use crate::platform::protect;
use crate::settings::SettingsStore;

/// URL scheme the UI loads clipboard pictures from: `clip://<entry id>`.
pub const SCHEME: &str = "clip";

pub struct ClipboardStore {
    history: Mutex<History>,
    /// `%APPDATA%\Grandium\clipboard`; `None` keeps history in memory only.
    dir: Option<PathBuf>,
}

impl ClipboardStore {
    pub fn load(dir: Option<PathBuf>) -> Self {
        let history = dir
            .as_ref()
            .and_then(|dir| fs::read(dir.join("history.dat")).ok())
            .and_then(|sealed| protect::unprotect(&sealed).ok())
            .and_then(|json| serde_json::from_slice(&json).ok())
            .unwrap_or_default();
        let store = Self {
            history: Mutex::new(history),
            dir,
        };
        store.update(|history| history.prune(now()));
        store
    }

    /// Reads the history without changing it.
    pub fn with<R>(&self, read: impl FnOnce(&History) -> R) -> R {
        read(&self.history.lock().unwrap())
    }

    fn record(&self, copied: Copied) {
        let content = match copied {
            Copied::Text(text) => Content::Text { text },
            Copied::Bitmap(bitmap) => match image::from_dib(&bitmap) {
                Ok(picture) => self.keep_picture(picture),
                Err(_) => return,
            },
            Copied::Png(png) => match image::from_png(&png) {
                Ok(picture) => self.keep_picture(picture),
                Err(_) => return,
            },
        };
        self.update(|history| history.add(content, now()));
    }

    /// Saves a picture (encrypted) and returns the entry content for it.
    fn keep_picture(&self, picture: ClipImage) -> Content {
        if let Some(path) = self.picture_path(&picture.hash) {
            if !path.exists() {
                if let Ok(sealed) = protect::protect(&picture.png) {
                    let _ = files::write_atomically(&path, &sealed);
                }
            }
        }
        Content::Image {
            hash: picture.hash,
            width: picture.width,
            height: picture.height,
        }
    }

    pub fn set_pinned(&self, id: u64, pinned: bool) {
        self.update(|history| {
            history.set_pinned(id, pinned);
            Vec::new()
        });
    }

    pub fn delete(&self, id: u64) {
        self.update(|history| history.remove(id).into_iter().collect());
    }

    /// Forgets everything except pinned entries.
    pub fn clear(&self) {
        self.update(History::clear);
    }

    /// The PNG of a picture entry.
    pub fn picture(&self, id: u64) -> Option<Vec<u8>> {
        let hash = self.with(|history| match &history.get(id)?.content {
            Content::Image { hash, .. } => Some(hash.clone()),
            Content::Text { .. } => None,
        })?;
        let sealed = fs::read(self.picture_path(&hash)?).ok()?;
        protect::unprotect(&sealed).ok()
    }

    /// Puts an entry back on the clipboard.
    pub fn copy(&self, id: u64, owner: HWND) -> Result<(), String> {
        let content = self
            .with(|history| history.get(id).map(|entry| entry.content.clone()))
            .ok_or("That's no longer in your clipboard history.")?;
        match content {
            Content::Text { text } => system_clipboard::copy_text(owner, &text),
            Content::Image { .. } => {
                let png = self.picture(id).ok_or("That picture is missing.")?;
                let bitmap = image::to_dib(&png)?;
                system_clipboard::copy_image(owner, &bitmap, &png)
            }
        }
    }

    /// Changes the history, deletes pictures no entry uses anymore, and
    /// saves.
    fn update(&self, change: impl FnOnce(&mut History) -> Vec<Entry>) {
        let mut history = self.history.lock().unwrap();
        let removed = change(&mut history);
        for entry in removed {
            if let Content::Image { hash, .. } = entry.content {
                let still_used = history
                    .entries()
                    .iter()
                    .any(|e| matches!(&e.content, Content::Image { hash: h, .. } if *h == hash));
                if let (false, Some(path)) = (still_used, self.picture_path(&hash)) {
                    let _ = fs::remove_file(path);
                }
            }
        }
        if let Some(dir) = &self.dir {
            // A failed save only means recent copies won't survive a restart.
            let sealed = serde_json::to_vec(&*history)
                .map_err(|e| e.to_string())
                .and_then(|json| protect::protect(&json));
            if let Ok(sealed) = sealed {
                let _ = files::write_atomically(&dir.join("history.dat"), &sealed);
            }
        }
    }

    fn picture_path(&self, hash: &str) -> Option<PathBuf> {
        // Hashes are hex; anything else must not become part of a path.
        if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(
            self.dir
                .as_ref()?
                .join("pictures")
                .join(format!("{hash}.dat")),
        )
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Starts recording copies (unless paused in settings). If watching the
/// clipboard fails, Grandium works as before, with an empty history.
pub fn start_recording(app: &AppHandle) {
    let app = app.clone();
    let _ = system_clipboard::watch(move |copied| {
        if !app.state::<SettingsStore>().get().clipboard_paused {
            app.state::<ClipboardStore>().record(copied);
        }
    });
}

/// Serves `clip://<entry id>` with that entry's picture.
pub fn serve_picture(app: &AppHandle, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = app.clone();
    std::thread::spawn(move || {
        let id = request.uri().path().trim_start_matches('/').parse().ok();
        let picture = id.and_then(|id| app.state::<ClipboardStore>().picture(id));
        responder.respond(match picture {
            Some(png) => png_response(png),
            None => not_found(),
        });
    });
}
