//! Word from Grandium's installer, left in the registry
//! (see `windows/installer-hooks.nsh`).

use super::registry::{self, HKEY_CURRENT_USER};

const KEY: &str = r"Software\Grandium";
const SHOW_SETUP: &str = "ShowSetup";

/// Whether Grandium was just installed or updated, so the setup screen
/// should come up to (re)pick the browser and search engine.
pub fn setup_requested() -> bool {
    requested(KEY)
}

/// Called once the setup screen has been saved.
pub fn clear_setup_request() {
    registry::delete_value(HKEY_CURRENT_USER, KEY, SHOW_SETUP);
}

fn requested(key: &str) -> bool {
    registry::read_dword(HKEY_CURRENT_USER, key, SHOW_SETUP) == Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegDeleteTreeW, RegSetKeyValueW, REG_DWORD};

    /// Does what the installer does, under a key of its own.
    #[test]
    fn reads_and_clears_the_installers_request() {
        let key = format!(r"Software\Grandium-test-{}", std::process::id());
        assert!(!requested(&key));
        let one = 1u32;
        let written = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(&key),
                &HSTRING::from(SHOW_SETUP),
                REG_DWORD.0,
                Some((&raw const one).cast()),
                4,
            )
        };
        assert_eq!(written, ERROR_SUCCESS);
        assert!(requested(&key));
        registry::delete_value(HKEY_CURRENT_USER, &key, SHOW_SETUP);
        assert!(!requested(&key));
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&key));
        }
    }
}
