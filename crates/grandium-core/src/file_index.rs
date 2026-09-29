//! The file search index: the names of files and folders under a few
//! folders (Desktop, Documents, Downloads), kept current as they change.
//!
//! Walking folders only needs the standard library, so this builds and is
//! tested on any OS. Windows supplies the folders and reports changes.

use std::collections::BTreeMap;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Indexing stops at this many entries, which bounds memory and how long a
/// search takes.
pub const MAX_ENTRIES: usize = 150_000;
/// Folders nested deeper than this below a root aren't looked into.
const MAX_DEPTH: usize = 16;

/// A folder whose contents are indexed.
#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    pub path: PathBuf,
    /// How the folder is shown in results: "Documents".
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meta {
    pub is_dir: bool,
    /// Last modified, in Unix seconds.
    pub modified: u64,
}

/// What a change on disk means for the index; see [`FileIndex::inspect`].
#[derive(Debug, PartialEq)]
pub enum Change {
    /// Nothing to do.
    None,
    /// The path is gone, or shouldn't be indexed anymore: remove it and
    /// everything in it.
    Remove(PathBuf),
    /// Add or update these entries. A new folder brings its contents.
    Upsert(Vec<(PathBuf, Meta)>),
}

#[derive(Debug, Default)]
pub struct FileIndex {
    roots: Vec<Root>,
    /// Ordered by path, so a folder's contents come right after it.
    entries: BTreeMap<PathBuf, Meta>,
}

impl FileIndex {
    /// Walks the roots and indexes everything in them.
    pub fn scan(roots: Vec<Root>) -> Self {
        let mut unique: Vec<Root> = Vec::new();
        for root in roots {
            if root.path.is_dir() && unique.iter().all(|r| r.path != root.path) {
                unique.push(root);
            }
        }
        let mut index = FileIndex {
            roots: unique,
            entries: BTreeMap::new(),
        };
        for root in 0..index.roots.len() {
            let path = index.roots[root].path.clone();
            let found = index.walk(&path, 0);
            index.entries.extend(found);
        }
        index
    }

    pub fn roots(&self) -> &[Root] {
        &self.roots
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Path, &Meta)> {
        self.entries
            .iter()
            .map(|(path, meta)| (path.as_path(), meta))
    }

    pub fn get(&self, path: &Path) -> Option<&Meta> {
        self.entries.get(path)
    }

    /// The most recently modified files (not folders), newest first.
    pub fn recent(&self, limit: usize) -> Vec<(&Path, &Meta)> {
        let mut files: Vec<(&Path, &Meta)> = self.iter().filter(|(_, m)| !m.is_dir).collect();
        files.sort_by_key(|(_, meta)| std::cmp::Reverse(meta.modified));
        files.truncate(limit);
        files
    }

    /// The root `path` is in (the innermost, if roots are nested), unless
    /// `path` is a root itself.
    fn root_of(&self, path: &Path) -> Option<&Root> {
        self.roots
            .iter()
            .filter(|root| path != root.path && path.starts_with(&root.path))
            .max_by_key(|root| root.path.as_os_str().len())
    }

    /// Where `path` is, for showing under its name: "Documents › Taxes".
    pub fn location(&self, path: &Path) -> String {
        let Some(root) = self.root_of(path) else {
            return path
                .parent()
                .map_or_else(String::new, |p| p.display().to_string());
        };
        let mut parts = vec![root.label.clone()];
        if let Some(inside) = path.parent().and_then(|p| p.strip_prefix(&root.path).ok()) {
            parts.extend(
                inside
                    .iter()
                    .map(|part| part.to_string_lossy().into_owned()),
            );
        }
        parts.join(" › ")
    }

    /// Works out what a change reported at `path` means, by looking at
    /// the disk. Only reads the index; [`FileIndex::apply`] makes the
    /// change, so searches can go on while a big new folder is walked.
    pub fn inspect(&self, path: &Path) -> Change {
        let Some(root) = self.root_of(path) else {
            return Change::None;
        };
        let Ok(inside) = path.strip_prefix(&root.path) else {
            return Change::None;
        };
        let depth = inside.iter().count();
        // Only index things whose folder is indexed. Folders that are
        // skipped (hidden, node_modules, too deep) keep their contents out.
        let parent_indexed = path
            .parent()
            .is_some_and(|parent| parent == root.path || self.entries.contains_key(parent));
        if depth > MAX_DEPTH || !parent_indexed {
            return Change::None;
        }
        let Some((meta, is_real_dir)) = read_meta(path) else {
            return Change::Remove(path.to_path_buf());
        };
        let name = path.file_name().and_then(|n| n.to_str());
        if name.is_none_or(|name| is_skipped(name, meta.is_dir)) || is_hidden(path) {
            return Change::Remove(path.to_path_buf());
        }
        let mut entries = vec![(path.to_path_buf(), meta)];
        // A folder that's new here (created, renamed or moved in) brings
        // its contents. For one already indexed, changes inside it are
        // reported separately.
        if is_real_dir && !self.entries.contains_key(path) {
            entries.extend(self.walk(path, depth));
        }
        Change::Upsert(entries)
    }

