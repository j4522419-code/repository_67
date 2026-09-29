//! What the search box calls: finding results for what was typed, and
//! running the action picked for a result.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use grandium_core::apps::{can_run_as_admin, has_file_location};
use grandium_core::clipboard::{self as history, Content, Entry};
use grandium_core::file_index::{self, FileIndex, Meta};
use grandium_core::notes::{self, Note};
use grandium_core::query::{self, Query, SlashCommand};
use grandium_core::rank::{self, Ranker};
use grandium_core::snippets::{self, Snippet};
use grandium_core::system::{self, Command};
use grandium_core::usage::Usage;
use grandium_core::{calc, run, uninstall, web};
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::apps::{App, AppIndex, Uninstall};
use crate::clipboard::{self, ClipboardStore};
use crate::file_search::FileSearch;
use crate::notes::NoteStore;
use crate::platform::{self, Com, Launch};
use crate::settings::SettingsStore;
use crate::snippets::SnippetStore;
use crate::{files, icons, launcher, tray};

const MAX_RESULTS: usize = 8;
/// Clipboard history and `/files` scroll, so they can show more.
const MAX_CLIPBOARD_RESULTS: usize = 50;
const MAX_FILE_RESULTS: usize = 50;
/// How many files can show up among everything else.
const FILES_AMONG_EVERYTHING: usize = 3;
/// Files rank a little below apps and commands that match as well.
const FILE_WEIGHT: f64 = 0.7;
const MAX_SNIPPET_RESULTS: usize = 50;
const MAX_NOTE_RESULTS: usize = 50;
/// How many snippets and notes can show up among everything else.
const SNIPPETS_AMONG_EVERYTHING: usize = 2;
const NOTES_AMONG_EVERYTHING: usize = 2;
const SNIPPET_WEIGHT: f64 = 0.9;
const NOTE_WEIGHT: f64 = 0.8;

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
    let mut actions = vec![
        action("paste", "Paste", "Enter"),
        action("copy", "Copy", "Ctrl+Enter"),
        pin,
        action("delete", "Delete", "Ctrl+Delete"),
    ];
    if matches!(entry.content, Content::Text { .. }) {
        actions.insert(3, action("editSnippet", "Save as snippet", "Ctrl+S"));
    }
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
        actions,
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

fn file_key(path: &Path) -> String {
    format!("file:{}", path.display())
}

fn file_result(
    index: &FileIndex,
    path: &Path,
    meta: &Meta,
    highlights: Vec<[u32; 2]>,
) -> SearchResult {
    let kind = if meta.is_dir { "folder" } else { "file" };
    SearchResult {
        id: file_key(path),
        title: file_index::display_name(path).to_string(),
        subtitle: Some(index.location(path)),
        kind: if meta.is_dir { "Folder" } else { "File" },
        icon: Some(IconRef {
            scheme: icons::SCHEME,
            key: format!("{kind}:{}", path.display()),
        }),
        glyph: Some(kind),
        highlights,
        actions: vec![
            action("open", "Open", "Enter"),
            action("openLocation", "Open file location", "Ctrl+Enter"),
            action("copyPath", "Copy path", "Ctrl+Shift+C"),
        ],
        ..Default::default()
    }
}

/// Files and folders whose name has every word of `text` in it, best
/// first, with how well each matched.
/// Notes (in `notes_dir`) show up as notes instead.
fn file_matches(
    index: &FileIndex,
    notes_dir: &Path,
    ranker: &mut Ranker,
    text: &str,
    limit: usize,
    boost: impl Fn(&str) -> f64,
) -> Vec<(f64, SearchResult)> {
    let now = unix_now();
    let items: Vec<(&Path, &Meta, &str)> = index
        .iter()
        .filter(|(path, _)| !path.starts_with(notes_dir))
        .map(|(path, meta)| (path, meta, file_index::display_name(path)))
        .collect();
    // Files changed lately are more likely to be the ones wanted.
    let fresh = |modified: u64| 0.5 / (1.0 + now.saturating_sub(modified) as f64 / 604_800.0);
    ranker
        .rank_words(
            text,
            &items,
            |item| item.2,
            |item| boost(&file_key(item.0)) + fresh(item.1.modified),
            limit,
        )
        .into_iter()
        .map(|r| {
            let (path, meta, _) = items[r.index];
            (r.score, file_result(index, path, meta, r.highlights))
        })
        .collect()
}

