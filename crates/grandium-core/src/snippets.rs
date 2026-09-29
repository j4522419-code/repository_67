//! Snippets: saved text that's pasted from the launcher. Each has a
//! keyword; typing `;addr` finds the snippet with the keyword `addr`.

use serde::{Deserialize, Serialize};

use crate::clipboard::summarize;

/// Filled in when a snippet is pasted, with what they stand for.
pub const PLACEHOLDERS: &[(&str, &str)] = &[
    ("{date}", "today's date"),
    ("{time}", "the time now"),
    ("{clipboard}", "what's on the clipboard"),
];

const MAX_KEYWORD_CHARS: usize = 32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: u64,
    /// What's typed after `;` to find it: `addr`. Lowercase, no spaces.
    pub keyword: String,
    pub text: String,
}

impl Snippet {
    /// The text on one line, for showing in a list.
    pub fn summary(&self) -> String {
        summarize(&self.text)
    }
}

/// Every snippet, kept in keyword order.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snippets {
    items: Vec<Snippet>,
}

impl Snippets {
    pub fn all(&self) -> &[Snippet] {
        &self.items
    }

    pub fn get(&self, id: u64) -> Option<&Snippet> {
        self.items.iter().find(|s| s.id == id)
    }

    pub fn with_keyword(&self, keyword: &str) -> Option<&Snippet> {
        let keyword = clean_keyword(keyword);
        self.items.iter().find(|s| s.keyword == keyword)
    }

    /// Adds a snippet, or changes the one with `id`. Returns its ID, or
    /// what's wrong with it, ready to show.
    pub fn save(&mut self, id: Option<u64>, keyword: &str, text: &str) -> Result<u64, String> {
        let keyword = clean_keyword(keyword);
        check_keyword(&keyword)?;
        if text.trim().is_empty() {
            return Err("Type the text to paste.".into());
        }
        if let Some(other) = self.with_keyword(&keyword).filter(|s| Some(s.id) != id) {
            return Err(format!(
                "“;{}” is already used for “{}”.",
                other.keyword,
                other.summary()
            ));
        }
        let id = match id.and_then(|id| self.items.iter_mut().find(|s| s.id == id)) {
            Some(existing) => {
                existing.keyword = keyword;
                existing.text = text.to_string();
                existing.id
            }
            None => {
                let id = self.items.iter().map(|s| s.id).max().unwrap_or(0) + 1;
                self.items.push(Snippet {
                    id,
                    keyword,
                    text: text.to_string(),
                });
                id
            }
        };
        self.items.sort_by(|a, b| a.keyword.cmp(&b.keyword));
        Ok(id)
    }

    /// Whether there was a snippet with that ID.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|s| s.id != id);
        self.items.len() != before
    }
}

/// Keywords are stored without the `;` and in lowercase: `;Addr` → `addr`.
pub fn clean_keyword(keyword: &str) -> String {
    keyword.trim().trim_start_matches(';').trim().to_lowercase()
}

/// Whether `keyword` (already cleaned) can be used, and if not, why.
pub fn check_keyword(keyword: &str) -> Result<(), String> {
    if keyword.is_empty() {
        Err("Type a keyword, like “addr” to paste it with ;addr.".into())
    } else if keyword.contains(char::is_whitespace) {
        Err("A keyword can't have spaces.".into())
    } else if keyword.chars().count() > MAX_KEYWORD_CHARS {
        Err(format!(
            "Keep the keyword to {MAX_KEYWORD_CHARS} characters or fewer."
        ))
    } else {
        Ok(())
    }
}

/// What the placeholders stand for right now.
pub struct Fill<'a> {
    pub date: &'a str,
    pub time: &'a str,
    pub clipboard: &'a str,
}

/// Whether pasting `text` needs what's on the clipboard.
pub fn uses_clipboard(text: &str) -> bool {
    text.contains("{clipboard}")
}

