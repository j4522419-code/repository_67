//! Working out how to uninstall an app, cautiously: the wrong answer would
//! remove the wrong program, so when in doubt there's no answer.

use std::collections::HashSet;

use crate::run::split_command_line;

/// A program from Windows' list of installed programs (the one Settings >
/// Apps shows), as registered by its installer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledProgram {
    pub name: String,
    /// The installer's own uninstall command.
    pub uninstall_command: String,
    pub install_location: Option<String>,
    /// Usually the program's main `.exe`, sometimes with an icon index
    /// like `,0`.
    pub display_icon: Option<String>,
}

/// The installed program an app belongs to, found by one of these rules,
/// strongest first:
/// 1. the program's icon is the app's own file;
/// 2. the app's file is inside the program's own install folder;
/// 3. the names are exactly the same.
///
/// Gives up (`None`) when the strongest matching rule points at different
/// programs, since there's no telling which is right.
pub fn find_program<'a>(
    programs: &'a [InstalledProgram],
    app_name: &str,
    target: Option<&str>,
) -> Option<&'a InstalledProgram> {
    let target = target.map(normalize_path);
    // Parts of Windows can't be uninstalled like programs.
    if target.as_deref().is_some_and(is_in_windows_folder) {
        return None;
    }
    let icon_is_target = |p: &InstalledProgram| {
        let icon = p.display_icon.as_deref().map(icon_path);
        target.is_some() && icon == target
    };
    let target_in_folder = |p: &InstalledProgram| match (&target, &p.install_location) {
        (Some(target), Some(location)) => {
            let folder = normalize_path(location);
            is_specific_folder(&folder) && target.starts_with(&format!("{folder}\\"))
        }
        _ => false,
    };
    let same_name = |p: &InstalledProgram| p.name.trim().eq_ignore_ascii_case(app_name.trim());

    let rules: [&dyn Fn(&InstalledProgram) -> bool; 3] =
        [&icon_is_target, &target_in_folder, &same_name];
    for rule in rules {
        let matches: Vec<&InstalledProgram> = programs.iter().filter(|p| rule(p)).collect();
        let commands: HashSet<&str> = matches.iter().map(|p| p.uninstall_command.trim()).collect();
        match commands.len() {
            0 => continue,
            1 => return matches.first().copied(),
            _ => return None,
        }
    }
    None
}

/// Lowercase, backslashes, no quotes or trailing slash: comparable paths.
fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn is_in_windows_folder(path: &str) -> bool {
    path.split('\\').nth(1) == Some("windows")
}

/// `"C:\App\app.exe",0` → `c:\app\app.exe`
fn icon_path(icon: &str) -> String {
    let icon = icon.trim();
    let without_index = match icon.rsplit_once(',') {
        Some((path, index)) if index.trim().parse::<i32>().is_ok() => path,
        _ => icon,
    };
    normalize_path(without_index)
}

/// Whether an install folder belongs to one program, rather than being a
/// shared place like `C:\Program Files` or a user's AppData folder.
fn is_specific_folder(folder: &str) -> bool {
    let parts: Vec<&str> = folder.split('\\').filter(|p| !p.is_empty()).collect();
    let deep_enough = match parts.get(1) {
        Some(&"windows") => false,
        // C:\Users\<name>\AppData\Local\<program> at the least.
        Some(&"users") => parts.len() >= 6,
        _ => parts.len() >= 3,
    };
    deep_enough && !matches!(parts.last(), Some(&"common files") | Some(&"programs"))
}

/// Splits an uninstall command into the program and its arguments. For
/// Windows Installer packages, whose registered command often *changes* the
/// install (`/I`), it asks to remove instead (`/X`), like Settings does.
pub fn uninstall_command_line(command: &str) -> (String, String) {
    let (program, args) = split_command_line(command);
    let file = program.rsplit(['\\', '/']).next().unwrap_or(&program);
    let msiexec = file.eq_ignore_ascii_case("msiexec.exe") || file.eq_ignore_ascii_case("msiexec");
    let args = match args.get(..2) {
        Some(flag) if msiexec && flag.eq_ignore_ascii_case("/i") => format!("/X{}", &args[2..]),
        _ => args,
    };
    (program, args)
}

/// Store apps are started by an ID like `Microsoft.WindowsCalculator_8wekyb3d8bbwe!App`;
/// the part before `!` names the package they come in.
pub fn package_family(app_id: &str) -> Option<&str> {
    let (family, _app) = app_id.split_once('!')?;
    (family.contains('_') && !family.contains('\\')).then_some(family)
}

/// Publisher IDs of Microsoft's packages: `8wekyb3d8bbwe` for its Store
/// apps, `cw5n1h2txyewy` for parts of Windows.
const MICROSOFT_PUBLISHERS: &[&str] = &["8wekyb3d8bbwe", "cw5n1h2txyewy"];