fn is_shortcut(result: &SearchResult) -> bool {
    let id = result.id.to_ascii_lowercase();
    result.kind == "File" && (id.ends_with(".lnk") || id.ends_with(".url"))
}

fn snippet_key(id: u64) -> String {
    format!("snip:{id}")
}

fn snippet_result(snippet: &Snippet, highlights: Vec<[u32; 2]>) -> SearchResult {
    SearchResult {
        id: snippet_key(snippet.id),
        title: format!(";{}", snippet.keyword),
        subtitle: Some(snippet.summary()),
        kind: "Snippet",
        glyph: Some("snippet"),
        highlights,
        actions: vec![
            action("paste", "Paste", "Enter"),
            action("copy", "Copy", "Ctrl+Enter"),
            action("editSnippet", "Edit", "Ctrl+E"),
            ResultAction {
                confirm: Some(format!("Delete the snippet ;{}?", snippet.keyword)),
                ..action("delete", "Delete", "Ctrl+Delete")
            },
        ],
        preview: Some(Preview {
            text: Some(snippet.text.chars().take(1000).collect()),
            image: None,
        }),
        ..Default::default()
    }
}

/// Opens the snippet editor, with `keyword` filled in if there is one.
fn new_snippet_result(keyword: &str) -> SearchResult {
    let (title, subtitle) = if keyword.is_empty() {
        (
            "New snippet".to_string(),
            "Save text to paste by typing ; and a keyword".to_string(),
        )
    } else {
        (
            format!("New snippet ;{keyword}"),
            format!("Save text to paste with ;{keyword}"),
        )
    };
    SearchResult {
        id: format!("snipcmd:new:{keyword}"),
        title,
        subtitle: Some(subtitle),
        kind: "Snippet",
        glyph: Some("add"),
        actions: vec![action("editSnippet", "Create", "Enter")],
        ..Default::default()
    }
}

/// Snippets with every word of `text` in their keyword or text, best
/// first, with how well each matched.
fn snippet_matches(
    store: &SnippetStore,
    ranker: &mut Ranker,
    text: &str,
    limit: usize,
    boost: impl Fn(&str) -> f64,
) -> Vec<(f64, SearchResult)> {
    store.with(|all| {
        let items: Vec<(&Snippet, String)> = all
            .all()
            .iter()
            .map(|s| (s, format!(";{}\n{}", s.keyword, s.text)))
            .collect();
        ranker
            .rank_words(
                text,
                &items,
                |item| &item.1,
                |item| boost(&snippet_key(item.0.id)),
                limit,
            )
            .into_iter()
            .map(|r| {
                let snippet = items[r.index].0;
                let title = format!(";{}", snippet.keyword);
                let highlights = rank::clip_highlights(r.highlights, &title);
                (r.score, snippet_result(snippet, highlights))
            })
            .collect()
    })
}

fn note_key(path: &Path) -> String {
    format!("note:{}", path.display())
}

fn note_result(note: &Note, highlights: Vec<[u32; 2]>, now: u64) -> SearchResult {
    SearchResult {
        id: note_key(&note.path),
        title: note.title.clone(),
        subtitle: Some(history::ago(note.modified, now)),
        kind: "Note",
        glyph: Some("note"),
        highlights,
        actions: vec![
            action("open", "Open", "Enter"),
            action("copy", "Copy text", "Ctrl+Enter"),
            action("openLocation", "Open file location", "Ctrl+Shift+Enter"),
            ResultAction {
                confirm: Some(format!(
                    "Delete the note “{}”? It goes to the Recycle Bin.",
                    note.title
                )),
                ..action("delete", "Delete", "Ctrl+Delete")
            },
        ],
        preview: Some(Preview {
            text: Some(note.text.chars().take(1000).collect()),
            image: None,
        }),
        ..Default::default()
    }
}

fn save_note_result(text: &str) -> SearchResult {
    SearchResult {
        id: format!("notecmd:save:{text}"),
        title: format!("Save note “{text}”"),
        subtitle: Some("As a text file in your notes folder".into()),
        kind: "Note",
        glyph: Some("add"),
        actions: vec![
            action("save", "Save", "Enter"),
            action("saveOpen", "Save and open", "Ctrl+Enter"),
        ],
        ..Default::default()
    }
}