    pub fn apply(&mut self, change: Change) {
        match change {
            Change::None => {}
            Change::Remove(path) => {
                let gone: Vec<PathBuf> = self
                    .entries
                    .range(path.clone()..)
                    .map(|(p, _)| p)
                    .take_while(|p| p.starts_with(&path))
                    .cloned()
                    .collect();
                for path in gone {
                    self.entries.remove(&path);
                }
            }
            Change::Upsert(entries) => {
                for (path, meta) in entries {
                    if self.entries.len() < MAX_ENTRIES || self.entries.contains_key(&path) {
                        self.entries.insert(path, meta);
                    }
                }
            }
        }
    }

    /// Everything below `dir`, which is `depth` levels below its root.
    /// Other roots inside it are left to be walked as themselves.
    fn walk(&self, dir: &Path, depth: usize) -> Vec<(PathBuf, Meta)> {
        let mut found = Vec::new();
        let mut pending = vec![(dir.to_path_buf(), depth)];
        while let Some((dir, depth)) = pending.pop() {
            let Ok(listing) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in listing.flatten() {
                if self.entries.len() + found.len() >= MAX_ENTRIES {
                    return found;
                }
                let path = entry.path();
                let (Some(name), Ok(file_type)) = (path.file_name(), entry.file_type()) else {
                    continue;
                };
                // Names that aren't valid Unicode can't be typed anyway.
                let Some(name) = name.to_str() else {
                    continue;
                };
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                // Links to folders show as folders, but aren't followed:
                // they can point anywhere, even back up.
                let is_dir = file_type.is_dir() || (file_type.is_symlink() && path.is_dir());
                if is_skipped(name, is_dir) || has_hidden_attribute(&metadata) {
                    continue;
                }
                if self.roots.iter().any(|root| root.path == path) {
                    continue;
                }
                let meta = Meta {
                    is_dir,
                    modified: modified(&metadata),
                };
                if file_type.is_dir() && depth < MAX_DEPTH {
                    pending.push((path.clone(), depth + 1));
                }
                found.push((path, meta));
            }
        }
        found
    }
}

/// The entry's details, and whether it's a folder that can be walked (not a
/// link to one). `None` if it doesn't exist.
fn read_meta(path: &Path) -> Option<(Meta, bool)> {
    let metadata = fs::symlink_metadata(path).ok()?;
    let file_type = metadata.file_type();
    let is_dir = file_type.is_dir() || (file_type.is_symlink() && path.is_dir());
    Some((
        Meta {
            is_dir,
            modified: modified(&metadata),
        },
        file_type.is_dir(),
    ))
}

fn modified(metadata: &Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

fn is_hidden(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| has_hidden_attribute(&m))
}

