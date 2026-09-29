//! Starting Grandium when you sign in to Windows, through the same
//! registry list Task Manager's Startup apps shows.

use super::registry::{self, HKEY_CURRENT_USER};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "Grandium";
/// Passed when Windows starts Grandium, so it goes straight to the tray
/// instead of popping up.
pub const BACKGROUND_ARG: &str = "--background";

/// Adds Grandium to (or takes it off) the programs Windows starts at
/// sign-in. When adding, it's this copy of Grandium that will start, so a
/// moved portable copy is picked up next time Grandium runs.
pub fn set_start_with_windows(on: bool) -> Result<(), String> {
    if !on {
        registry::delete_value(HKEY_CURRENT_USER, RUN_KEY, NAME);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = format!("\"{}\" {BACKGROUND_ARG}", exe.display());
    registry::write_string(HKEY_CURRENT_USER, RUN_KEY, NAME, &command)
}

/// Keeps an existing startup entry pointing at this copy of Grandium (or
/// takes it away), without adding one nobody asked for.
pub fn keep_in_step(on: bool) -> Result<(), String> {
    if startup_command().is_some() || !on {
        set_start_with_windows(on)
    } else {
        Ok(())
    }
}

/// The command Windows runs at sign-in, if Grandium is on the list.
fn startup_command() -> Option<String> {
    registry::read_string(HKEY_CURRENT_USER, RUN_KEY, Some(NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Changes the real startup list of the machine running the tests, and
    /// puts it back.
    #[test]
    fn adds_and_removes_itself() {
        let before = startup_command();
        set_start_with_windows(true).unwrap();
        let command = startup_command().expect("not on the startup list");
        println!("{command}");
        assert!(command.ends_with(BACKGROUND_ARG));
        assert!(command.contains(&*std::env::current_exe().unwrap().to_string_lossy()));
        keep_in_step(true).unwrap();
        assert_eq!(startup_command(), Some(command));
        set_start_with_windows(false).unwrap();
        assert_eq!(startup_command(), None);
        keep_in_step(true).unwrap();
        assert_eq!(startup_command(), None, "only kept in step, never added");
        if let Some(before) = before {
            registry::write_string(HKEY_CURRENT_USER, RUN_KEY, NAME, &before).unwrap();
        }
    }
}
