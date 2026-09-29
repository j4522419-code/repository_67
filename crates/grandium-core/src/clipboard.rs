//! Clipboard history: what was copied, newest first, within limits.

use serde::{Deserialize, Serialize};

/// The most unpinned entries kept.
pub const MAX_ENTRIES: usize = 500;
/// The most unpinned pictures kept, since they take far more space.
pub const MAX_IMAGES: usize = 50;
/// Unpinned entries are forgotten after this long.
pub const MAX_AGE_SECS: u64 = 30 * DAY;
/// Longer texts aren't kept: they're rarely worth finding again.
pub const MAX_TEXT_CHARS: usize = 100_000;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub content: Content,
    /// Unix time in seconds.
    pub copied_at: u64,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Content {
    Text {
        text: String,
    },
    /// The picture itself is stored separately, named by its hash.
    Image {
        hash: String,
        width: u32,
        height: u32,
    },
}

impl Content {
    /// A one-line description: the text with its whitespace squeezed, or
    /// the picture's size.
    pub fn summary(&self) -> String {
        match self {
            Content::Text { text } => summarize(text),
            Content::Image { width, height, .. } => format!("Image {width} × {height}"),
        }
    }
}

/// The start of a text on one line: whitespace (including line breaks)
/// squeezed to single spaces, cut at 300 characters.
pub fn summarize(text: &str) -> String {
    let mut summary = String::new();
    for word in text.split_whitespace() {
        if !summary.is_empty() {
            summary.push(' ');
        }
        summary.push_str(word);
        if summary.chars().count() >= 300 {
            break;
        }
    }
    summary.chars().take(300).collect()
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct History {
    /// Newest first.
    entries: Vec<Entry>,
    next_id: u64,
}

impl History {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Adds a copy at the top. Copying something that's already in the
    /// history moves it to the top instead of adding it twice. Blank and
    /// huge texts are skipped. Returns the entries dropped to stay within
    /// the limits, so their pictures can be deleted.
    pub fn add(&mut self, content: Content, now: u64) -> Vec<Entry> {
        if let Content::Text { text } = &content {
            if text.trim().is_empty() || text.chars().count() > MAX_TEXT_CHARS {
                return Vec::new();
            }
        }
        let existing = self.entries.iter().position(|e| same(&e.content, &content));
        let entry = match existing {
            Some(at) => Entry {
                copied_at: now,
                ..self.entries.remove(at)
            },
            None => {
                self.next_id += 1;
                Entry {
                    id: self.next_id,
                    content,
                    copied_at: now,
                    pinned: false,
                }
            }
        };
        self.entries.insert(0, entry);
        self.prune(now)
    }

    /// Drops unpinned entries that are too old, or beyond the count limits.
    pub fn prune(&mut self, now: u64) -> Vec<Entry> {
        let (mut kept, mut images) = (0, 0);
        let (keep, dropped): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| {
                if entry.pinned {
                    return true;
                }
                let is_image = matches!(entry.content, Content::Image { .. });
                let fits = now.saturating_sub(entry.copied_at) <= MAX_AGE_SECS
                    && kept < MAX_ENTRIES
                    && (!is_image || images < MAX_IMAGES);
                if fits {
                    kept += 1;
                    images += usize::from(is_image);
                }
                fits
            });
        self.entries = keep;
        dropped
    }

    pub fn set_pinned(&mut self, id: u64, pinned: bool) -> bool {
        match self.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.pinned = pinned;
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<Entry> {
        let at = self.entries.iter().position(|entry| entry.id == id)?;
        Some(self.entries.remove(at))
    }

    /// Forgets everything except pinned entries; returns what was removed.
    pub fn clear(&mut self) -> Vec<Entry> {
        let (pinned, removed) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| entry.pinned);
        self.entries = pinned;
        removed
    }

    /// Pinned entries first, then the rest; newest first within each.
    pub fn listing(&self) -> Vec<&Entry> {
        let (mut pinned, rest): (Vec<&Entry>, Vec<&Entry>) =
            self.entries.iter().partition(|entry| entry.pinned);
        pinned.extend(rest);
        pinned
    }
}

fn same(a: &Content, b: &Content) -> bool {
    match (a, b) {
        (Content::Text { text: a }, Content::Text { text: b }) => a == b,
        (Content::Image { hash: a, .. }, Content::Image { hash: b, .. }) => a == b,
        _ => false,
    }
}

