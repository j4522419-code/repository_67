//! Following changes to the files in a folder and all its subfolders.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

use super::message;

#[derive(Debug, PartialEq)]
pub enum FolderEvent {
    /// Something at this path was added, removed, renamed or written to.
    Changed(PathBuf),
    /// More changed than Windows could keep track of; look at everything
    /// again.
    Overflow,
}

/// Watches `folder` and everything in it on a thread of its own, sending
/// what changes to `events`. Stops when the folder can't be watched anymore
/// (it was deleted, say) or `events` has no receiver.
pub fn watch_folder(folder: PathBuf, events: Sender<FolderEvent>) -> Result<(), String> {
    let (ready, started) = mpsc::channel();
    std::thread::spawn(move || {
        let name: Vec<u16> = folder.as_os_str().encode_wide().chain([0]).collect();
        let opened = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                FILE_LIST_DIRECTORY.0,
                // Don't get in the way of anything done to the folder.
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                // Needed to open a folder rather than a file.
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        };
        match opened {
            Ok(handle) => {
                let _ = ready.send(Ok(()));
                follow(handle, &folder, &events);
                let _ = unsafe { CloseHandle(handle) };
            }
            Err(error) => {
                let _ = ready.send(Err(message(error)));
            }
        }
    });
    started
        .recv()
        .map_err(|_| "The folder watcher didn't start.".to_string())?
}

fn follow(folder_handle: HANDLE, folder: &Path, events: &Sender<FolderEvent>) {
    // The records hold 32-bit numbers, so the buffer is made of them to be
    // aligned right. 64 KB is the most a network folder allows.
    let mut buffer = vec![0u32; 16 * 1024];
    loop {
        let mut filled = 0u32;
        let result = unsafe {
            ReadDirectoryChangesW(
                folder_handle,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 4) as u32,
                true,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_LAST_WRITE,
                Some(&mut filled),
                None,
                None,
            )
        };
        if result.is_err() {
            return;
        }
        let sent = if filled == 0 {
            // The changes didn't fit in the buffer.
            events.send(FolderEvent::Overflow).is_ok()
        } else {
            let bytes: Vec<u8> = buffer[..(filled as usize).div_ceil(4)]
                .iter()
                .flat_map(|n| n.to_ne_bytes())
                .collect();
            changed_names(&bytes[..filled as usize])
                .into_iter()
                .all(|name| events.send(FolderEvent::Changed(folder.join(name))).is_ok())
        };
        if !sent {
            return;
        }
    }
}

/// The names in a buffer of `FILE_NOTIFY_INFORMATION` records: each has
/// the offset of the next record, the kind of change, the name's length in
/// bytes, then the name, relative to the watched folder.
fn changed_names(bytes: &[u8]) -> Vec<String> {
    let number = |at: usize| -> Option<u32> {
        let chunk = bytes.get(at..at + 4)?;
        Some(u32::from_ne_bytes(chunk.try_into().ok()?))
    };
    let mut names = Vec::new();
    let mut at = 0;
    while let (Some(next), Some(length)) = (number(at), number(at + 8)) {
        let Some(name) = bytes.get(at + 12..at + 12 + length as usize) else {
            break;
        };
        let units: Vec<u16> = name
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_ne_bytes(pair))
            .collect();
        names.push(String::from_utf16_lossy(&units));
        if next == 0 {
            break;
        }
        at += next as usize;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn record(next: u32, action: u32, name: &str) -> Vec<u8> {
        let name: Vec<u8> = name.encode_utf16().flat_map(|u| u.to_ne_bytes()).collect();
        let mut bytes = Vec::new();
        bytes.extend(next.to_ne_bytes());
        bytes.extend(action.to_ne_bytes());
        bytes.extend((name.len() as u32).to_ne_bytes());
        bytes.extend(name);
        // Records start on 4-byte boundaries.
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        bytes
    }

    #[test]
    fn reads_change_records() {
        let mut bytes = record(0, 4, r"Taxes\old name.pdf");
        // Point the first record at the second.
        let second = (bytes.len() as u32).to_ne_bytes();
        bytes[..4].copy_from_slice(&second);
        bytes.extend(record(0, 5, "Ünïcode ✓.txt"));
        assert_eq!(
            changed_names(&bytes),
            [r"Taxes\old name.pdf", "Ünïcode ✓.txt"]
        );

        let single = record(0, 1, "Budget.xlsx");
        assert_eq!(changed_names(&single), ["Budget.xlsx"]);
        assert!(changed_names(&[]).is_empty());
        assert!(changed_names(&single[..14]).is_empty(), "cut short");
    }

    /// Watches a real folder on the machine running the tests.
    #[test]
    fn reports_new_files() {
        let folder = std::env::temp_dir().join(format!("grandium-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("inside")).unwrap();
        let (events, received) = mpsc::channel();
        watch_folder(folder.clone(), events).expect("couldn't watch the folder");

        // The watcher is ready once Windows has been asked for changes, which
        // can be a moment after `watch_folder` returns; changes made before
        // that aren't reported. So keep changing the file until one is.
        let file = folder.join(r"inside\new file.txt");
        let mut reported = false;
        'attempts: for attempt in 0..10 {
            std::fs::write(&file, format!("attempt {attempt}")).unwrap();
            while let Ok(event) = received.recv_timeout(Duration::from_secs(1)) {
                if event == FolderEvent::Changed(file.clone()) {
                    reported = true;
                    break 'attempts;
                }
                println!("also reported: {event:?}");
            }
        }
        drop(received);
        let _ = std::fs::remove_dir_all(&folder);
        assert!(reported, "no change reported for {}", file.display());
    }
}
