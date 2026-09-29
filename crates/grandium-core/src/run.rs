//! Things you'd type into the Run box (Win+R): paths, environment
//! variables like `%temp%`, shell links like `shell:startup`, and commands.

/// Whether typed text is a place to open rather than a search: a path, an
/// environment variable, a network share, a shell or settings link, or a
/// web address.
pub fn looks_like_location(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let drive_path = bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || bytes[2] == b'\\' || bytes[2] == b'/');
    drive_path
        || lower.starts_with('%')
        || lower.starts_with(r"\\")
        || ["shell:", "ms-settings:", "http://", "https://"]
            .iter()
            .any(|scheme| lower.starts_with(scheme))
}

/// Whether typed text could be the name of a program, like `regedit` or
/// `services.msc`: a single word of letters, digits, dots, dashes and
/// underscores.
pub fn looks_like_program(text: &str) -> bool {
    text.len() >= 2
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && text.chars().any(|c| c.is_ascii_alphabetic())
}

const PROGRAM_EXTENSIONS: &[&str] = &[".exe", ".com", ".bat", ".cmd", ".msc", ".cpl"];

/// Splits a command line into the program and its arguments: a quoted
/// program, or an unquoted path that ends in a program extension (Windows
/// allows spaces in those, like `C:\Program Files\App\uninstall.exe /S`),
/// or else everything up to the first space.
pub fn split_command_line(command: &str) -> (String, String) {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        return match rest.split_once('"') {
            Some((program, args)) => (program.to_string(), args.trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
    }

    let lower = command.to_ascii_lowercase();
    if PROGRAM_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return (command.to_string(), String::new());
    }
    let program_end = PROGRAM_EXTENSIONS
        .iter()
        .filter_map(|ext| lower.find(&format!("{ext} ")).map(|at| at + ext.len()))
        .min();
    let (program, args) = match program_end {
        Some(end) => command.split_at(end),
        None => command.split_once(' ').unwrap_or((command, "")),
    };
    (program.to_string(), args.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_locations() {
        for text in [
            "%temp%",
            "%APPDATA%\\Microsoft",
            r"C:\Users",
            "c:/windows",
            "D:",
            r"\\server\share",
            "shell:startup",
            "Shell:Downloads",
            "ms-settings:display",
            "https://example.com",
        ] {
            assert!(looks_like_location(text), "{text}");
        }
    }

    #[test]
    fn plain_searches_are_not_locations() {
        for text in ["chrome", "c++", "cats: the musical", "ab", "shell", "100%"] {
            assert!(!looks_like_location(text), "{text}");
        }
    }

    #[test]
    fn recognizes_program_names() {
        for text in ["regedit", "cmd", "services.msc", "appwiz.cpl", "7z"] {
            assert!(looks_like_program(text), "{text}");
        }
        for text in ["a", "notepad plus", "123", "c:\\x", "%temp%"] {
            assert!(!looks_like_program(text), "{text}");
        }
    }

    fn split(command: &str, program: &str, args: &str) {
        assert_eq!(
            split_command_line(command),
            (program.to_string(), args.to_string()),
            "{command}"
        );
    }

    #[test]
    fn splits_quoted_programs() {
        split(
            r#""C:\Program Files\App\unins000.exe" /SILENT"#,
            r"C:\Program Files\App\unins000.exe",
            "/SILENT",
        );
        split(r#""C:\a b\c.exe""#, r"C:\a b\c.exe", "");
    }

    #[test]
    fn splits_unquoted_paths_with_spaces() {
        split(
            r"C:\Program Files\App\uninstall.exe /S",
            r"C:\Program Files\App\uninstall.exe",
            "/S",
        );
        split(
            r"C:\Program Files\App\app.exe",
            r"C:\Program Files\App\app.exe",
            "",
        );
    }

    #[test]
    fn splits_simple_commands() {
        split("cmd /k dir", "cmd", "/k dir");
        split("regedit", "regedit", "");
        split("MsiExec.exe /X{1234-5678}", "MsiExec.exe", "/X{1234-5678}");
    }
}
