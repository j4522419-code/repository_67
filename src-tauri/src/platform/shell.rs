//! The Windows shell: finding installed apps, starting them, reading
//! their icons, and opening links.
//!
//! Apps come from the shell's "AppsFolder", the same list as the Start
//! menu's "All apps". It covers regular programs and Store apps alike, and
//! `shell:AppsFolder\<id>` starts either kind.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use grandium_core::run;

use windows::core::{w, Interface, GUID, HSTRING, PCWSTR, PWSTR};

use windows::Win32::Foundation::{ERROR_CANCELLED, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::Storage::EnhancedStorage::PKEY_Link_TargetParsingPath;
use windows::Win32::Storage::FileSystem::SearchPathW;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads,
    IEnumShellItems, IShellItem, IShellItem2, IShellItemImageFactory, SHCreateItemFromParsingName,
    SHFileOperationW, SHGetKnownFolderItem, SHGetKnownFolderPath, ShellExecuteExW, ShellExecuteW,
    FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, KF_FLAG_DEFAULT,
    SEE_MASK_FLAG_NO_UI, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
    SHFILEOPSTRUCTW, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::{message, registry};

/// An installed app, as the shell lists it.
pub struct ShellApp {
    /// The app's name inside AppsFolder: an app ID for Store apps, often a
    /// path for programs.
    pub id: String,
    pub name: String,
    /// What the app's shortcut points to, when it has one.
    pub target: Option<String>,
}

/// Lists everything in the Start menu's "All apps".
pub fn installed_apps() -> Result<Vec<ShellApp>, String> {
    unsafe {
        let folder: IShellItem =
            SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None).map_err(message)?;
        let items: IEnumShellItems = folder
            .BindToHandler(None, &BHID_EnumItems)
            .map_err(message)?;

        let mut apps = Vec::new();
        let mut batch: [Option<IShellItem>; 32] = Default::default();
        loop {
            let mut fetched = 0;
            items
                .Next(&mut batch, Some(&raw mut fetched))
                .map_err(message)?;
            if fetched == 0 {
                break;
            }
            for item in batch.iter_mut().take(fetched as usize) {
                if let Some(app) = item.take().and_then(|item| read_app(&item)) {
                    apps.push(app);
                }
            }
        }
        Ok(apps)
    }
}

unsafe fn read_app(item: &IShellItem) -> Option<ShellApp> {
    let name = take_string(item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?)?;
    let id = take_string(item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).ok()?)?;
    let target = item
        .cast::<IShellItem2>()
        .ok()
        .and_then(|item| item.GetString(&PKEY_Link_TargetParsingPath).ok())
        .and_then(|target| take_string(target))
        .filter(|target| !target.is_empty())
        .or_else(|| path_from_id(&id));
    Some(ShellApp { id, name, target })
}

/// The folders file search looks in, where Windows keeps them for this
/// user (possibly moved into OneDrive), by name.
pub fn user_folders() -> Vec<(&'static str, PathBuf)> {
    [
        ("Desktop", FOLDERID_Desktop),
        ("Documents", FOLDERID_Documents),
        ("Downloads", FOLDERID_Downloads),
    ]
    .into_iter()
    .filter_map(|(name, id)| {
        let path = unsafe { take_string(SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None).ok()?)? };
        Some((name, PathBuf::from(path)))
    })
    .collect()
}

/// Program IDs are often paths, either plain or starting with a known
/// folder's ID, like `{6D809377-...}\Google\Chrome\Application\chrome.exe`.
fn path_from_id(id: &str) -> Option<String> {
    let bytes = id.as_bytes();
    if bytes.len() > 2 && bytes[1] == b':' && bytes[2] == b'\\' {
        return Some(id.to_string());
    }
    let (folder, rest) = id.strip_prefix('{')?.split_once("}\\")?;
    let folder = GUID::try_from(folder).ok()?;
    let folder =
        unsafe { take_string(SHGetKnownFolderPath(&folder, KF_FLAG_DEFAULT, None).ok()?)? };
    Some(format!("{folder}\\{rest}"))
}