/// `text` with its placeholders filled in. Text that comes from filling
/// one in (the clipboard's, say) is left as it is, even if it looks like a
/// placeholder itself.
pub fn expand(text: &str, fill: &Fill) -> String {
    let mut expanded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        expanded.push_str(&rest[..start]);
        rest = &rest[start..];
        let value = [
            ("{date}", fill.date),
            ("{time}", fill.time),
            ("{clipboard}", fill.clipboard),
        ]
        .into_iter()
        .find(|(placeholder, _)| rest.starts_with(placeholder));
        match value {
            Some((placeholder, value)) => {
                expanded.push_str(value);
                rest = &rest[placeholder.len()..];
            }
            None => {
                expanded.push('{');
                rest = &rest[1..];
            }
        }
    }
    expanded.push_str(rest);
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILL: Fill = Fill {
        date: "29/09/2026",
        time: "14:05",
        clipboard: "copied {date}",
    };

    #[test]
    fn fills_in_placeholders() {
        assert_eq!(
            expand("Sent {date} at {time}: {clipboard}", &FILL),
            "Sent 29/09/2026 at 14:05: copied {date}"
        );
        assert_eq!(expand("{date}{date}", &FILL), "29/09/202629/09/2026");
        assert_eq!(
            expand("{unknown} {date {} {", &FILL),
            "{unknown} {date {} {"
        );
        assert_eq!(expand("héllo {time} ✓", &FILL), "héllo 14:05 ✓");
        assert!(uses_clipboard("x {clipboard}"));
        assert!(!uses_clipboard("x {date}"));
    }

    #[test]
    fn saves_and_finds_by_keyword() {
        let mut snippets = Snippets::default();
        let addr = snippets
            .save(None, " ;Addr ", "12 Harbour Street\nWellington")
            .unwrap();
        let sig = snippets.save(None, "sig", "Cheers,\nSam").unwrap();
        assert_ne!(addr, sig);
        assert_eq!(snippets.with_keyword(";ADDR").unwrap().id, addr);
        assert_eq!(snippets.get(sig).unwrap().keyword, "sig");
        assert_eq!(
            snippets.get(addr).unwrap().summary(),
            "12 Harbour Street Wellington"
        );
        let keywords: Vec<&str> = snippets.all().iter().map(|s| s.keyword.as_str()).collect();
        assert_eq!(keywords, ["addr", "sig"]);
    }

    #[test]
    fn edits_in_place() {
        let mut snippets = Snippets::default();
        let id = snippets.save(None, "addr", "old").unwrap();
        assert_eq!(snippets.save(Some(id), "home", "new"), Ok(id));
        assert_eq!(snippets.all().len(), 1);
        assert_eq!(snippets.get(id).unwrap().keyword, "home");
        assert_eq!(snippets.get(id).unwrap().text, "new");
        // Keeping its own keyword is fine.
        assert_eq!(snippets.save(Some(id), "home", "newer"), Ok(id));
    }

    #[test]
    fn rejects_bad_snippets() {
        let mut snippets = Snippets::default();
        snippets.save(None, "addr", "12 Harbour Street").unwrap();
        let taken = snippets.save(None, "ADDR", "other").unwrap_err();
        assert!(taken.contains("already used"), "{taken}");
        assert!(snippets.save(None, ";", "x").is_err());
        assert!(snippets.save(None, "two words", "x").is_err());
        assert!(snippets.save(None, &"a".repeat(33), "x").is_err());
        assert!(snippets.save(None, "blank", "  \n ").is_err());
        assert_eq!(snippets.all().len(), 1);
    }

    #[test]
    fn removes() {
        let mut snippets = Snippets::default();
        let id = snippets.save(None, "addr", "x").unwrap();
        assert!(snippets.remove(id));
        assert!(!snippets.remove(id));
        assert!(snippets.all().is_empty());
        // IDs aren't reused while others remain, and start over after.
        assert_eq!(snippets.save(None, "a", "x"), Ok(1));
    }

    #[test]
    fn loads_older_or_partial_files() {
        let snippets: Snippets = serde_json::from_str("{}").unwrap();
        assert!(snippets.all().is_empty());
        let json = r#"{"items":[{"id":7,"keyword":"addr","text":"x"}]}"#;
        let snippets: Snippets = serde_json::from_str(json).unwrap();
        assert_eq!(snippets.with_keyword("addr").unwrap().id, 7);
    }
}
