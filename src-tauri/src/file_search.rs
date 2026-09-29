//! File search: indexes Desktop, Documents, Downloads and any folders added
//! in settings, in the background, then keeps the index current as files
//! come and go.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, RwLock, RwLockReadGuard};
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
    /// The folders searched.
    roots: RwLock<Vec<Root>>,
    watching: Mutex<Watching>,
}

#[derive(Default)]
struct Watching {
    folders: Vec<PathBuf>,
    /// Where the watchers send changes; also takes requests to index
    /// everything again.
    events: Option<Sender<FolderEvent>>,
}

impl FileSearch {
    /// The index as it is now: empty until the first scan finishes.
    pub fn index(&self) -> RwLockReadGuard<'_, FileIndex> {
        self.index.read().unwrap()
    }

    /// Searches these folders from now on, watching any that are new.
    /// Returns whether there's a thread following the changes (there is
    /// once `start` has run).
    fn use_roots(&self, roots: Vec<Root>) -> bool {
        let mut watching = self.watching.lock().unwrap();
        let Some(events) = watching.events.clone() else {
            return false;
        };
        for root in &roots {
            if !watching.folders.contains(&root.path)
                && platform::watch::watch_folder(root.path.clone(), events.clone()).is_ok()
            {
                watching.folders.push(root.path.clone());
            }
        }
        *self.roots.write().unwrap() = roots;
        true
    }
}

/// Desktop, Documents and Downloads, then `extra` folders, named after
/// themselves.
fn roots_for(extra: &[String]) -> Vec<Root> {
    let mut roots: Vec<Root> = platform::user_folders()
        .into_iter()
        .map(|(label, path)| Root {
            path,
            label: label.into(),
        })
        .collect();
    roots.extend(extra.iter().map(|folder| {
        let path = PathBuf::from(folder);
        let label = path.file_name().map_or_else(
            || folder.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        Root { path, label }
    }));
    roots
}

/// Starts watching and indexing the folders, then keeps following the
/// changes, all on a background thread.
pub fn start(app: &AppHandle, extra_folders: &[String]) {
    let (events, changes) = mpsc::channel();
    app.state::<FileSearch>().watching.lock().unwrap().events = Some(events);
    let roots = roots_for(extra_folders);
    let app = app.clone();
    std::thread::spawn(move || {
        let search = app.state::<FileSearch>();
        // Watch first, so nothing that changes during the scan is missed:
        // those changes wait in the channel until it's done.
        search.use_roots(roots);
        scan(&search);
        follow(&search, &changes);
    });
}

/// After settings change which extra folders are searched.
pub fn set_extra_folders(app: &AppHandle, extra_folders: &[String]) {
    let search = app.state::<FileSearch>();
    if search.use_roots(roots_for(extra_folders)) {
        // Index again on the thread following changes, so the new index
        // can't be overwritten by one being built from the old folders.
        if let Some(events) = &search.watching.lock().unwrap().events {
            let _ = events.send(FolderEvent::Overflow);
        }
    }
}

fn scan(search: &FileSearch) {
    let roots = search.roots.read().unwrap().clone();
    let fresh = FileIndex::scan(roots);
    *search.index.write().unwrap() = fresh;
}

fn follow(search: &FileSearch, changes: &Receiver<FolderEvent>) {
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
            scan(search);
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
        let extra = std::env::temp_dir().join(format!("grandium-extra-{}", std::process::id()));
        std::fs::create_dir_all(&extra).unwrap();
        std::fs::write(extra.join("found me.txt"), "x").unwrap();
        let roots = roots_for(&[extra.to_string_lossy().into_owned()]);
        let index = FileIndex::scan(roots);
        println!(
            "{} files and folders in {:?}",
            index.len(),
            started.elapsed()
        );
        for (path, _) in index.iter().take(20) {
            println!("  {} ({})", path.display(), index.location(path));
        }
        let found = extra.join("found me.txt");
        let label = extra.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(index.location(&found), label, "extra folders are searched");
        let _ = std::fs::remove_dir_all(&extra);
    }
}
