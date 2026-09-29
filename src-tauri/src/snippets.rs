//! Snippets: kept in `%APPDATA%\Grandium\snippets.json`, pasted with their
//! placeholders filled in, and made or changed on the editor screen.

use std::path::PathBuf;
use std::sync::Mutex;

use grandium_core::clipboard::Content;
use grandium_core::snippets::{self, Fill, Snippets, PLACEHOLDERS};
use serde::{Deserialize, Serialize};
use tauri::State;
use windows::Win32::Foundation::HWND;

use crate::clipboard::ClipboardStore;
use crate::files;
use crate::platform;

pub struct SnippetStore {
    snippets: Mutex<Snippets>,
    /// Where they're saved; `None` keeps them in memory only.
    file: Option<PathBuf>,
}

impl SnippetStore {
    pub fn load(file: Option<PathBuf>) -> Self {
        Self {
            snippets: Mutex::new(files::load_json(file.as_deref())),
            file,
        }
    }

    pub fn with<R>(&self, read: impl FnOnce(&Snippets) -> R) -> R {
        read(&self.snippets.lock().unwrap())
    }

    /// Changes the snippets and saves them. Unlike usage history, losing
    /// snippets matters, so a failed save is reported.
    fn update<R>(&self, change: impl FnOnce(&mut Snippets) -> R) -> Result<R, String> {
        let mut snippets = self.snippets.lock().unwrap();
        let result = change(&mut snippets);
        if let Some(file) = &self.file {
            files::save_json(file, &*snippets)
                .map_err(|e| format!("Couldn't save your snippets: {e}"))?;
        }
        Ok(result)
    }

    pub fn remove(&self, id: u64) -> Result<(), String> {
        self.update(|snippets| {
            snippets.remove(id);
        })
    }

    /// The snippet's text with its placeholders filled in, ready to paste.
    /// `owner` is one of our windows, for reading the clipboard.
    pub fn expanded(&self, id: u64, owner: HWND) -> Result<String, String> {
        let text = self
            .with(|snippets| snippets.get(id).map(|s| s.text.clone()))
            .ok_or("That snippet doesn't exist anymore.")?;
        let clipboard = if snippets::uses_clipboard(&text) {
            platform::clipboard::read_text(owner).unwrap_or_default()
        } else {
            String::new()
        };
        let (date, time) = platform::locale::date_and_time();
        Ok(snippets::expand(
            &text,
            &Fill {
                date: &date,
                time: &time,
                clipboard: &clipboard,
            },
        ))
    }
}

/// A snippet being made or changed on the editor screen.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetDraft {
    /// `None` for a new snippet.
    id: Option<u64>,
    keyword: String,
    text: String,
}

#[derive(Serialize)]
pub struct Placeholder {
    text: &'static str,
    meaning: &'static str,
}

#[derive(Serialize)]
pub struct SnippetEditing {
    draft: SnippetDraft,
    placeholders: Vec<Placeholder>,
}

/// What the editor starts with for the result `id`: a snippet to change
/// (`snip:<id>`), a new one with a keyword typed already
/// (`snipcmd:new:<keyword>`), or a new one holding a copied text
/// (`clip:<id>`).
#[tauri::command]
pub fn snippet_draft(
    id: String,
    snippets: State<SnippetStore>,
    clipboard: State<ClipboardStore>,
) -> Result<SnippetEditing, String> {
    let blank = |keyword: &str, text: String| SnippetDraft {
        id: None,
        keyword: snippets::clean_keyword(keyword),
        text,
    };
    let draft = if let Some(id) = id.strip_prefix("snip:") {
        let id: u64 = id.parse().map_err(|_| "Unknown snippet.")?;
        snippets
            .with(|all| all.get(id).cloned())
            .map(|snippet| SnippetDraft {
                id: Some(snippet.id),
                keyword: snippet.keyword,
                text: snippet.text,
            })
            .ok_or("That snippet doesn't exist anymore.")?
    } else if let Some(keyword) = id.strip_prefix("snipcmd:new:") {
        blank(keyword, String::new())
    } else if let Some(entry) = id.strip_prefix("clip:") {
        let entry: u64 = entry.parse().map_err(|_| "Unknown clipboard entry.")?;
        let text = clipboard.with(|history| match history.get(entry).map(|e| &e.content) {
            Some(Content::Text { text }) => Some(text.clone()),
            _ => None,
        });
        blank(
            "",
            text.ok_or("Only copied text can be saved as a snippet.")?,
        )
    } else {
        return Err("Unknown snippet.".into());
    };
    Ok(SnippetEditing {
        draft,
        placeholders: PLACEHOLDERS
            .iter()
            .map(|&(text, meaning)| Placeholder { text, meaning })
            .collect(),
    })
}

/// Saves the editor's snippet and returns its keyword, or says what's
/// wrong with it.
#[tauri::command]
pub fn save_snippet(draft: SnippetDraft, snippets: State<SnippetStore>) -> Result<String, String> {
    let id = snippets.update(|all| all.save(draft.id, &draft.keyword, &draft.text))??;
    Ok(snippets.with(|all| all.get(id).map(|s| s.keyword.clone()).unwrap_or_default()))
}
