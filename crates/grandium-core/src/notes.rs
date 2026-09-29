//! Quick notes: plain text files in one folder (`Documents\Grandium\Notes`
//! on Windows), so they can be opened, edited, synced and backed up
//! without Grandium.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::clipboard::summarize;
use crate::file_index;
use crate::rank::{self, Ranker};

/// Only this much of a note is read, for searching and previews.
const MAX_READ_BYTES: usize = 64 * 1024;
/// A folder with more notes than this has only the newest ones searched.
const MAX_NOTES: usize = 2000;
/// Long first lines make long file names; this is where they're cut.
const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub path: PathBuf,
    pub title: String,
    /// The note's text (up to the first 64 KB).
    pub text: String,
    /// Last changed, in Unix seconds.
    pub modified: u64,
    /// Title and text together, which is what searches look in.
    haystack: String,
    /// Tells whether the file changed since it was read.
    stamp: Stamp,
}

/// A file's exact modification time and size.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stamp {
    nanos: u128,
    size: u64,
}

/// The notes in a folder, newest first.
#[derive(Debug, Default)]
pub struct Notes {
    dir: PathBuf,
    notes: Vec<Note>,
}

impl Notes {
    /// Notes kept in `dir`, which is created when the first note is saved.
    pub fn new(dir: PathBuf) -> Self {
        let mut notes = Notes {
            dir,
            notes: Vec::new(),
        };
        notes.refresh();
        notes
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn all(&self) -> &[Note] {
        &self.notes
    }

    pub fn get(&self, path: &Path) -> Option<&Note> {
        self.notes.iter().find(|n| n.path == path)
    }

    /// Catches up with the folder: reads new and changed notes, and
    /// forgets deleted ones. Unchanged notes aren't read again.
    pub fn refresh(&mut self) {
        let Ok(listing) = fs::read_dir(&self.dir) else {
            self.notes.clear();
            return;
        };
        let mut found: Vec<(PathBuf, Stamp)> = listing
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .map(|entry| entry.path())
            .filter(|path| is_note_file(path))
            .filter_map(|path| {
                let metadata = fs::metadata(&path).ok()?;
                let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
                let stamp = Stamp {
                    nanos: modified.as_nanos(),
                    size: metadata.len(),
                };
                Some((path, stamp))
            })
            .collect();
        found.sort_by_key(|(_, stamp)| std::cmp::Reverse(stamp.nanos));
        found.truncate(MAX_NOTES);

        let mut old = std::mem::take(&mut self.notes);
        self.notes = found
            .into_iter()
            .filter_map(|(path, stamp)| {
                let known = old
                    .iter()
                    .position(|n| n.path == path && n.stamp == stamp)
                    .map(|i| old.swap_remove(i));
                known.or_else(|| read_note(path, stamp))
            })
            .collect();
    }

    /// Saves `text` as a new note named after its first line, and returns
    /// where it went.
    pub fn create(&mut self, text: &str) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.dir)?;
        let stem = file_stem(text);
        let mut path = self.dir.join(format!("{stem}.txt"));
        let mut n = 2;
        while path.exists() {
            path = self.dir.join(format!("{stem} ({n}).txt"));
            n += 1;
        }
        let mut contents = text.trim().to_string();
        contents.push('\n');
        // `create_new`: never overwrite, even if a note appeared meanwhile.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| io::Write::write_all(&mut file, contents.as_bytes()))?;
        self.refresh();
        Ok(path)
    }

    /// Notes with every word of `query` in their title or text, best
    /// first, as `(note, score, highlights in the title)`.
    pub fn search(
        &self,
        ranker: &mut Ranker,
        query: &str,
        limit: usize,
    ) -> Vec<(&Note, f64, Vec<[u32; 2]>)> {
        ranker
            .rank_words(query, &self.notes, |n| &n.haystack, |_| 0.0, limit)
            .into_iter()
            .map(|r| {
                let note = &self.notes[r.index];
                let highlights = rank::clip_highlights(r.highlights, &note.title);
                (note, r.score, highlights)
            })
            .collect()
    }
}

fn read_note(path: PathBuf, stamp: Stamp) -> Option<Note> {
    let bytes = fs::read(&path).ok()?;
    let text = decode(&bytes[..bytes.len().min(MAX_READ_BYTES)]);
    let title = title(&text).unwrap_or_else(|| file_index::display_name(&path).to_string());
    let haystack = format!("{title}\n{text}");
    Some(Note {
        path,
        title,
        text,
        modified: (stamp.nanos / 1_000_000_000) as u64,
        haystack,
        stamp,
    })
}

/// All of a note's text, however long.
pub fn read_text(path: &Path) -> io::Result<String> {
    fs::read(path).map(|bytes| decode(&bytes))
}

