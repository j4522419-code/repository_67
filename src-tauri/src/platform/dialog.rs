//! Windows' own dialog for picking a folder.

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

use super::message;

/// Asks for a folder, over `owner`. `None` if the dialog was closed
/// without picking one. The calling thread needs COM.
pub fn pick_folder(owner: HWND, title: &str) -> Result<Option<String>, String> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(message)?;
        let options = dialog.GetOptions().map_err(message)?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .map_err(message)?;
        dialog.SetTitle(&HSTRING::from(title)).map_err(message)?;
        dialog.SetOkButtonLabel(w!("Add folder")).map_err(message)?;
        match dialog.Show(Some(owner)) {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => return Ok(None),
            Err(e) => return Err(message(e)),
        }
        let item = dialog.GetResult().map_err(message)?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(message)?;
        let text = path.to_string().map_err(|e| e.to_string());
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0.cast_const().cast()));
        text.map(Some)
    }
}
