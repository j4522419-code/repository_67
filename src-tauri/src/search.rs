//! What the search box calls: ranking apps for what was typed, and running
//! the action picked for a result.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use grandium_core::apps::{can_run_as_admin, has_file_location};
use grandium_core::rank::Ranker;
use grandium_core::usage::Usage;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::apps::{App, AppIndex};
use crate::launcher;
use crate::platform::{self, Com, Launch};

const MAX_RESULTS: usize = 8;

pub struct SearchState {
    ranker: Mutex<Ranker>,
    usage: Mutex<Usage>,
    /// Where usage is saved; `None` if there's nowhere to save it.
    usage_file: Option<PathBuf>,
}

impl SearchState {
    /// Loads usage history from `usage_file`, starting fresh if it's
    /// missing or unreadable.
    pub fn load(usage_file: Option<PathBuf>) -> Self {
        let usage = usage_file
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            ranker: Mutex::default(),
            usage: Mutex::new(usage),
            usage_file,
        }
    }

    fn record_use(&self, key: &str) {
        let mut usage = self.usage.lock().unwrap();
        usage.record(key, unix_now());
        if let Some(path) = &self.usage_file {
            // Losing usage history isn't worth bothering anyone about.
            let _ = save_json(path, &*usage);
        }
    }
}

/// Writes to a temporary file first, so a crash mid-write can't leave a
/// half-written file behind.
fn save_json(path: &PathBuf, value: &impl Serialize) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec(value)?)?;
    fs::rename(temp, path)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    id: String,
    title: String,
    kind: &'static str,
    /// What to pass to the `icon` URL scheme.
    icon: Option<String>,
    /// `[start, end)` ranges of `title` to highlight, in UTF-16 units.
    highlights: Vec<[u32; 2]>,
    /// The first action is what Enter does.
    actions: Vec<ResultAction>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultAction {
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
}

const OPEN: ResultAction = ResultAction {
    id: "open",
    label: "Open",
    shortcut: Some("Enter"),
};
const OPEN_LOCATION: ResultAction = ResultAction {
    id: "openLocation",
    label: "Open file location",
    shortcut: Some("Ctrl+Enter"),
};
const RUN_AS_ADMIN: ResultAction = ResultAction {
    id: "runAsAdmin",
    label: "Run as administrator",
    shortcut: Some("Ctrl+Shift+Enter"),
};

fn app_result(app: &App, highlights: Vec<[u32; 2]>) -> SearchResult {
    let target = app.target.as_deref();
    let mut actions = vec![OPEN];
    if has_file_location(target) {
        actions.push(OPEN_LOCATION);
    }
    if can_run_as_admin(target) {
        actions.push(RUN_AS_ADMIN);
    }
    SearchResult {
        id: app.key.clone(),
        title: app.name.clone(),
        kind: "App",
        icon: Some(app.id.clone()),
        highlights,
        actions,
    }
}

#[tauri::command]
pub fn search(
    query: String,
    index: State<AppIndex>,
    state: State<SearchState>,
) -> Vec<SearchResult> {
    let apps = index.apps();
    let usage = state.usage.lock().unwrap();
    let now = unix_now();
    let ranked = state.ranker.lock().unwrap().rank(
        &query,
        &apps,
        |app| &app.name,
        |app| usage.boost(&app.key, now),
        MAX_RESULTS,
    );
    ranked
        .into_iter()
        .map(|r| app_result(&apps[r.index], r.highlights))
        .collect()
}

#[tauri::command]
pub async fn run_action(app: AppHandle, id: String, action: String) -> Result<(), String> {
    let target = app
        .state::<AppIndex>()
        .find(&id)
        .ok_or("This app doesn't seem to be installed anymore.")?;

    let run = move || -> Result<(), String> {
        let _com = Com::init();
        match action.as_str() {
            "open" => platform::launch(&target.id, Launch::Normal),
            "runAsAdmin" => platform::launch(&target.id, Launch::AsAdmin),
            "openLocation" => match &target.target {
                Some(path) => platform::show_in_explorer(path),
                None => Err("This app has no file location.".into()),
            },
            other => Err(format!("Unknown action: {other}")),
        }
    };
    // Shell calls can block (the administrator prompt waits for an answer),
    // so they run off the UI thread.
    tauri::async_runtime::spawn_blocking(run)
        .await
        .map_err(|e| e.to_string())??;

    app.state::<SearchState>().record_use(&id);
    launcher::hide(&app);
    Ok(())
}
