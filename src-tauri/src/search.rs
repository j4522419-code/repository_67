//! What the search box calls: finding results for what was typed, and
//! running the action picked for a result.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use grandium_core::apps::{can_run_as_admin, has_file_location};
use grandium_core::query::{self, Query};
use grandium_core::rank::Ranker;
use grandium_core::system::{self, Command};
use grandium_core::usage::Usage;
use grandium_core::{calc, web};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};

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
    /// `<kind>:<details>`, e.g. `app:<app id>` or `calc:14400`.
    id: String,
    title: String,
    kind: &'static str,
    /// An app icon: what to pass to the `icon` URL scheme.
    icon: Option<String>,
    /// A built-in icon, by name, for results that aren't apps.
    glyph: Option<&'static str>,
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
    /// When set, the UI asks this question before running the action.
    confirm: Option<&'static str>,
}

const fn action(id: &'static str, label: &'static str, shortcut: &'static str) -> ResultAction {
    ResultAction {
        id,
        label,
        shortcut: Some(shortcut),
        confirm: None,
    }
}

fn app_result(app: &App, highlights: Vec<[u32; 2]>) -> SearchResult {
    let target = app.target.as_deref();
    let mut actions = vec![action("open", "Open", "Enter")];
    if has_file_location(target) {
        actions.push(action("openLocation", "Open file location", "Ctrl+Enter"));
    }
    if can_run_as_admin(target) {
        actions.push(action(
            "runAsAdmin",
            "Run as administrator",
            "Ctrl+Shift+Enter",
        ));
    }
    SearchResult {
        id: app.key.clone(),
        title: app.name.clone(),
        kind: "App",
        icon: Some(app.id.clone()),
        glyph: None,
        highlights,
        actions,
    }
}

fn calc_result(value: f64) -> SearchResult {
    let formatted = calc::format(value);
    SearchResult {
        id: format!("calc:{}", formatted.copy),
        title: format!("= {}", formatted.display),
        kind: "Calculator",
        icon: None,
        glyph: Some("calculator"),
        highlights: Vec::new(),
        actions: vec![action("copy", "Copy result", "Enter")],
    }
}

fn web_result(engine: &web::Engine, text: &str) -> SearchResult {
    let title = if text.is_empty() {
        format!("Search {}", engine.name)
    } else {
        format!("Search {} for “{text}”", engine.name)
    };
    SearchResult {
        id: format!("web:{}:{text}", engine.keyword),
        title,
        kind: "Web",
        icon: None,
        glyph: Some("web"),
        highlights: Vec::new(),
        actions: vec![action("open", "Search", "Enter")],
    }
}

fn system_key(command: &Command) -> String {
    format!("sys:{}", command.id)
}

fn system_result(command: &'static Command, highlights: Vec<[u32; 2]>) -> SearchResult {
    SearchResult {
        id: system_key(command),
        title: command.name.to_string(),
        kind: "System",
        icon: None,
        glyph: Some(command.id),
        highlights,
        actions: vec![ResultAction {
            confirm: command.confirm,
            ..action("run", command.name, "Enter")
        }],
    }
}

#[tauri::command]
pub fn search(
    query: String,
    index: State<AppIndex>,
    state: State<SearchState>,
) -> Vec<SearchResult> {
    let usage = state.usage.lock().unwrap();
    let mut ranker = state.ranker.lock().unwrap();
    let now = unix_now();
    let boost = |key: &str| usage.boost(key, now);
    let system_matches = |ranker: &mut Ranker, text: &str| {
        system::search(ranker, text, |command| boost(&system_key(command)))
            .into_iter()
            .map(|m| (m.score, system_result(m.command, m.highlights)))
    };

    match query::parse(&query) {
        Query::Calculator(text) => calc::evaluate(text, false)
            .map(calc_result)
            .into_iter()
            .collect(),
        Query::Web(engine, text) => vec![web_result(engine, text)],
        Query::System("") => system::COMMANDS
            .iter()
            .map(|command| system_result(command, Vec::new()))
            .collect(),
        Query::System(text) => system_matches(&mut ranker, text)
            .map(|(_, result)| result)
            .collect(),
        Query::Everything("") => Vec::new(),
        Query::Everything(text) => {
            let calc = calc::evaluate(text, true).map(calc_result);
            let apps = index.apps();
            let mut ranked: Vec<(f64, SearchResult)> = ranker
                .rank(
                    text,
                    &apps,
                    |app| &app.name,
                    |app| boost(&app.key),
                    MAX_RESULTS,
                )
                .into_iter()
                .map(|r| (r.score, app_result(&apps[r.index], r.highlights)))
                .collect();
            ranked.extend(system_matches(&mut ranker, text));
            ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
            // Leave room for the answer on top and the web search below.
            ranked.truncate(MAX_RESULTS - 1 - usize::from(calc.is_some()));

            calc.into_iter()
                .chain(ranked.into_iter().map(|(_, result)| result))
                .chain(Some(web_result(web::DEFAULT, text)))
                .collect()
        }
    }
}

#[tauri::command]
pub async fn run_action(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    action: String,
) -> Result<(), String> {
    let (kind, details) = id.split_once(':').ok_or("Unknown result.")?;
    match kind {
        "app" => run_app_action(&app, &id, &action).await?,
        "sys" => run_system_command(&app, details).await?,
        "calc" => {
            let owner = window.hwnd().map_err(|e| e.to_string())?;
            platform::copy_text(owner, details)?;
        }
        "web" => {
            let (keyword, text) = details.split_once(':').ok_or("Unknown search.")?;
            let url = web::engine(keyword)
                .ok_or("Unknown search engine.")?
                .search_url(text);
            in_background(move || platform::open_url(&url)).await?;
        }
        _ => return Err("Unknown result.".into()),
    }
    launcher::hide(&app);
    Ok(())
}

async fn run_app_action(app: &AppHandle, id: &str, action: &str) -> Result<(), String> {
    let target = app
        .state::<AppIndex>()
        .find(id)
        .ok_or("This app doesn't seem to be installed anymore.")?;
    let action = action.to_string();
    in_background(move || match action.as_str() {
        "open" => platform::launch(&target.id, Launch::Normal),
        "runAsAdmin" => platform::launch(&target.id, Launch::AsAdmin),
        "openLocation" => match &target.target {
            Some(path) => platform::show_in_explorer(path),
            None => Err("This app has no file location.".into()),
        },
        other => Err(format!("Unknown action: {other}")),
    })
    .await?;
    app.state::<SearchState>().record_use(id);
    Ok(())
}

async fn run_system_command(app: &AppHandle, id: &str) -> Result<(), String> {
    let command = system::command(id).ok_or("Unknown command.")?;
    app.state::<SearchState>().record_use(&system_key(command));
    // Get out of the way first, so the PC doesn't wake up or unlock to an
    // open launcher.
    launcher::hide(app);
    let result = in_background(move || platform::run_system_command(command.id)).await;
    if result.is_err() {
        launcher::show(app);
    }
    result
}

/// Runs shell work off the UI thread, with COM ready. Some calls block for
/// a while: the administrator prompt waits for an answer, and sleep only
/// returns once the PC wakes up.
async fn in_background<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _com = Com::init();
        work()
    })
    .await
    .map_err(|e| e.to_string())?
}
