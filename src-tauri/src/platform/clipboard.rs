//! Writing to the clipboard.

use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

use super::message;

/// Puts `text` on the clipboard. `owner` is one of our windows; Windows
/// needs one to accept the new clipboard contents.
pub fn copy_text(owner: HWND, text: &str) -> Result<(), String> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        open(owner)?;
        let result = set_text(&wide);
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

unsafe fn set_text(wide: &[u16]) -> Result<(), String> {
    EmptyClipboard().map_err(message)?;
    let memory = GlobalAlloc(GMEM_MOVEABLE, size_of_val(wide)).map_err(message)?;
    let target = GlobalLock(memory);
    if target.is_null() {
        let _ = GlobalFree(Some(memory));
        return Err("Couldn't copy to the clipboard.".into());
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), target.cast::<u16>(), wide.len());
    let _ = GlobalUnlock(memory);
    // Once this succeeds the clipboard owns the memory; until then we do.
    if let Err(error) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0))) {
        let _ = GlobalFree(Some(memory));
        return Err(error.message());
    }
    Ok(())
}