/// Whether a Store package comes from Microsoft. Some of those are parts of
/// Windows (like Windows Security) that must never be removed, and packages
/// don't say which, so none of Microsoft's are offered.
pub fn is_microsoft_package(family: &str) -> bool {
    let publisher = family.rsplit('_').next().unwrap_or_default();
    let name = family.to_ascii_lowercase();
    MICROSOFT_PUBLISHERS.contains(&publisher.to_ascii_lowercase().as_str())
        || ["microsoft.", "microsoftwindows.", "windows."]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(
        name: &str,
        command: &str,
        location: Option<&str>,
        icon: Option<&str>,
    ) -> InstalledProgram {
        InstalledProgram {
            name: name.into(),
            uninstall_command: command.into(),
            install_location: location.map(Into::into),
            display_icon: icon.map(Into::into),
        }
    }

    fn programs() -> Vec<InstalledProgram> {
        vec![
            program(
                "Google Chrome",
                r#""C:\Program Files\Google\Chrome\Application\120.0\Installer\setup.exe" --uninstall"#,
                Some(r"C:\Program Files\Google\Chrome\Application"),
                Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe,0"),
            ),
            program(
                "7-Zip 23.01 (x64)",
                r#""C:\Program Files\7-Zip\Uninstall.exe""#,
                Some(r"C:\Program Files\7-Zip\"),
                None,
            ),
            program(
                "Discord",
                r#""C:\Users\me\AppData\Local\Discord\Update.exe" --uninstall"#,
                Some(r"C:\Users\me\AppData\Local\Discord"),
                None,
            ),
            program(
                "Sloppy Installer",
                r#""C:\Program Files\Sloppy\uninst.exe""#,
                Some(r"C:\Program Files"),
                None,
            ),
            program("Notes", "MsiExec.exe /X{AAAA}", None, None),
        ]
    }

    #[test]
    fn matches_by_icon() {
        let all = programs();
        let found = find_program(
            &all,
            "Chrome",
            Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        );
        assert_eq!(found.unwrap().name, "Google Chrome");
    }

    #[test]
    fn matches_by_install_folder() {
        let all = programs();
        let found = find_program(
            &all,
            "7-Zip File Manager",
            Some(r"C:\Program Files\7-Zip\7zFM.exe"),
        );
        assert_eq!(found.unwrap().name, "7-Zip 23.01 (x64)");
        let found = find_program(
            &all,
            "Discord",
            Some(r"C:\Users\me\AppData\Local\Discord\Update.exe"),
        );
        assert_eq!(found.unwrap().name, "Discord");
    }

    #[test]
    fn ignores_shared_install_folders() {
        let all = programs();
        // "Sloppy Installer" claims all of C:\Program Files; it must not
        // capture other programs.
        let found = find_program(&all, "Other App", Some(r"C:\Program Files\Other\other.exe"));
        assert_eq!(found, None);
    }

    #[test]
    fn matches_by_exact_name_last() {
        let all = programs();
        let found = find_program(&all, "notes", Some(r"C:\Tools\notes.exe"));
        assert_eq!(found.unwrap().name, "Notes");
        assert_eq!(
            find_program(&all, "Note", Some(r"C:\Tools\notes.exe")),
            None
        );
    }

    #[test]
    fn built_in_windows_tools_have_no_uninstaller() {
        let mut all = programs();
        all.push(program(
            "Windows Thing",
            "x.exe",
            Some(r"C:\Windows\System32"),
            None,
        ));
        assert_eq!(
            find_program(&all, "Notepad", Some(r"C:\Windows\System32\notepad.exe")),
            None
        );
        // Even when something with the same name is installed.
        all.push(program("Notepad", "other.exe", None, None));
        assert_eq!(
            find_program(&all, "Notepad", Some(r"C:\Windows\notepad.exe")),
            None
        );
    }

    #[test]
    fn gives_up_when_programs_disagree() {
        let mut all = programs();
        all.push(program("Notes", "C:\\Other\\remove.exe", None, None));
        assert_eq!(
            find_program(&all, "Notes", Some(r"C:\Tools\notes.exe")),
            None
        );
    }

    #[test]
    fn duplicate_entries_for_one_program_are_fine() {
        let mut all = programs();
        all.push(program("Notes", "MsiExec.exe /X{AAAA}", None, None));
        assert_eq!(find_program(&all, "Notes", None).unwrap().name, "Notes");
    }

    #[test]
    fn folder_specificity() {
        assert!(is_specific_folder(r"c:\program files\7-zip"));
        assert!(is_specific_folder(r"d:\games\thing"));
        assert!(is_specific_folder(r"c:\users\me\appdata\local\discord"));
        assert!(!is_specific_folder(r"c:\program files"));
        assert!(!is_specific_folder(r"c:\program files\common files"));
        assert!(!is_specific_folder(r"c:\users\me\appdata\local"));
        assert!(!is_specific_folder(r"c:\users\me\appdata\local\programs"));
        assert!(!is_specific_folder(r"c:\windows\system32"));
    }

    #[test]
    fn msi_commands_remove_instead_of_change() {
        assert_eq!(
            uninstall_command_line("MsiExec.exe /I{1234-ABCD}"),
            ("MsiExec.exe".to_string(), "/X{1234-ABCD}".to_string())
        );
        assert_eq!(
            uninstall_command_line("msiexec /x {1234}"),
            ("msiexec".to_string(), "/x {1234}".to_string())
        );
        assert_eq!(
            uninstall_command_line(r#""C:\App\uninst.exe" /I"#),
            (r"C:\App\uninst.exe".to_string(), "/I".to_string())
        );
    }

    #[test]
    fn recognizes_microsoft_packages() {
        assert!(is_microsoft_package("Microsoft.SecHealthUI_8wekyb3d8bbwe"));
        assert!(is_microsoft_package(
            "Microsoft.WindowsTerminal_8wekyb3d8bbwe"
        ));
        assert!(is_microsoft_package(
            "MicrosoftWindows.Client.CBS_cw5n1h2txyewy"
        ));
        assert!(is_microsoft_package("Windows.PrintDialog_cw5n1h2txyewy"));
        assert!(!is_microsoft_package(
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0"
        ));
        assert!(!is_microsoft_package(
            "5319275A.WhatsAppDesktop_cv1g1gvanyjgm"
        ));
    }

    #[test]
    fn finds_store_package_families() {
        assert_eq!(
            package_family("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"),
            Some("Microsoft.WindowsCalculator_8wekyb3d8bbwe")
        );
        assert_eq!(package_family("Chrome"), None);
        assert_eq!(package_family(r"C:\Tools\a!b_c.exe"), None);
    }
}