/// Text files are usually UTF-8, but Notepad can also save UTF-16.
fn decode(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = utf16
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn is_note_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    matches!(ext.as_deref(), Some("txt" | "md")) && !file_index::is_skipped(name, false)
}

/// A note's title: its first line with text, without Markdown's `#`s.
/// `None` for a note without any text.
pub fn title(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.trim_start_matches('#').trim();
    let line = if line.is_empty() {
        "Untitled note"
    } else {
        line
    };
    Some(summarize(line).chars().take(100).collect())
}

/// A file name (without `.txt`) for a new note: its title, with what
/// Windows doesn't allow in file names left out.
pub fn file_stem(text: &str) -> String {
    let title = title(text).unwrap_or_default();
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut stem: String = summarize(&cleaned).chars().take(MAX_NAME_CHARS).collect();
    // Windows drops dots and spaces at the end of names.
    stem.truncate(stem.trim_end_matches(['.', ' ']).len());
    const RESERVED: &[&str] = &[
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if stem.is_empty() {
        "Note".into()
    } else if RESERVED.contains(&stem.to_lowercase().as_str()) {
        format!("{stem} note")
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "grandium-notes-{}-{}",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&path);
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn titles(notes: &Notes) -> Vec<&str> {
        let mut titles: Vec<&str> = notes.all().iter().map(|n| n.title.as_str()).collect();
        titles.sort();
        titles
    }

    #[test]
    fn creates_notes_named_after_their_first_line() {
        let dir = TempDir::new();
        let mut notes = Notes::new(dir.0.join("Notes"));
        assert!(notes.all().is_empty(), "no folder yet, no notes");

        let path = notes.create("  buy milk  ").unwrap();
        assert_eq!(path.file_name().unwrap(), "buy milk.txt");
        assert_eq!(fs::read_to_string(&path).unwrap(), "buy milk\n");
        let again = notes.create("buy milk").unwrap();
        assert_eq!(again.file_name().unwrap(), "buy milk (2).txt");
        let odd = notes.create("a/b: c? <d>...").unwrap();
        assert_eq!(odd.file_name().unwrap(), "a b c d.txt");

        assert_eq!(titles(&notes), ["a/b: c? <d>...", "buy milk", "buy milk"]);
        assert_eq!(notes.get(&path).unwrap().text, "buy milk\n");
    }

    #[test]
    fn follows_the_folder() {
        let dir = TempDir::new();
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join("plan.md"), "# Trip plan\n\nBook the ferry").unwrap();
        fs::write(dir.0.join("list.txt"), "eggs\nflour").unwrap();
        fs::write(dir.0.join("photo.jpg"), "not a note").unwrap();
        fs::write(dir.0.join("~$lock.txt"), "office lock file").unwrap();
        let mut notes = Notes::new(dir.0.clone());
        assert_eq!(titles(&notes), ["Trip plan", "eggs"]);

        fs::remove_file(dir.0.join("list.txt")).unwrap();
        fs::write(dir.0.join("new.txt"), "fresh").unwrap();
        notes.refresh();
        assert_eq!(titles(&notes), ["Trip plan", "fresh"]);
    }

    #[test]
    fn reads_utf16_and_bom_files() {
        assert_eq!(decode(b"\xEF\xBB\xBFhi"), "hi");
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("héllo".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode(&utf16), "héllo");
    }

    #[test]
    fn searches_titles_and_text() {
        let dir = TempDir::new();
        let mut notes = Notes::new(dir.0.clone());
        notes.create("Trip plan\nBook the ferry to Picton").unwrap();
        notes.create("Groceries\neggs, flour, milk").unwrap();
        let mut ranker = Ranker::default();

        let found = notes.search(&mut ranker, "trip", 10);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.title, "Trip plan");
        assert_eq!(found[0].2, vec![[0, 4]]);

        // Found in the text: nothing in the title to highlight.
        let found = notes.search(&mut ranker, "ferry", 10);
        assert_eq!(found[0].0.title, "Trip plan");
        assert!(found[0].2.is_empty());

        assert!(notes.search(&mut ranker, "ferry milk", 10).is_empty());
    }

    #[test]
    fn titles_and_names() {
        assert_eq!(title("\n\n  ## Ideas  \nmore").as_deref(), Some("Ideas"));
        assert_eq!(title("#\nx").as_deref(), Some("Untitled note"));
        assert_eq!(title("  \n "), None);
        assert_eq!(file_stem(""), "Note");
        assert_eq!(file_stem("con"), "con note");
        assert_eq!(file_stem("end with dots..."), "end with dots");
        assert_eq!(file_stem(&"word ".repeat(30)).chars().count(), 59);
        assert!(is_note_file(Path::new("/n/a.TXT")));
        assert!(is_note_file(Path::new("/n/b.md")));
        assert!(!is_note_file(Path::new("/n/c.docx")));
        assert!(!is_note_file(Path::new("/n/.hidden.txt")));
    }
}
