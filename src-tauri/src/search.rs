//! What the search box calls: finding results for what was typed, and
//! running the action picked for a result.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use grandium_core::apps::{can_run_as_admin, has_file_location};
use grandium_core::clipboard::{self as history, Content, Entry};
use grandium_core::query::{self, Query, SlashCommand};
use grandium_core::rank::Ranker;
use grandium_core::system::{self, Command};
use grandium_core::usage::Usage;
use grandium_core::{calc, run, uninstall, web};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::apps::{App, AppIndex, Uninstall};
use crate::clipboard::{self, ClipboardStore};
use crate::platform::{self, Com, Launch};
use crate::settings::SettingsStore;
use crate::{files, icons, launcher, tray};

const MAX_RESULTS: usize = 8;
/// Clipboard history scrolls, so it can show more.
const MAX_CLIPBOARD_RESULTS: usize = 50;

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
        Self {
            ranker: Mutex::default(),
            usage: Mutex::new(files::load_json(usage_file.as_deref())),
            usage_file,
        }
    }

    fn record_use(&self, key: &str) {
        let mut usage = self.usage.lock().unwrap();
        usage.record(key, unix_now());
        if let Some(path) = &self.usage_file {
            // Losing usage history isn't worth bothering anyone about.
            let _ = files::save_json(path, &*usage);
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// `<kind>:<details>`, e.g. `app:<app id>` or `calc:14400`.
    id: String,
    title: String,
    /// Smaller text after the title, like the folder `%temp%` stands for.
    subtitle: Option<String>,
    kind: &'static str,
    /// A picture to show as the icon: an app's icon or a copied image.
    icon: Option<IconRef>,
    /// A built-in icon, by name, for results that aren't apps.
    glyph: Option<&'static str>,
    /// `[start, end)` ranges of `title` to highlight, in UTF-16 units.
    highlights: Vec<[u32; 2]>,
    /// The first action is what Enter does.
    actions: Vec<ResultAction>,
    /// For slash commands: the text to put in the search box when picked.
    fill: Option<String>,
    /// Shown below the list while the result is selected.
    preview: Option<Preview>,
}

/// A picture the UI loads through one of Grandium's URL schemes.
#[derive(Clone, Serialize)]
pub struct IconRef {
    scheme: &'static str,
    key: String,
}

#[derive(Serialize)]
pub struct Preview {
    text: Option<String>,
    image: Option<IconRef>,
}

/// What happened after running an action.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// It's done and the launcher closed.
    Done,
    /// The launcher stays open; search again to show the change.
    Refresh,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultAction {
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
    /// When set, the UI asks this question before running the action.
    confirm: Option<String>,
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
    if let Some(how) = &app.uninstall {
        let question = match how {
            Uninstall::Program { name, .. } if name.eq_ignore_ascii_case(&app.name) => {
                format!("Uninstall {name}? This opens its uninstaller.")
            }
            // Say what really goes: uninstalling "Git Bash" removes Git.
            Uninstall::Program { name, .. } => format!(
                "Uninstall {name}? {} is part of it. This opens its uninstaller.",
                app.name
            ),
            Uninstall::Package(_) => {
                format!("Uninstall {}? It will be removed from this PC.", app.name)
            }
        };
        actions.push(ResultAction {
            id: "uninstall",
            label: "Uninstall",
            // No shortcut, so it can't happen by accident.
            shortcut: None,
            confirm: Some(question),
        });
    }
    SearchResult {
        id: app.key.clone(),
        title: app.name.clone(),
        kind: "App",
        icon: Some(IconRef {
            scheme: icons::SCHEME,
            key: app.id.clone(),
        }),
        glyph: None,
        highlights,
        actions,
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
            confirm: command.confirm.map(String::from),
            ..action("run", command.name, "Enter")
        }],
        ..Default::default()
    }
}

/// Something typed like into the Run box: a path, `%temp%`, a program.
/// `resolved` is what it stands for, shown under the title.
fn run_result(text: &str, resolved: Option<String>, glyph: &'static str) -> SearchResult {
    SearchResult {
        id: format!("run:{text}"),
        title: text.to_string(),
        subtitle: resolved.filter(|resolved| resolved != text),
        kind: "Run",
        glyph: Some(glyph),
        actions: vec![
            action("open", "Open", "Enter"),
            action("runAsAdmin", "Run as administrator", "Ctrl+Shift+Enter"),
        ],
        ..Default::default()
    }
}

fn location_result(text: &str) -> SearchResult {
    let glyph = if text.contains("://") {
        "web"
    } else {
        "folder"
    };
    run_result(text, Some(platform::expand_env(text)), glyph)
}

fn slash_command_result(command: &SlashCommand) -> SearchResult {
    let aliases: Vec<String> = command.aliases.iter().map(|a| format!("/{a}")).collect();
    let subtitle = if aliases.is_empty() {
        command.description.to_string()
    } else {
        format!("{} · also {}", command.description, aliases.join(", "))
    };
    SearchResult {
        id: format!("cmd:{}", command.name),
        title: format!("/{}", command.name),
        subtitle: Some(subtitle),
        kind: "Command",
        glyph: Some(command.glyph),
        actions: vec![action("fill", "Choose", "Enter")],
        fill: Some(format!("/{} ", command.name)),
        ..Default::default()
    }
}