/// Windows' hidden and system attributes; Explorer doesn't show those
/// files either.
#[cfg(windows)]
fn has_hidden_attribute(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    metadata.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn has_hidden_attribute(_: &Metadata) -> bool {
    false
}

/// Files and folders that aren't worth finding: hidden ones, tool and
/// download leftovers, and folders full of code dependencies.
pub fn is_skipped(name: &str, is_dir: bool) -> bool {
    let lower = name.to_lowercase();
    if lower.starts_with('.') || lower.starts_with('$') {
        return true;
    }
    if is_dir {
        return matches!(lower.as_str(), "node_modules" | "__pycache__");
    }
    // "~$Budget.xlsx" is Office's lock file for an open "Budget.xlsx".
    lower.starts_with("~$")
        || matches!(lower.as_str(), "desktop.ini" | "thumbs.db")
        || [".tmp", ".crdownload", ".part", ".partial"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}

/// The name to show and search by. Like Explorer, shortcuts and web links
/// are shown without their `.lnk` or `.url` ending.
pub fn display_name(path: &Path) -> &str {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lower = name.to_ascii_lowercase();
    if name.len() > 4 && (lower.ends_with(".lnk") || lower.ends_with(".url")) {
        &name[..name.len() - 4]
    } else {
        name
    }
}

/// Files whose icon is the same as every other file of their type share
/// one: "ext:.pdf". `None` means the file has its own icon (programs,
/// shortcuts, icon files) and must be asked for itself.
pub fn icon_group(path: &Path, is_dir: bool) -> Option<String> {
    if is_dir {
        return Some("folder".into());
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    const OWN_ICON: &[&str] = &["exe", "lnk", "url", "ico", "appref-ms", "msc", "cpl", "scr"];
    (!OWN_ICON.contains(&ext.as_str())).then(|| format!("ext:.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh folder for one test, removed afterwards.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "grandium-files-{}-{}",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn add(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            if relative.ends_with('/') {
                fs::create_dir_all(&path).unwrap();
            } else {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, b"x").unwrap();
            }
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn root(dir: &TempDir, sub: &str, label: &str) -> Root {
        Root {
            path: dir.0.join(sub),
            label: label.into(),
        }
    }

    fn names(index: &FileIndex) -> Vec<String> {
        let mut names: Vec<String> = index
            .iter()
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn indexes_files_and_folders_but_skips_junk() {
        let dir = TempDir::new();
        dir.add("Documents/Budget 2026.xlsx");
        dir.add("Documents/Taxes/2025 return.pdf");
        dir.add("Documents/project/node_modules/lib/index.js");
        dir.add("Documents/project/.git/config");
        dir.add("Documents/project/main.rs");
        dir.add("Documents/~$Budget 2026.xlsx");
        dir.add("Documents/desktop.ini");
        dir.add("Downloads/setup.exe");
        dir.add("Downloads/movie.mp4.crdownload");
        let index = FileIndex::scan(vec![
            root(&dir, "Documents", "Documents"),
            root(&dir, "Downloads", "Downloads"),
            root(&dir, "Missing", "Missing"),
        ]);
        assert_eq!(
            names(&index),
            [
                "2025 return.pdf",
                "Budget 2026.xlsx",
                "Taxes",
                "main.rs",
                "project",
                "setup.exe"
            ]
        );
        assert_eq!(index.roots().len(), 2, "missing folders aren't roots");
        let taxes = index.get(&dir.0.join("Documents/Taxes")).unwrap();
        assert!(taxes.is_dir);
        assert!(taxes.modified > 0);
    }

    #[test]
    fn nested_roots_are_indexed_once() {
        let dir = TempDir::new();
        dir.add("Home/notes.txt");
        dir.add("Home/Downloads/song.mp3");
        let index = FileIndex::scan(vec![
            root(&dir, "Home", "Home"),
            root(&dir, "Home/Downloads", "Downloads"),
        ]);
        assert_eq!(names(&index), ["notes.txt", "song.mp3"]);
        let song = dir.0.join("Home/Downloads/song.mp3");
        assert_eq!(index.location(&song), "Downloads");
    }

    #[test]
    fn locations_start_at_the_root() {
        let dir = TempDir::new();
        let file = dir.add("Documents/Work/2026/plan.docx");
        let top = dir.add("Documents/todo.txt");
        let index = FileIndex::scan(vec![root(&dir, "Documents", "Documents")]);
        assert_eq!(index.location(&file), "Documents › Work › 2026");
        assert_eq!(index.location(&top), "Documents");
    }

    #[test]
    fn follows_changes() {
        let dir = TempDir::new();
        dir.add("Downloads/old.zip");
        let mut index = FileIndex::scan(vec![root(&dir, "Downloads", "Downloads")]);
        fn changed(index: &mut FileIndex, path: &Path) {
            let change = index.inspect(path);
            index.apply(change);
        }

        // A new file.
        let report = dir.add("Downloads/report.pdf");
        changed(&mut index, &report);
        assert_eq!(names(&index), ["old.zip", "report.pdf"]);

        // A folder moved in brings its contents.
        let album = dir.add("Downloads/Album/");
        dir.add("Downloads/Album/01 Intro.mp3");
        dir.add("Downloads/Album/Art/cover.jpg");
        changed(&mut index, &album);
        assert_eq!(
            names(&index),
            [
                "01 Intro.mp3",
                "Album",
                "Art",
                "cover.jpg",
                "old.zip",
                "report.pdf"
            ]
        );

        // Renaming the folder: the old name goes, with everything in it.
        let renamed = dir.0.join("Downloads/Album 2026");
        fs::rename(&album, &renamed).unwrap();
        changed(&mut index, &album);
        changed(&mut index, &renamed);
        assert!(index
            .get(&dir.0.join("Downloads/Album/Art/cover.jpg"))
            .is_none());
        assert!(index.get(&renamed.join("Art/cover.jpg")).is_some());

        // Deleting a file.
        fs::remove_file(&report).unwrap();
        changed(&mut index, &report);
        assert!(index.get(&report).is_none());

        // Skipped things stay out, and so does what's inside them.
        let partial = dir.add("Downloads/big.iso.crdownload");
        let hidden = dir.add("Downloads/.cache/thing.bin");
        changed(&mut index, &partial);
        changed(&mut index, hidden.parent().unwrap());
        changed(&mut index, &hidden);
        assert_eq!(index.get(&partial), None);
        assert_eq!(index.get(&hidden), None);

        // Outside the roots, and the root itself: nothing to do.
        assert_eq!(index.inspect(&dir.0.join("elsewhere.txt")), Change::None);
        assert_eq!(index.inspect(&dir.0.join("Downloads")), Change::None);
    }

    #[test]
    fn a_similar_name_isnt_inside_a_folder() {
        // As plain text, "Album 2" sorts between "Album" and "Album/a.mp3";
        // removing "Album" mustn't take it too.
        let dir = TempDir::new();
        dir.add("Music/Album/a.mp3");
        dir.add("Music/Album 2/b.mp3");
        let mut index = FileIndex::scan(vec![root(&dir, "Music", "Music")]);
        index.apply(Change::Remove(dir.0.join("Music/Album")));
        assert_eq!(names(&index), ["Album 2", "b.mp3"]);
    }

    #[test]
    fn recent_lists_newest_files_first() {
        let mut index = FileIndex::default();
        let entry = |modified, is_dir| Meta { is_dir, modified };
        index.apply(Change::Upsert(vec![
            (PathBuf::from("/r/old.txt"), entry(100, false)),
            (PathBuf::from("/r/new.txt"), entry(300, false)),
            (PathBuf::from("/r/folder"), entry(400, true)),
            (PathBuf::from("/r/mid.txt"), entry(200, false)),
        ]));
        let recent: Vec<&str> = index
            .recent(2)
            .iter()
            .map(|(p, _)| display_name(p))
            .collect();
        assert_eq!(recent, ["new.txt", "mid.txt"]);
    }

    #[test]
    fn skip_rules() {
        assert!(is_skipped(".git", true));
        assert!(is_skipped("node_modules", true));
        assert!(is_skipped("$RECYCLE.BIN", true));
        assert!(is_skipped("~$Report.docx", false));
        assert!(is_skipped("Thumbs.db", false));
        assert!(is_skipped("video.mp4.CRDOWNLOAD", false));
        assert!(!is_skipped("node_modules", false));
        assert!(!is_skipped("Taxes", true));
        assert!(!is_skipped("report.pdf", false));
    }

    #[test]
    fn shortcuts_show_without_their_ending() {
        assert_eq!(display_name(Path::new("/d/Steam.lnk")), "Steam");
        assert_eq!(display_name(Path::new("/d/News.URL")), "News");
        assert_eq!(display_name(Path::new("/d/report.pdf")), "report.pdf");
        assert_eq!(display_name(Path::new("/d/.lnk")), ".lnk");
    }

    #[test]
    fn icons_are_shared_by_type() {
        let group = |path: &str, dir| icon_group(Path::new(path), dir);
        assert_eq!(group("/d/a.PDF", false).as_deref(), Some("ext:.pdf"));
        assert_eq!(group("/d/b.pdf", false), group("/d/a.PDF", false));
        assert_eq!(group("/d/Taxes", true).as_deref(), Some("folder"));
        assert_eq!(group("/d/setup.exe", false), None);
        assert_eq!(group("/d/Steam.lnk", false), None);
        assert_eq!(group("/d/README", false), None);
    }

    #[test]
    fn stops_at_the_entry_limit() {
        let mut index = FileIndex::default();
        let many = (0..MAX_ENTRIES + 10)
            .map(|i| {
                let meta = Meta {
                    is_dir: false,
                    modified: 0,
                };
                (PathBuf::from(format!("/r/{i}.txt")), meta)
            })
            .collect();
        index.apply(Change::Upsert(many));
        assert_eq!(index.len(), MAX_ENTRIES);
    }
}
