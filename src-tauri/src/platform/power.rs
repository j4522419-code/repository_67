//! System commands: lock, sleep, restart, shut down, sign out, and
//! emptying the Recycle Bin. The IDs match `grandium_core::system`.

use std::os::windows::process::CommandExt;
use std::process::Command;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, E_UNEXPECTED, HANDLE};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken, CREATE_NO_WINDOW};
use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION};

use super::message;

pub fn run_system_command(id: &str) -> Result<(), String> {
    match id {
        "lock" => unsafe { LockWorkStation() }.map_err(message),
        "sleep" => sleep(),
        "restart" => shutdown_exe(&["/r", "/t", "0"]),
        "shutdown" => shutdown_exe(&["/s", "/t", "0"]),
        "signout" => shutdown_exe(&["/l"]),
        "emptybin" => empty_recycle_bin(),
        other => Err(format!("Unknown command: {other}")),
    }
}

fn sleep() -> Result<(), String> {
    unsafe {
        enable_shutdown_privilege();
        // Sleep, not hibernate. Returns once the PC wakes up again.
        if SetSuspendState(false, false, false) {
            Ok(())
        } else {
            Err(windows::core::Error::from_thread().message())
        }
    }
}

/// Sleeping needs the "shut down the system" privilege switched on. Every
/// user has it, but it starts out off.
unsafe fn enable_shutdown_privilege() {
    let mut token = HANDLE::default();
    if OpenProcessToken(
        GetCurrentProcess(),
        TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
        &mut token,
    )
    .is_err()
    {
        return;
    }
    let mut privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        ..Default::default()
    };
    if LookupPrivilegeValueW(
        PCWSTR::null(),
        SE_SHUTDOWN_NAME,
        &mut privileges.Privileges[0].Luid,
    )
    .is_ok()
    {
        privileges.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
        let _ = AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None);
    }
    let _ = CloseHandle(token);
}

/// Windows' own `shutdown` tool handles restart, shut down and sign out,
/// including the privileges they need.
fn shutdown_exe(args: &[&str]) -> Result<(), String> {
    Command::new("shutdown.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW.0)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

fn empty_recycle_bin() -> Result<(), String> {
    // Grandium asks for confirmation itself, so skip Windows' dialog.
    match unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOCONFIRMATION) } {
        Ok(()) => Ok(()),
        // What Windows says when the bin is already empty.
        Err(error) if error.code() == E_UNEXPECTED => Ok(()),
        Err(error) => Err(error.message()),
    }
}