fn clipboard_result(entry: &Entry, highlights: Vec<[u32; 2]>, now: u64) -> SearchResult {
    let when = history::ago(entry.copied_at, now);
    let icon = matches!(entry.content, Content::Image { .. }).then(|| IconRef {
        scheme: clipboard::SCHEME,
        key: entry.id.to_string(),
    });
    let preview = match &entry.content {
        Content::Text { text } => Preview {
            text: Some(text.chars().take(1000).collect()),
            image: None,
        },
        Content::Image { .. } => Preview {
            text: None,
            image: icon.clone(),
        },
    };
    let pin = if entry.pinned {
        action("unpin", "Unpin", "Ctrl+P")
    } else {
        action("pin", "Pin", "Ctrl+P")
    };
    SearchResult {
        id: format!("clip:{}", entry.id),
        title: entry.content.summary(),
        subtitle: Some(if entry.pinned {
            format!("Pinned · {when}")
        } else {
            when
        }),
        kind: "Clipboard",
        icon,
        glyph: Some("clipboard"),
        highlights,
        actions: vec![
            action("paste", "Paste", "Enter"),
            action("copy", "Copy", "Ctrl+Enter"),
            pin,
            action("delete", "Delete", "Ctrl+Delete"),
        ],
        preview: Some(preview),
        ..Default::default()
    }
}

/// Clipboard entries matching `text`, best first (all of them, pinned first,
/// when `text` is empty), with how well each matched.
fn clipboard_matches(
    store: &ClipboardStore,
    ranker: &mut Ranker,
    text: &str,
    limit: usize,
) -> Vec<(f64, SearchResult)> {
    let now = unix_now();
    store.with(|history| {
        let entries = history.listing();
        if text.is_empty() {
            return entries
                .iter()
                .take(limit)
                .map(|entry| (0.0, clipboard_result(entry, Vec::new(), now)))
                .collect();
        }
        let summaries: Vec<(&Entry, String)> = entries
            .iter()
            .map(|entry| (*entry, entry.content.summary()))
            .collect();
        // A tiny boost for newer copies, so they win ties.
        let recency =
            |copied_at: u64| 0.1 / (1.0 + now.saturating_sub(copied_at) as f64 / 86_400.0);
        ranker
            .rank(
                text,
                &summaries,
                |item| &item.1,
                |item| recency(item.0.copied_at),
                limit,
            )
            .into_iter()
            .map(|r| {
                (
                    r.score,
                    clipboard_result(summaries[r.index].0, r.highlights, now),
                )
            })
            .collect()
    })
}

/// Pause/resume and clear, at the end of the clipboard history.
fn clipboard_controls(paused: bool) -> Vec<SearchResult> {
    let control = |id: &str, title: &str, subtitle: &str, glyph, action| SearchResult {
        id: format!("clipcmd:{id}"),
        title: title.to_string(),
        subtitle: Some(subtitle.to_string()),
        kind: "Clipboard",
        glyph: Some(glyph),
        actions: vec![action],
        ..Default::default()
    };
    let pause = if paused {
        control(
            "resume",
            "Resume clipboard history",
            "Paused: new copies aren't being saved",
            "play",
            action("run", "Resume", "Enter"),
        )
    } else {
        control(
            "pause",
            "Pause clipboard history",
            "Stop saving new copies for now",
            "pause",
            action("run", "Pause", "Enter"),
        )
    };
    let clear = control(
        "clear",
        "Clear clipboard history",
        "Pinned items stay",
        "emptybin",
        ResultAction {
            confirm: Some(
                "Delete everything in your clipboard history except pinned items?".into(),
            ),
            ..action("run", "Clear", "Enter")
        },
    );
    vec![pause, clear]
}

