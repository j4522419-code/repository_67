//! Installed web browsers, and opening links in one of them.

use grandium_core::web;
use windows::core::HSTRING;
use windows::Win32::UI::Shell::SHLoadIndirectString;

use super::registry::{self, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use super::shell::{open_url, start_program, Launch};

/// Where browsers register themselves: the list Windows offers when
/// choosing a default browser.
const BROWSERS_KEY: &str = r"Software\Clients\StartMenuInternet";

pub struct Browser {
    /// The browser's registry name, like `Google Chrome` or
    /// `Firefox-308046B0AF4A39CB`; stays the same across updates.
    pub id: String,
    pub name: String,
    /// How Windows starts it.
    pub command: String,
}

/// Browsers installed for this user or for everyone, by name.
pub fn installed_browsers() -> Vec<Browser> {
    let mut browsers: Vec<Browser> = Vec::new();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for id in registry::subkeys(root, BROWSERS_KEY) {
            // Internet Explorer is retired and just opens Edge.
            let retired = id.eq_ignore_ascii_case("IEXPLORE.EXE");
            if retired || browsers.iter().any(|b| b.id.eq_ignore_ascii_case(&id)) {
                continue;
            }
            let key = format!(r"{BROWSERS_KEY}\{id}");
            let command = registry::read_string(root, &format!(r"{key}\shell\open\command"), None);
            if let Some(command) = command {
                let name = registry::read_string(root, &key, None)
                    .map(|name| resolve(&name))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| id.clone());
                browsers.push(Browser { id, name, command });
            }
        }
    }
    browsers.sort_by_key(|browser| browser.name.to_lowercase());
    browsers
}

/// Some names are references like `@C:\app.exe,-101` to text stored in a
/// program; this looks those up.
fn resolve(name: &str) -> String {
    if !name.starts_with('@') {
        return name.to_string();
    }
    let mut buffer = [0u16; 256];
    match unsafe { SHLoadIndirectString(&HSTRING::from(name), &mut buffer, None) } {
        Ok(()) => {
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            String::from_utf16_lossy(&buffer[..len])
        }
        Err(_) => String::new(),
    }
}

/// Opens a web address in the browser with this ID, or in Windows' default
/// browser when there's no ID or that browser is gone.
pub fn open_link(url: &str, browser: Option<&str>) -> Result<(), String> {
    let chosen = browser.and_then(|id| {
        installed_browsers()
            .into_iter()
            .find(|b| b.id.eq_ignore_ascii_case(id))
    });
    match chosen {
        Some(browser) => {
            let (program, args) = web::browser_command(&browser.command, url);
            start_program(&program, &args, Launch::Normal)
        }
        None => open_url(url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lists the browsers on the machine running the tests (GitHub's
    /// Windows runners have Chrome, Edge and Firefox).
    #[test]
    fn finds_installed_browsers() {
        let browsers = installed_browsers();
        for browser in &browsers {
            println!(
                "{:<30} | {:<40} | {}",
                browser.name, browser.id, browser.command
            );
        }
        assert!(!browsers.is_empty(), "no browsers found");
        assert!(browsers.iter().all(|b| !b.name.starts_with('@')));
    }
}
