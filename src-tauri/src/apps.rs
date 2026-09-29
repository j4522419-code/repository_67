//! The list of installed apps, re-read in the background now and then so
//! new installs (and uninstalls) show up.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use grandium_core::apps;
use grandium_core::uninstall::{self, InstalledProgram};
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
    /// How to uninstall it, when that's known for sure.
    pub uninstall: Option<Uninstall>,
}

#[derive(Clone)]
pub enum Uninstall {
    /// A program, by its name in the installed programs list (which can
    /// differ from the app's: "Git Bash" is part of "Git") and its own
    /// uninstall command.
    Program { name: String, command: String },
    /// A Store package, by its full name.
    Package(String),
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

    /// Makes the next `refresh_if_stale` re-read the list, e.g. after an
    /// uninstall.
    pub fn mark_stale(&self) {
        *self.refreshed_at.lock().unwrap() = None;
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
            platform::installed_apps().map(|apps| {
                let programs = platform::installed_programs();
                let packages = platform::removable_packages();
                to_apps(apps, &programs, &packages)
            })
        };
        let index = app.state::<AppIndex>();
        // On failure keep the old list; the next refresh tries again.
        if let Ok(found) = found {
            *index.apps.write().unwrap() = Arc::new(found);
            *index.refreshed_at.lock().unwrap() = Some(Instant::now());
        }
        index.refreshing.store(false, Ordering::SeqCst);
    });
}

fn to_apps(
    found: Vec<platform::ShellApp>,
    programs: &[InstalledProgram],
    packages: &HashMap<String, String>,
) -> Vec<App> {
    let mut seen = HashSet::new();
    found
        .into_iter()
        .map(|app| {
            let target = app.target.as_deref().and_then(apps::clean_target);
            let uninstall =
                match uninstall::package_family(&app.id) {
                    Some(family) => packages.get(family).cloned().map(Uninstall::Package),
                    None => uninstall::find_program(programs, &app.name, target.as_deref()).map(
                        |program| Uninstall::Program {
                            name: program.name.clone(),
                            command: program.uninstall_command.clone(),
                        },
                    ),
                };
            App {
                key: format!("app:{}", app.id),
                id: app.id,
                name: app.name,
                target,
                uninstall,
            }
        })
        .filter(|app| apps::is_app(&app.name, app.target.as_deref()))
        // The same app can be listed twice (e.g. shortcuts for all users
        // and for just you).
        .filter(|app| seen.insert((app.name.to_lowercase(), app.target.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs against the real apps and programs of the machine running the
    /// tests (a GitHub Windows runner in CI) and prints what it works out.
    #[test]
    fn works_out_uninstallers_for_real_apps() {
        let _com = Com::init();
        let programs = platform::installed_programs();
        let packages = platform::removable_packages();
        println!(
            "{} installed programs, {} removable Store packages",
            programs.len(),
            packages.len()
        );
        let apps = to_apps(
            platform::installed_apps().expect("couldn't read AppsFolder"),
            &programs,
            &packages,
        );
        let removable: Vec<&App> = apps.iter().filter(|a| a.uninstall.is_some()).collect();
        println!(
            "{} of {} apps can be uninstalled:",
            removable.len(),
            apps.len()
        );
        for app in &removable {
            let how = match app.uninstall.as_ref().unwrap() {
                Uninstall::Program { name, command } => format!("{name}: {command}"),
                Uninstall::Package(name) => format!("Store package {name}"),
            };
            println!("{:<40} | {how}", app.name);
        }

        assert!(!programs.is_empty());
        let in_windows = |app: &&App| {
            app.target
                .as_deref()
                .is_some_and(|t| t.to_lowercase().starts_with(r"c:\windows\"))
        };
        assert!(
            apps.iter()
                .filter(in_windows)
                .all(|app| app.uninstall.is_none()),
            "parts of Windows must never be offered for uninstalling"
        );
        assert!(
            apps.iter().all(|app| !matches!(
                &app.uninstall,
                Some(Uninstall::Package(name)) if name.starts_with("Microsoft")
            )),
            "Microsoft's packages must never be offered for uninstalling"
        );
    }
}