fn notes_folder_result(dir: &Path) -> SearchResult {
    SearchResult {
        id: "notecmd:folder".into(),
        title: "Open notes folder".into(),
        subtitle: Some(dir.display().to_string()),
        kind: "Note",
        glyph: Some("folder"),
        actions: vec![action("open", "Open", "Enter")],
        ..Default::default()
    }
}

#[tauri::command]
pub fn search(query: String, app: AppHandle) -> Vec<SearchResult> {
    let index = app.state::<AppIndex>();
    let state = app.state::<SearchState>();
    let clipboard = app.state::<ClipboardStore>();
    let settings = app.state::<SettingsStore>();
    let files = app.state::<FileSearch>();
    let snippet_store = app.state::<SnippetStore>();
    let note_store = app.state::<NoteStore>();
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
        Query::Files("") => {
            let index = files.index();
            let notes_dir = note_store.dir();
            index
                .recent(MAX_FILE_RESULTS * 2)
                .into_iter()
                .filter(|(path, _)| !path.starts_with(&notes_dir))
                .take(MAX_FILE_RESULTS)
                .map(|(path, meta)| file_result(&index, path, meta, Vec::new()))
                .collect()
        }
        Query::Files(text) => file_matches(
            &files.index(),
            &note_store.dir(),
            &mut ranker,
            text,
            MAX_FILE_RESULTS,
            boost,
        )
        .into_iter()
        .map(|(_, result)| result)
        .collect(),
        Query::Snippets(text) => {
            let keyword = snippets::clean_keyword(text);
            let exact = snippet_store.with(|all| all.with_keyword(&keyword).cloned());
            let mut results: Vec<SearchResult> = exact
                .iter()
                .map(|snippet| {
                    let title_len = snippet.keyword.encode_utf16().count() as u32 + 1;
                    snippet_result(snippet, vec![[0, title_len]])
                })
                .collect();
            if text.is_empty() {
                results.extend(snippet_store.with(|all| {
                    all.all()
                        .iter()
                        .take(MAX_SNIPPET_RESULTS)
                        .map(|snippet| snippet_result(snippet, Vec::new()))
                        .collect::<Vec<_>>()
                }));
            } else {
                let exact_id = exact.as_ref().map(|s| snippet_key(s.id));
                results.extend(
                    snippet_matches(
                        &snippet_store,
                        &mut ranker,
                        text,
                        MAX_SNIPPET_RESULTS,
                        boost,
                    )
                    .into_iter()
                    .map(|(_, result)| result)
                    .filter(|result| Some(&result.id) != exact_id.as_ref()),
                );
            }
            if text.is_empty() {
                results.push(new_snippet_result(""));
            } else if exact.is_none() && snippets::check_keyword(&keyword).is_ok() {
                results.push(new_snippet_result(&keyword));
            }
            results
        }
        Query::Notes("") => {
            let mut results: Vec<SearchResult> = note_store.with(|all| {
                all.all()
                    .iter()
                    .take(MAX_NOTE_RESULTS)
                    .map(|note| note_result(note, Vec::new(), now))
                    .collect()
            });
            results.push(notes_folder_result(&note_store.dir()));
            results
        }
        Query::Notes(text) => {
            let mut results = vec![save_note_result(text)];
            results.extend(note_store.with(|all| {
                all.search(&mut ranker, text, MAX_NOTE_RESULTS)
                    .into_iter()
                    .map(|(note, _, highlights)| note_result(note, highlights, now))
                    .collect::<Vec<_>>()
            }));
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
            if text.chars().count() >= 2 {
                // Desktop shortcuts to apps are already there as the apps.
                let apps: HashSet<String> =
                    ranked.iter().map(|(_, r)| r.title.to_lowercase()).collect();
                let found = file_matches(
                    &files.index(),
                    &note_store.dir(),
                    &mut ranker,
                    text,
                    FILES_AMONG_EVERYTHING + 2,
                    boost,
                );
                ranked.extend(
                    found
                        .into_iter()
                        .filter(|(_, r)| {
                            !(is_shortcut(r) && apps.contains(&r.title.to_lowercase()))
                        })
                        .take(FILES_AMONG_EVERYTHING)
                        .map(|(score, r)| (score * FILE_WEIGHT, r)),
                );
                let snippets = snippet_matches(
                    &snippet_store,
                    &mut ranker,
                    text,
                    SNIPPETS_AMONG_EVERYTHING,
                    boost,
                );
                ranked.extend(
                    snippets
                        .into_iter()
                        .map(|(score, r)| (score * SNIPPET_WEIGHT, r)),
                );
                let notes: Vec<(f64, SearchResult)> = note_store.with(|all| {
                    all.search(&mut ranker, text, NOTES_AMONG_EVERYTHING)
                        .into_iter()
                        .map(|(note, score, highlights)| {
                            (score * NOTE_WEIGHT, note_result(note, highlights, now))
                        })
                        .collect()
                });
                ranked.extend(notes);
            }
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
        "file" => run_file_action(&app, &window, &id, details, &action).await?,
        "snip" => return run_snippet_action(&app, &window, &id, details, &action).await,
        "note" => return run_note_action(&app, &window, &id, details, &action).await,
        "notecmd" => run_note_command(&app, details, &action).await?,
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

async fn run_file_action(
    app: &AppHandle,
    window: &WebviewWindow,
    id: &str,
    path: &str,
    action: &str,
) -> Result<(), String> {
    if action == "copyPath" {
        let owner = window.hwnd().map_err(|e| e.to_string())?;
        return platform::copy_text(owner, path);
    }
    if !Path::new(path).exists() {
        return Err(format!(
            "“{}” isn't there anymore. It may have been moved or deleted.",
            file_index::display_name(Path::new(path))
        ));
    }
    let target = path.to_string();
    match action {
        "openLocation" => in_background(move || platform::show_in_explorer(&target)).await,
        _ => {
            in_background(move || platform::open_path(&target)).await?;
            app.state::<SearchState>().record_use(id);
            Ok(())
        }
    }
}

async fn run_snippet_action(
    app: &AppHandle,
    window: &WebviewWindow,
    key: &str,
    id: &str,
    action: &str,
) -> Result<Outcome, String> {
    let id: u64 = id.parse().map_err(|_| "Unknown snippet.")?;
    let store = app.state::<SnippetStore>();
    match action {
        "delete" => {
            store.remove(id)?;
            Ok(Outcome::Refresh)
        }
        "copy" | "paste" => {
            {
                let owner = window.hwnd().map_err(|e| e.to_string())?;
                let text = store.expanded(id, owner)?;
                platform::copy_text(owner, &text)?;
            }
            app.state::<SearchState>().record_use(key);
            launcher::hide(app);
            if action == "paste" {
                let target = launcher::previous_window(app);
                in_background(move || {
                    platform::clipboard::paste_into(target);
                    Ok(())
                })
                .await?;
            }
            Ok(Outcome::Done)
        }
        other => Err(format!("Unknown action: {other}")),
    }
}

async fn run_note_action(
    app: &AppHandle,
    window: &WebviewWindow,
    key: &str,
    path: &str,
    action: &str,
) -> Result<Outcome, String> {
    let file = PathBuf::from(path);
    if !file.exists() {
        return Err("That note isn't there anymore. It may have been moved or deleted.".into());
    }
    match action {
        "delete" => {
            let target = path.to_string();
            in_background(move || platform::recycle(&target)).await?;
            app.state::<NoteStore>().refresh();
            return Ok(Outcome::Refresh);
        }
        "copy" => {
            let text = notes::read_text(&file).map_err(|e| e.to_string())?;
            let owner = window.hwnd().map_err(|e| e.to_string())?;
            platform::copy_text(owner, text.trim_end())?;
        }
        "openLocation" => {
            let target = path.to_string();
            in_background(move || platform::show_in_explorer(&target)).await?;
        }
        _ => {
            let target = path.to_string();
            in_background(move || platform::open_path(&target)).await?;
            app.state::<SearchState>().record_use(key);
        }
    }
    launcher::hide(app);
    Ok(Outcome::Done)
}

/// Saving a note typed after `/note`, and opening the notes folder.
async fn run_note_command(app: &AppHandle, command: &str, action: &str) -> Result<(), String> {
    let store = app.state::<NoteStore>();
    let open = if let Some(text) = command.strip_prefix("save:") {
        let path = store.create(text)?;
        (action == "saveOpen").then_some(path)
    } else if command == "folder" {
        let dir = store.dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Couldn't make the notes folder: {e}"))?;
        Some(dir)
    } else {
        return Err("Unknown command.".into());
    };
    if let Some(path) = open {
        let target = path.to_string_lossy().into_owned();
        in_background(move || platform::open_path(&target)).await?;
    }
    Ok(())
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
