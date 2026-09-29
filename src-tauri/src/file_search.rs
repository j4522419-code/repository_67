//! File search: indexes Desktop, Documents and Downloads in the background,
//! then keeps the index current as files come and go.

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver};
use std::sync::{RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

use grandium_core::file_index::{FileIndex, Root};
use tauri::{AppHandle, Manager};

use crate::platform::{self, watch::FolderEvent};

/// Changes arriving this close together are handled together, so a file
/// being written to isn't looked at again for every write.
const SETTLE: Duration = Duration::from_millis(300);
/// ...but a steady stream of changes still gets handled this often.
const MAX_WAIT: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct FileSearch {
    index: RwLock<FileIndex>,
}

impl FileSearch {
    /// The index as it is now: empty until the first scan finishes.
    pub fn index(&self) -> RwLockReadGuard<'_, FileIndex> {
        self.index.read().unwrap()
    }
}

/// Starts watching the folders, indexes them, and then keeps following
/// the changes, all on a background thread.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let roots: Vec<Root> = platform::user_folders()
            .into_iter()
            .map(|(label, path)| Root {
                path,
                label: label.into(),
            })
            .collect();
        // Watch first, so nothing that changes during the scan is missed:
        // those changes wait in the channel until it's done.
        let (events, changes) = mpsc::channel();
        for root in &roots {
            let _ = platform::watch::watch_folder(root.path.clone(), events.clone());
        }
        drop(events);

        let search = app.state::<FileSearch>();
        *search.index.write().unwrap() = FileIndex::scan(roots.clone());
        follow(&search, &roots, &changes);
    });
}

fn follow(search: &FileSearch, roots: &[Root], changes: &Receiver<FolderEvent>) {
    while let Ok(first) = changes.recv() {
        let mut batch = vec![first];
        let started = Instant::now();
        while started.elapsed() < MAX_WAIT {
            match changes.recv_timeout(SETTLE) {
                Ok(event) => batch.push(event),
                Err(_) => break,
            }
        }

        if batch.contains(&FolderEvent::Overflow) {
            let fresh = FileIndex::scan(roots.to_vec());
            *search.index.write().unwrap() = fresh;
            continue;
        }
        // In the order they happened: a new folder has to be added before
        // what's in it.
        let mut seen = HashSet::new();
        for event in batch {
            let FolderEvent::Changed(path) = event else {
                continue;
            };
            if seen.insert(path.clone()) {
                let change = search.index().inspect(&path);
                search.index.write().unwrap().apply(change);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Indexes the real folders of the machine running the tests and
    /// prints what it finds.
    #[test]
    fn indexes_the_user_folders() {
        let folders = platform::user_folders();
        for (name, path) in &folders {
            println!("{name}: {}", path.display());
        }
        assert_eq!(folders.len(), 3, "Desktop, Documents and Downloads");

        let started = Instant::now();
        let roots = folders
            .into_iter()
            .map(|(label, path)| Root {
                path,
                label: label.into(),
            })
            .collect();
        let index = FileIndex::scan(roots);
        println!(
            "{} files and folders in {:?}",
            index.len(),
            started.elapsed()
        );
        for (path, _) in index.iter().take(20) {
            println!("  {} ({})", path.display(), index.location(path));
        }
    }
}
