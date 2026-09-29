//! Windows shell integration: finding installed apps, starting them and
//! reading their icons.
//!
//! Apps come from the shell's "AppsFolder", the same list as the Start
//! menu's "All apps". It covers regular programs and Store apps alike, and
//! `shell:AppsFolder\<id>` starts either kind.

use std::os::windows::process::CommandExt;
use std::process::Command;

use windows::core::{w, Interface, GUID, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_CANCELLED, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::Storage::EnhancedStorage::PKEY_Link_TargetParsingPath;
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, IShellItem2,
    IShellItemImageFactory, SHCreateItemFromParsingName, SHGetKnownFolderItem,
    SHGetKnownFolderPath, ShellExecuteExW, KF_FLAG_DEFAULT, SEE_MASK_FLAG_NO_UI,
    SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Keeps COM initialized on the current thread while alive; the shell
/// APIs below need it.
pub struct Com {
    initialized: bool,
}

impl Com {
    pub fn init() -> Self {
        let result =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        Self {
            initialized: result.is_ok(),
        }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

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

fn message(error: windows::core::Error) -> String {
    error.message()
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
        lpVerb: match how {
            Launch::Normal => PCWSTR::null(),
            Launch::AsAdmin => w!("runas"),
        },
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    match unsafe { ShellExecuteExW(&mut info) } {
        Ok(()) => Ok(()),
        // The user said no to the administrator prompt; nothing went wrong.
        Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => Ok(()),
        Err(e) => Err(e.message()),
    }
}

/// Opens File Explorer with `path` selected.
pub fn show_in_explorer(path: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{path}\""))
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// The app's icon as a PNG, `size` pixels square (or close to it).
pub fn app_icon(id: &str, size: u32) -> Result<Vec<u8>, String> {
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&app_path(id), None).map_err(message)?;
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
