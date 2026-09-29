//! The clipboard: watching what gets copied, writing to it, and pasting.

use std::sync::{mpsc, OnceLock};
use std::time::Duration;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
    RegisterClassW, SetForegroundWindow, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CLIPBOARDUPDATE, WNDCLASSW,
};

use super::message;

/// Something copied, as read from the clipboard.
pub enum Copied {
    Text(String),
    /// A Windows bitmap (`CF_DIB`).
    Bitmap(Vec<u8>),
    /// PNG bytes, which browsers and Office also put on the clipboard.
    Png(Vec<u8>),
}

type OnCopy = Box<dyn Fn(Copied) + Send + Sync>;
static ON_COPY: OnceLock<OnCopy> = OnceLock::new();

/// Calls `on_copy` whenever something is copied, on a thread of its own.
/// Copies that apps mark as private (password managers do) are skipped,
/// as are things other than text and pictures, like files.
pub fn watch(on_copy: impl Fn(Copied) + Send + Sync + 'static) -> Result<(), String> {
    ON_COPY
        .set(Box::new(on_copy))
        .map_err(|_| "Already watching the clipboard.".to_string())?;
    let (ready, started) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let instance = GetModuleHandleW(None)
            .map(|module| HINSTANCE(module.0))
            .unwrap_or_default();
        let class = w!("GrandiumClipboardWatcher");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(on_message),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        });
        // A message-only window: never shown, it just receives messages.
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        );
        let listening =
            window.and_then(|window| AddClipboardFormatListener(window).map(|()| window));
        if let Err(error) = listening {
            let _ = ready.send(Err(message(error)));
            return;
        }
        let _ = ready.send(Ok(()));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            DispatchMessageW(&msg);
        }
    });
    started
        .recv()
        .map_err(|_| "Couldn't start watching the clipboard.".to_string())?
}

unsafe extern "system" fn on_message(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        // Read quickly and let go of the clipboard before doing anything
        // slow with what was copied.
        let copied = open(window).ok().and_then(|()| {
            let copied = read();
            let _ = CloseClipboard();
            copied
        });
        if let (Some(copied), Some(on_copy)) = (copied, ON_COPY.get()) {
            on_copy(copied);
        }
        return LRESULT(0);
    }
    DefWindowProcW(window, msg, wparam, lparam)
}

/// Reads the open clipboard: text if there is any, else a picture.
unsafe fn read() -> Option<Copied> {
    let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
    let ignore = RegisterClipboardFormatW(w!("Clipboard Viewer Ignore"));
    let can_include = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
    if available(exclude) || available(ignore) {
        return None;
    }
    // The same flag Windows' own clipboard history (Win+V) respects.
    if available(can_include) && read_bytes(can_include).is_some_and(|b| b.starts_with(&[0; 4])) {
        return None;
    }

    let text = u32::from(CF_UNICODETEXT.0);
    let bitmap = u32::from(CF_DIB.0);
    let png = RegisterClipboardFormatW(w!("PNG"));
    if available(text) {
        read_bytes(text).map(|bytes| Copied::Text(utf16_text(&bytes)))
    } else if available(bitmap) {
        read_bytes(bitmap).map(Copied::Bitmap)
    } else if available(png) {
        read_bytes(png).map(Copied::Png)
    } else {
        None
    }
}

unsafe fn available(format: u32) -> bool {
    format != 0 && IsClipboardFormatAvailable(format).is_ok()
}

unsafe fn read_bytes(format: u32) -> Option<Vec<u8>> {
    let memory = windows::Win32::Foundation::HGLOBAL(GetClipboardData(format).ok()?.0);
    let size = GlobalSize(memory);
    let data = GlobalLock(memory);
    if data.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(data.cast::<u8>(), size).to_vec();
    let _ = GlobalUnlock(memory);
    Some(bytes)
}

/// Clipboard text is UTF-16, ending at the first zero.
fn utf16_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// The text on the clipboard, if there is any.
pub fn read_text(owner: HWND) -> Option<String> {
    unsafe {
        open(owner).ok()?;
        let format = u32::from(CF_UNICODETEXT.0);
        let text = if available(format) {
            read_bytes(format).map(|bytes| utf16_text(&bytes))
        } else {
            None
        };
        let _ = CloseClipboard();
        text
    }
}

/// Puts `text` on the clipboard. `owner` is one of our windows; Windows
/// needs one to accept new clipboard contents.
pub fn copy_text(owner: HWND, text: &str) -> Result<(), String> {
    let bytes: Vec<u8> = text
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    write(owner, &[(u32::from(CF_UNICODETEXT.0), &bytes)])
}

/// Puts a picture on the clipboard both as a bitmap, which every app
/// understands, and as a PNG, which keeps transparency for apps that read it.
pub fn copy_image(owner: HWND, bitmap: &[u8], png: &[u8]) -> Result<(), String> {
    let png_format = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    write(owner, &[(u32::from(CF_DIB.0), bitmap), (png_format, png)])
}

fn write(owner: HWND, formats: &[(u32, &[u8])]) -> Result<(), String> {
    unsafe {
        open(owner)?;
        let result = EmptyClipboard()
            .map_err(message)
            .and_then(|()| formats.iter().try_for_each(|&(f, b)| set_bytes(f, b)));
        let _ = CloseClipboard();
        result
    }
}

/// Another app may have the clipboard open for a moment, so retry briefly.
unsafe fn open(owner: HWND) -> Result<(), String> {
    for _ in 0..10 {
        if OpenClipboard(Some(owner)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("The clipboard is busy. Try again.".into())
}

unsafe fn set_bytes(format: u32, bytes: &[u8]) -> Result<(), String> {
    let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(message)?;
    let target = GlobalLock(memory);
    if target.is_null() {
        let _ = GlobalFree(Some(memory));
        return Err("Couldn't copy to the clipboard.".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
    let _ = GlobalUnlock(memory);
    // Once this succeeds the clipboard owns the memory; until then we do.
    if let Err(error) = SetClipboardData(format, Some(HANDLE(memory.0))) {
        let _ = GlobalFree(Some(memory));
        return Err(error.message());
    }
    Ok(())
}

/// The window in front right now, as a plain number that can be stored.
pub fn foreground_window() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

/// Brings `window` back to the front and presses Ctrl+V in it.
pub fn paste_into(window: isize) {
    unsafe {
        let window = HWND(window as *mut _);
        if !window.is_invalid() {
            let _ = SetForegroundWindow(window);
        }
        // Give the window a moment to take the keyboard.
        std::thread::sleep(Duration::from_millis(60));
        let key = |key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        let down = KEYBD_EVENT_FLAGS(0);
        let inputs = [
            key(VK_CONTROL, down),
            key(VK_V, down),
            key(VK_V, KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}
