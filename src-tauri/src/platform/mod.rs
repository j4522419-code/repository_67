//! Windows APIs, wrapped for the rest of the app.

mod clipboard;
mod power;
mod shell;

use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};

pub use clipboard::copy_text;
pub use power::run_system_command;
pub use shell::{app_icon, installed_apps, launch, open_url, show_in_explorer, Launch, ShellApp};

/// Keeps COM initialized on the current thread while alive; the shell
/// APIs need it.
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

fn message(error: windows::core::Error) -> String {
    error.message()
}