/// Copies a string the shell allocated for us, then frees it.
unsafe fn take_string(s: PWSTR) -> Option<String> {
    if s.is_null() {
        return None;
    }
    let copy = s.to_string().ok();
    CoTaskMemFree(Some(s.0.cast_const().cast()));
    copy
}

fn app_path(id: &str) -> HSTRING {
    HSTRING::from(format!("shell:AppsFolder\\{id}"))
}

#[derive(Clone, Copy)]
pub enum Launch {
    Normal,
    AsAdmin,
}

/// Starts the app with the given AppsFolder ID.
pub fn launch(id: &str, how: Launch) -> Result<(), String> {
    let file = app_path(id);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // Run it the way the shell would, including verbs like "runas", and
        // finish before returning: the calling thread may exit right after.
        fMask: SEE_MASK_INVOKEIDLIST | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: verb(how),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    execute(&mut info)
}

fn verb(how: Launch) -> PCWSTR {
    match how {
        Launch::Normal => PCWSTR::null(),
        Launch::AsAdmin => w!("runas"),
    }
}

fn execute(info: &mut SHELLEXECUTEINFOW) -> Result<(), String> {
    match unsafe { ShellExecuteExW(info) } {
        Ok(()) => Ok(()),
        // The user said no to the administrator prompt; nothing went wrong.
        Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => Ok(()),
        Err(e) => Err(e.message()),
    }
}

/// Opens what was typed, the way the Run box (Win+R) does: a path,
/// folder or link opens as it is; anything else is a program, found through
/// App Paths and the PATH, followed by its arguments.
pub fn run_command(text: &str, how: Launch) -> Result<(), String> {
    let expanded = expand_env(text.trim());
    let (file, args) = if Path::new(&expanded).exists() || run::looks_like_location(&expanded) {
        (expanded, String::new())
    } else {
        run::split_command_line(&expanded)
    };
    start_program(&file, &args, how)
}

/// Opens a file or folder the way double-clicking it in Explorer would.
/// For a file type nothing opens, Windows asks what to open it with.
pub fn open_path(path: &str) -> Result<(), String> {
    let file = HSTRING::from(path);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_INVOKEIDLIST | SEE_MASK_NOASYNC,
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    execute(&mut info)
}

/// Starts a program (or opens a file, folder or link) with arguments.
pub fn start_program(file: &str, args: &str, how: Launch) -> Result<(), String> {
    let file = HSTRING::from(file);
    let args = HSTRING::from(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: verb(how),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: if args.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(args.as_ptr())
        },
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    execute(&mut info)
}

/// Expands environment variables: `%temp%` becomes
/// `C:\Users\you\AppData\Local\Temp`.
pub fn expand_env(text: &str) -> String {
    let source = HSTRING::from(text);
    unsafe {
        let needed = ExpandEnvironmentStringsW(&source, None);
        if needed == 0 {
            return text.to_string();
        }
        let mut buffer = vec![0u16; needed as usize];
        let written = ExpandEnvironmentStringsW(&source, Some(&mut buffer));
        if written == 0 || written as usize > buffer.len() {
            return text.to_string();
        }
        // `written` counts the terminating zero.
        String::from_utf16_lossy(&buffer[..written as usize - 1])
    }
}

/// Where the Run box would find the program `name`: its registered App
/// Path if it has one, otherwise the first match on the PATH.
pub fn find_program(name: &str) -> Option<String> {
    let file = if name.contains('.') {
        name.to_string()
    } else {
        format!("{name}.exe")
    };
    let key = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{file}");
    for root in [registry::HKEY_CURRENT_USER, registry::HKEY_LOCAL_MACHINE] {
        if let Some(path) = registry::read_string(root, &key, None) {
            return Some(path.trim_matches('"').to_string());
        }
    }
    let mut buffer = vec![0u16; 1024];
    let len = unsafe {
        SearchPathW(
            PCWSTR::null(),
            &HSTRING::from(name),
            w!(".exe"),
            Some(&mut buffer),
            None,
        )
    };
    (len > 0 && (len as usize) < buffer.len())
        .then(|| String::from_utf16_lossy(&buffer[..len as usize]))
}

