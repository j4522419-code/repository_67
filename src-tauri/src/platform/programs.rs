//! Installed programs and Store packages, and uninstalling them.

use std::collections::HashMap;

use grandium_core::uninstall::{self, InstalledProgram};
use windows::core::HSTRING;
use windows::ApplicationModel::PackageSignatureKind;
use windows::Management::Deployment::PackageManager;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use super::message;
use super::registry::{self, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

const UNINSTALL_KEYS: &[&str] = &[
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
    // Where 32-bit programs register on 64-bit Windows.
    r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
];

/// Everything in Windows' list of installed programs, for this user and for
/// all users, leaving out the parts Windows itself hides (system
/// components and updates).
pub fn installed_programs() -> Vec<InstalledProgram> {
    let mut programs = Vec::new();
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for list in UNINSTALL_KEYS {
            for entry in registry::subkeys(root, list) {
                let key = format!(r"{list}\{entry}");
                let hidden = registry::read_dword(root, &key, "SystemComponent") == Some(1)
                    || registry::read_string(root, &key, Some("ParentKeyName")).is_some();
                let name = registry::read_string(root, &key, Some("DisplayName"));
                let command = registry::read_string(root, &key, Some("UninstallString"));
                if let (false, Some(name), Some(uninstall_command)) = (hidden, name, command) {
                    programs.push(InstalledProgram {
                        name,
                        uninstall_command,
                        install_location: registry::read_string(
                            root,
                            &key,
                            Some("InstallLocation"),
                        ),
                        display_icon: registry::read_string(root, &key, Some("DisplayIcon")),
                    });
                }
            }
        }
    }
    programs
}

/// This user's Store packages that can be removed, by package family name,
/// with the full name removing needs. Leaves out parts of Windows, shared
/// frameworks, and anything from Microsoft (see
/// [`uninstall::is_microsoft_package`]).
pub fn removable_packages() -> HashMap<String, String> {
    let mut packages = HashMap::new();
    let Ok(manager) = PackageManager::new() else {
        return packages;
    };
    let Ok(found) = manager.FindPackagesByUserSecurityId(&HSTRING::new()) else {
        return packages;
    };
    for package in found {
        let removable = package.IsFramework().is_ok_and(|framework| !framework)
            && package
                .SignatureKind()
                .is_ok_and(|kind| kind != PackageSignatureKind::System);
        if let (true, Ok(id)) = (removable, package.Id()) {
            if let (Ok(family), Ok(full)) = (id.FamilyName(), id.FullName()) {
                let family = family.to_string();
                if !uninstall::is_microsoft_package(&family) {
                    packages.insert(family, full.to_string());
                }
            }
        }
    }
    packages
}

/// Removes a Store package for this user, waiting until it's done.
pub fn remove_package(full_name: &str) -> Result<(), String> {
    let full_name = HSTRING::from(full_name);
    // Package removal reports back on other threads, so this thread joins
    // the multithreaded apartment rather than the UI-style one.
    std::thread::spawn(move || unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let result = PackageManager::new()
            .and_then(|manager| manager.RemovePackageAsync(&full_name))
            .and_then(|removal| removal.join())
            .map(drop)
            .map_err(message);
        if initialized {
            CoUninitialize();
        }
        result
    })
    .join()
    .map_err(|_| "Removing the app failed unexpectedly.".to_string())?
}
