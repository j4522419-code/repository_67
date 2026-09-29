//! Rules about which Start menu entries count as apps.

/// File types that Start menu folders often contain but aren't apps:
/// manuals, readmes, license files.
const DOCUMENT_EXTENSIONS: &[&str] = &["chm", "htm", "html", "md", "pdf", "rtf", "txt"];

/// Whether a Start menu entry is worth showing as an app. Hides uninstallers
/// and documentation shortcuts.
pub fn is_app(name: &str, target: Option<&str>) -> bool {
    if name.to_lowercase().contains("uninstall") {
        return false;
    }
    let extension = target
        .and_then(|t| t.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase());
    !matches!(extension, Some(ext) if DOCUMENT_EXTENSIONS.contains(&ext.as_str()))
}

/// Whether the entry can be started with administrator rights (only
/// regular desktop programs can).
pub fn can_run_as_admin(target: Option<&str>) -> bool {
    target.is_some_and(|t| t.to_ascii_lowercase().ends_with(".exe"))
}

/// Whether the entry points at a file on disk (as opposed to a Store app
/// or a web link), so "Open file location" makes sense.
pub fn has_file_location(target: Option<&str>) -> bool {
    target.is_some_and(|t| {
        let bytes = t.as_bytes();
        let drive_path = bytes.len() > 2 && bytes[1] == b':' && bytes[2] == b'\\';
        drive_path || t.starts_with(r"\\")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_regular_apps() {
        assert!(is_app(
            "Google Chrome",
            Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        ));
        assert!(is_app("Calculator", None)); // Store apps have no file path
        assert!(is_app("Some Game", Some("steam://rungameid/123")));
    }

    #[test]
    fn hides_uninstallers() {
        assert!(!is_app("Uninstall Foo", Some(r"C:\Foo\unins000.exe")));
        assert!(!is_app("Foo Uninstaller", None));
    }

    #[test]
    fn hides_documents() {
        assert!(!is_app(
            "Python 3.12 Manuals",
            Some(r"C:\Python312\Doc\python.chm")
        ));
        assert!(!is_app("Readme", Some(r"C:\Foo\README.TXT")));
    }

    #[test]
    fn run_as_admin_needs_an_exe() {
        assert!(can_run_as_admin(Some(r"C:\Windows\System32\cmd.exe")));
        assert!(can_run_as_admin(Some(r"C:\Tools\APP.EXE")));
        assert!(!can_run_as_admin(None));
        assert!(!can_run_as_admin(Some(r"C:\Foo\site.url")));
    }

    #[test]
    fn file_location_needs_a_disk_path() {
        assert!(has_file_location(Some(r"C:\Windows\notepad.exe")));
        assert!(has_file_location(Some(r"\\server\share\tool.exe")));
        assert!(!has_file_location(None));
        assert!(!has_file_location(Some("steam://rungameid/123")));
        assert!(!has_file_location(Some(
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        )));
    }
}