/// How long ago something happened, briefly: "just now", "5 min ago",
/// "3 h ago", "2 days ago".
pub fn ago(then: u64, now: u64) -> String {
    let elapsed = now.saturating_sub(then);
    match elapsed {
        e if e < MINUTE => "just now".into(),
        e if e < HOUR => format!("{} min ago", e / MINUTE),
        e if e < DAY => format!("{} h ago", e / HOUR),
        e if e < 2 * DAY => "yesterday".into(),
        e => format!("{} days ago", e / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_790_000_000;

    fn text(t: &str) -> Content {
        Content::Text { text: t.into() }
    }

    fn image(hash: &str) -> Content {
        Content::Image {
            hash: hash.into(),
            width: 10,
            height: 10,
        }
    }

    fn texts(history: &History) -> Vec<String> {
        history
            .entries()
            .iter()
            .map(|e| e.content.summary())
            .collect()
    }

    #[test]
    fn newest_first() {
        let mut history = History::default();
        history.add(text("one"), NOW);
        history.add(text("two"), NOW + 1);
        assert_eq!(texts(&history), ["two", "one"]);
    }

    #[test]
    fn copying_again_moves_to_the_top() {
        let mut history = History::default();
        history.add(text("one"), NOW);
        history.add(text("two"), NOW + 1);
        let id = history.entries()[1].id;
        history.add(text("one"), NOW + 2);
        assert_eq!(texts(&history), ["one", "two"]);
        assert_eq!(history.entries()[0].id, id);
        assert_eq!(history.entries()[0].copied_at, NOW + 2);
    }

    #[test]
    fn same_picture_is_kept_once() {
        let mut history = History::default();
        history.add(image("abc"), NOW);
        history.add(image("abc"), NOW + 1);
        assert_eq!(history.entries().len(), 1);
    }

    #[test]
    fn skips_blank_and_huge_texts() {
        let mut history = History::default();
        history.add(text("  \n\t "), NOW);
        history.add(text(&"x".repeat(MAX_TEXT_CHARS + 1)), NOW);
        assert!(history.entries().is_empty());
    }

    #[test]
    fn keeps_at_most_max_entries_dropping_the_oldest() {
        let mut history = History::default();
        for i in 0..MAX_ENTRIES + 3 {
            history.add(text(&i.to_string()), NOW + i as u64);
        }
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert_eq!(history.entries().last().unwrap().content.summary(), "3");
    }

    #[test]
    fn keeps_at_most_max_images_and_reports_dropped_ones() {
        let mut history = History::default();
        let mut dropped = Vec::new();
        for i in 0..MAX_IMAGES + 2 {
            dropped.extend(history.add(image(&i.to_string()), NOW + i as u64));
        }
        history.add(text("still here"), NOW + 100);
        assert_eq!(history.entries().len(), MAX_IMAGES + 1);
        assert_eq!(dropped.len(), 2);
    }

    #[test]
    fn forgets_old_entries_but_not_pinned_ones() {
        let mut history = History::default();
        history.add(text("old"), NOW);
        history.add(text("old but pinned"), NOW);
        let pinned = history.entries()[0].id;
        history.set_pinned(pinned, true);
        let dropped = history.add(text("new"), NOW + MAX_AGE_SECS + 1);
        assert_eq!(texts(&history), ["new", "old but pinned"]);
        assert_eq!(dropped.len(), 1);
    }

    #[test]
    fn clear_keeps_pinned_entries() {
        let mut history = History::default();
        history.add(text("a"), NOW);
        history.add(text("b"), NOW);
        let b = history.entries()[0].id;
        history.set_pinned(b, true);
        assert_eq!(history.clear().len(), 1);
        assert_eq!(texts(&history), ["b"]);
    }

    #[test]
    fn listing_puts_pinned_first() {
        let mut history = History::default();
        history.add(text("a"), NOW);
        history.add(text("b"), NOW + 1);
        history.add(text("c"), NOW + 2);
        let a = history.entries()[2].id;
        history.set_pinned(a, true);
        let order: Vec<String> = history
            .listing()
            .iter()
            .map(|e| e.content.summary())
            .collect();
        assert_eq!(order, ["a", "c", "b"]);
    }

    #[test]
    fn remove_and_get() {
        let mut history = History::default();
        history.add(text("a"), NOW);
        let id = history.entries()[0].id;
        assert!(history.get(id).is_some());
        assert!(history.remove(id).is_some());
        assert!(history.get(id).is_none());
        assert!(history.remove(id).is_none());
    }

    #[test]
    fn summaries_are_one_short_line() {
        assert_eq!(summarize("  hello\n\n  world\t!  "), "hello world !");
        assert_eq!(summarize(&"word ".repeat(200)).chars().count(), 300);
        assert_eq!(image("x").summary(), "Image 10 × 10");
    }

    #[test]
    fn describes_how_long_ago() {
        assert_eq!(ago(NOW, NOW + 5), "just now");
        assert_eq!(ago(NOW, NOW + 5 * MINUTE), "5 min ago");
        assert_eq!(ago(NOW, NOW + 3 * HOUR), "3 h ago");
        assert_eq!(ago(NOW, NOW + DAY + HOUR), "yesterday");
        assert_eq!(ago(NOW, NOW + 9 * DAY), "9 days ago");
        assert_eq!(ago(NOW + 10, NOW), "just now");
    }

    #[test]
    fn round_trips_through_json() {
        let mut history = History::default();
        history.add(text("a"), NOW);
        history.add(image("h"), NOW);
        let json = serde_json::to_string(&history).unwrap();
        assert_eq!(serde_json::from_str::<History>(&json).unwrap(), history);
    }
}