/// Opens File Explorer with `path` selected.
pub fn show_in_explorer(path: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{path}\""))
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Moves a file to the Recycle Bin, without asking first.
pub fn recycle(path: &str) -> Result<(), String> {
    // A list of paths, each ending in a zero, with one more zero at the end.
    let from: Vec<u16> = path.encode_utf16().chain([0, 0]).collect();
    let flags = FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI;
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: flags.0 as u16,
        ..Default::default()
    };
    match unsafe { SHFileOperationW(&mut operation) } {
        0 if operation.fAnyOperationsAborted.as_bool() => Err("Deleting was cancelled.".into()),
        0 => Ok(()),
        code => Err(format!("Windows couldn't delete it (error {code}).")),
    }
}

/// Opens a web address in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("Couldn't open your web browser.".into())
    }
}

/// The app's icon as a PNG, `size` pixels square (or close to it).
pub fn app_icon(id: &str, size: u32) -> Result<Vec<u8>, String> {
    icon(&app_path(id), size)
}

/// A file's or folder's icon, as Explorer shows it; like [`app_icon`].
/// Only the icon, never a thumbnail of what's inside, which could mean
/// downloading a OneDrive file.
pub fn file_icon(path: &str, size: u32) -> Result<Vec<u8>, String> {
    icon(&HSTRING::from(path), size)
}

fn icon(parsing_name: &HSTRING, size: u32) -> Result<Vec<u8>, String> {
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(parsing_name, None).map_err(message)?;
        let size = SIZE {
            cx: size as i32,
            cy: size as i32,
        };
        let bitmap = factory
            .GetImage(size, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .map_err(message)?;
        let png = bitmap_to_png(bitmap);
        let _ = DeleteObject(bitmap.into());
        png
    }
}

unsafe fn bitmap_to_png(bitmap: HBITMAP) -> Result<Vec<u8>, String> {
    let mut info = BITMAP::default();
    let read = GetObjectW(
        bitmap.into(),
        size_of::<BITMAP>() as i32,
        Some((&raw mut info).cast()),
    );
    if read == 0 {
        return Err("couldn't read the icon".into());
    }

    let (width, height) = (info.bmWidth, info.bmHeight.abs());
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // negative: rows top to bottom
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let dc = CreateCompatibleDC(None);
    let rows = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        Some(pixels.as_mut_ptr().cast()),
        &mut header,
        DIB_RGB_COLORS,
    );
    let _ = DeleteDC(dc);
    if rows != height {
        return Err("couldn't read the icon's pixels".into());
    }
    grandium_core::icon::bgra_to_png(width as u32, height as u32, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Com;

    #[test]
    fn recycles_files() {
        let file =
            std::env::temp_dir().join(format!("grandium-recycle-{}.txt", std::process::id()));
        std::fs::write(&file, "bye").unwrap();
        recycle(&file.to_string_lossy()).expect("couldn't recycle");
        assert!(!file.exists());
    }

    /// Runs against the real shell of the machine running the tests (a
    /// GitHub Windows runner in CI) and prints what it finds.
    #[test]
    fn lists_apps_with_icons() {
        let _com = Com::init();
        let apps = installed_apps().expect("couldn't read AppsFolder");
        println!("found {} apps", apps.len());
        for app in apps.iter().take(30) {
            println!("{:<45} | {:<70} | {:?}", app.name, app.id, app.target);
        }
        assert!(apps.len() >= 5, "found only {} apps", apps.len());

        let icons: Vec<Vec<u8>> = apps
            .iter()
            .take(10)
            .filter_map(|app| app_icon(&app.id, 64).ok())
            .collect();
        println!("icons read: {} of 10", icons.len());
        assert!(!icons.is_empty());
        assert!(icons.iter().all(|png| png.starts_with(b"\x89PNG")));
    }
}
