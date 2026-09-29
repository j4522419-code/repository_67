//! The list of installed apps, re-read in the background now and then so
//! new installs show up.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use grandium_core::apps;
use tauri::{AppHandle, Manager};

use crate::platform::{self, Com};

/// Re-read the app list at most this often.
const REFRESH_AFTER: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub struct App {
    /// The ID the shell uses to start the app (see [`platform::ShellApp`]).
    pub id: String,
    pub name: String,
    pub target: Option<String>,
    /// Identifies the app in search results and usage history.
    pub key: String,
}

#[derive(Default)]
pub struct AppIndex {
    apps: RwLock<Arc<Vec<App>>>,
    refreshing: AtomicBool,
    refreshed_at: Mutex<Option<Instant>>,
}

impl AppIndex {
    pub fn apps(&self) -> Arc<Vec<App>> {
        self.apps.read().unwrap().clone()
    }

    pub fn find(&self, key: &str) -> Option<App> {
        self.apps().iter().find(|app| app.key == key).cloned()
    }
}

/// Re-reads the installed apps on a background thread, unless that
/// happened in the last couple of minutes.
pub fn refresh_if_stale(app: &AppHandle) {
    let index = app.state::<AppIndex>();
    let fresh = index
        .refreshed_at
        .lock()
        .unwrap()
        .is_some_and(|at| at.elapsed() < REFRESH_AFTER);
    if fresh || index.refreshing.swap(true, Ordering::SeqCst) {
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let found = {
            let _com = Com::init();
            platform::installed_apps()
        };
        let index = app.state::<AppIndex>();
        // On failure keep the old list; the next refresh tries again.
        if let Ok(found) = found {
            *index.apps.write().unwrap() = Arc::new(to_apps(found));
            *index.refreshed_at.lock().unwrap() = Some(Instant::now());
        }
        index.refreshing.store(false, Ordering::SeqCst);
    });
}

fn to_apps(found: Vec<platform::ShellApp>) -> Vec<App> {
    let mut seen = HashSet::new();
    found
        .into_iter()
        .map(|app| App {
            key: format!("app:{}", app.id),
            target: app.target.as_deref().and_then(apps::clean_target),
            id: app.id,
            name: app.name,
        })
        .filter(|app| apps::is_app(&app.name, app.target.as_deref()))
        // The same app can be listed twice (e.g. shortcuts for all users
        // and for just you).
        .filter(|app| seen.insert((app.name.to_lowercase(), app.target.clone())))
        .collect()
}