#[tauri::command]
pub fn search(
    query: String,
    index: State<AppIndex>,
    state: State<SearchState>,
    clipboard: State<ClipboardStore>,
    settings: State<SettingsStore>,
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
        Query::Run("") => Vec::new(),
        Query::Run(text) => vec![run_result(text, Some(platform::expand_env(text)), "run")],
        Query::Clipboard(text) => {
            let mut results: Vec<SearchResult> =
                clipboard_matches(&clipboard, &mut ranker, text, MAX_CLIPBOARD_RESULTS)
                    .into_iter()
                    .map(|(_, result)| result)
                    .collect();
            if text.is_empty() {
                results.extend(clipboard_controls(settings.get().clipboard_paused));
            }
            results
        }
        Query::Commands(prefix) => query::matching_commands(prefix)
            .into_iter()
            .map(slash_command_result)
            .collect(),
        Query::Everything("") => Vec::new(),
        Query::Everything(text) => {
            // Things that show up above or below the ranked matches.
            let calc = calc::evaluate(text, true).map(calc_result);
            let location = run::looks_like_location(text).then(|| location_result(text));
            let program = (run::looks_like_program(text))
                .then(|| platform::find_program(text))
                .flatten()
                .map(|path| run_result(text, Some(path), "run"));
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
            // A couple of clipboard matches, ranked below similar matches
            // of other kinds.
            if text.chars().count() >= 3 {
                let copies = clipboard_matches(&clipboard, &mut ranker, text, 2);
                ranked.extend(copies.into_iter().map(|(score, r)| (score * 0.6, r)));
            }
            ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
            let extras = [calc.is_some(), location.is_some(), program.is_some(), true];
            ranked.truncate(MAX_RESULTS - extras.iter().filter(|&&shown| shown).count());

            calc.into_iter()
                .chain(location)
                .chain(ranked.into_iter().map(|(_, result)| result))
                .chain(program)
                .chain(Some(web_result(
                    web::search_engine(&settings.get().search_engine),
                    text,
                )))
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
) -> Result<Outcome, String> {
    let (kind, details) = id.split_once(':').ok_or("Unknown result.")?;
    match kind {
        "clip" => return run_clipboard_action(&app, &window, details, &action).await,
        "clipcmd" => {
            run_clipboard_command(&app, details);
            return Ok(Outcome::Refresh);
        }
        "app" => run_app_action(&app, &id, &action).await?,
        "sys" => run_system_command(&app, details).await?,
        "calc" => {
            let owner = window.hwnd().map_err(|e| e.to_string())?;
            platform::copy_text(owner, details)?;
        }
        "run" => {
            let how = match action.as_str() {
                "runAsAdmin" => Launch::AsAdmin,
                _ => Launch::Normal,
            };
            let command = details.to_string();
            let lower = command.to_ascii_lowercase();
            let web_address = lower.starts_with("http://") || lower.starts_with("https://");
            let browser = app.state::<SettingsStore>().get().browser;
            in_background(move || {
                if web_address && matches!(how, Launch::Normal) {
                    platform::open_link(&command, browser.as_deref())
                } else {
                    platform::run_command(&command, how)
                }
            })
            .await
            .map_err(|error| format!("Couldn't open “{details}”: {error}"))?;
        }
        "web" => {
            let (keyword, text) = details.split_once(':').ok_or("Unknown search.")?;
            let url = web::engine(keyword)
                .ok_or("Unknown search engine.")?
                .search_url(text);
            let browser = app.state::<SettingsStore>().get().browser;
            in_background(move || platform::open_link(&url, browser.as_deref())).await?;
        }
        _ => return Err("Unknown result.".into()),
    }
    launcher::hide(&app);
    Ok(Outcome::Done)
}

async fn run_clipboard_action(
    app: &AppHandle,
    window: &WebviewWindow,
    id: &str,
    action: &str,
) -> Result<Outcome, String> {
    let id: u64 = id.parse().map_err(|_| "Unknown clipboard entry.")?;
    let store = app.state::<ClipboardStore>();
    match action {
        "pin" | "unpin" => store.set_pinned(id, action == "pin"),
        "delete" => store.delete(id),
        "copy" | "paste" => {
            {
                let owner = window.hwnd().map_err(|e| e.to_string())?;
                store.copy(id, owner)?;
            }
            launcher::hide(app);
            if action == "paste" {
                let target = launcher::previous_window(app);
                in_background(move || {
                    platform::clipboard::paste_into(target);
                    Ok(())
                })
                .await?;
            }
            return Ok(Outcome::Done);
        }
        other => return Err(format!("Unknown action: {other}")),
    }
    Ok(Outcome::Refresh)
}

fn run_clipboard_command(app: &AppHandle, command: &str) {
    match command {
        "pause" | "resume" => {
            let paused = command == "pause";
            app.state::<SettingsStore>()
                .update(|settings| settings.clipboard_paused = paused);
            tray::sync_clipboard_paused(app, paused);
        }
        "clear" => app.state::<ClipboardStore>().clear(),
        _ => {}
    }
}

async fn run_app_action(app: &AppHandle, id: &str, action: &str) -> Result<(), String> {
    let target = app
        .state::<AppIndex>()
        .find(id)
        .ok_or("This app doesn't seem to be installed anymore.")?;
    if action == "uninstall" {
        return uninstall_app(app, target).await;
    }
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

async fn uninstall_app(app: &AppHandle, target: App) -> Result<(), String> {
    let how = target
        .uninstall
        .clone()
        .ok_or("Grandium doesn't know how to uninstall this app.")?;
    // Uninstallers bring up their own windows, and removing a Store app
    // takes a moment; either way the launcher should be out of the way.
    launcher::hide(app);
    let result = in_background(move || match how {
        Uninstall::Program { command, .. } => {
            let (program, args) = uninstall::uninstall_command_line(&command);
            platform::start_program(&program, &args, Launch::Normal)
        }
        Uninstall::Package(full_name) => platform::remove_package(&full_name),
    })
    .await;
    app.state::<AppIndex>().mark_stale();
    if let Err(error) = &result {
        launcher::show(app);
        return Err(format!("Couldn't uninstall {}: {error}", target.name));
    }
    result
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
