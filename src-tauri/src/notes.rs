//! Quick notes, kept as text files in `Documents\Grandium\Notes`.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use grandium_core::notes::Notes;
use tauri::AppHandle;

use crate::{files, platform};

/// Notes can be changed in any editor, so the folder is looked at again
/// when it's been this long.
const REFRESH_AFTER: Duration = Duration::from_secs(2);

pub struct NoteStore {
    notes: Mutex<Notes>,
    refreshed_at: Mutex<Instant>,
}

impl NoteStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            notes: Mutex::new(Notes::new(dir)),
            refreshed_at: Mutex::new(Instant::now()),
        }
    }

    /// Reads the notes, catching up with the folder first if needed.
    pub fn with<R>(&self, read: impl FnOnce(&Notes) -> R) -> R {
        let mut notes = self.notes.lock().unwrap();
        let mut refreshed_at = self.refreshed_at.lock().unwrap();
        if refreshed_at.elapsed() >= REFRESH_AFTER {
            notes.refresh();
            *refreshed_at = Instant::now();
        }
        read(&notes)
    }

    pub fn dir(&self) -> PathBuf {
        self.notes.lock().unwrap().dir().to_path_buf()
    }

    pub fn create(&self, text: &str) -> Result<PathBuf, String> {
        self.notes
            .lock()
            .unwrap()
            .create(text)
            .map_err(|e| format!("Couldn't save the note: {e}"))
    }

    /// Catches up with the folder now, after changing it.
    pub fn refresh(&self) {
        self.notes.lock().unwrap().refresh();
        *self.refreshed_at.lock().unwrap() = Instant::now();
    }
}

/// `Documents\Grandium\Notes`, or next to Grandium's other files if
/// Windows can't say where Documents is.
pub fn notes_dir(app: &AppHandle) -> PathBuf {
    let documents = platform::user_folders()
        .into_iter()
        .find(|(name, _)| *name == "Documents")
        .map(|(_, path)| path.join("Grandium"));
    documents
        .or_else(|| files::data_dir(app))
        .unwrap_or_else(std::env::temp_dir)
        .join("Notes")
}
