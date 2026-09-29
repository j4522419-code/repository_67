//! Windows APIs, wrapped for the rest of the app.

mod browsers;
pub mod clipboard;
pub mod locale;
mod power;
mod programs;
pub mod protect;
mod registry;
mod shell;
pub mod watch;

use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};

pub use browsers::{installed_browsers, open_link};
pub use clipboard::copy_text;
pub use power::run_system_command;
pub use programs::{installed_programs, removable_packages, remove_package};
pub use shell::{
    app_icon, expand_env, file_icon, find_program, installed_apps, launch, open_path, recycle,
    run_command, show_in_explorer, start_program, user_folders, Launch, ShellApp,
};

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
