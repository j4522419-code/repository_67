//! System commands: lock, sleep, restart and so on.

use std::collections::HashSet;

use crate::rank::Ranker;

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub id: &'static str,
    pub name: &'static str,
    /// Other words people use for it: "reboot" for Restart.
    aliases: &'static [&'static str],
    /// Asked before running commands that can lose work or data.
    pub confirm: Option<&'static str>,
}

pub const COMMANDS: &[Command] = &[
    Command {
        id: "lock",
        name: "Lock",
        aliases: &["lock screen", "lock computer"],
        confirm: None,
    },
    Command {
        id: "sleep",
        name: "Sleep",
        aliases: &["suspend"],
        confirm: None,
    },
    Command {
        id: "restart",
        name: "Restart",
        aliases: &["reboot"],
        confirm: Some("Restart your PC now? Unsaved work in open apps may be lost."),
    },
    Command {
        id: "shutdown",
        name: "Shut down",
        aliases: &["shutdown", "power off", "turn off"],
        confirm: Some("Shut down your PC now? Unsaved work in open apps may be lost."),
    },
    Command {
        id: "signout",
        name: "Sign out",
        aliases: &["log off", "log out", "logout"],
        confirm: Some("Sign out now? Apps you have open will close."),
    },
    Command {
        id: "emptybin",
        name: "Empty Recycle Bin",
        aliases: &["empty trash", "recycle bin"],
        confirm: Some("Permanently delete everything in the Recycle Bin?"),
    },
];

pub fn command(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.id == id)
}

#[derive(Debug)]
pub struct Match {
    pub command: &'static Command,
    pub score: f64,
    /// Highlights in the command's name; empty when an alias matched.
    pub highlights: Vec<[u32; 2]>,
}

/// Commands whose name or an alias matches `query`, best first.
pub fn search(ranker: &mut Ranker, query: &str, boost: impl Fn(&Command) -> f64) -> Vec<Match> {
    let entries: Vec<(&'static Command, &'static str)> = COMMANDS
        .iter()
        .flat_map(|command| {
            std::iter::once(command.name)
                .chain(command.aliases.iter().copied())
                .map(move |text| (command, text))
        })
        .collect();
    let ranked = ranker.rank(
        query,
        &entries,
        |entry| entry.1,
        |entry| boost(entry.0),
        entries.len(),
    );

    let mut seen = HashSet::new();
    ranked
        .into_iter()
        .filter_map(|ranked| {
            let (command, text) = entries[ranked.index];
            seen.insert(command.id).then(|| Match {
                command,
                score: ranked.score,
                highlights: if text == command.name {
                    ranked.highlights
                } else {
                    Vec::new()
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(query: &str) -> Vec<&'static str> {
        search(&mut Ranker::default(), query, |_| 0.0)
            .into_iter()
            .map(|m| m.command.name)
            .collect()
    }

    #[test]
    fn matches_by_name() {
        assert_eq!(names("lock")[0], "Lock");
        assert_eq!(names("shut")[0], "Shut down");
        assert_eq!(names("empty")[0], "Empty Recycle Bin");
    }

    #[test]
    fn matches_by_alias_without_highlights() {
        let found = search(&mut Ranker::default(), "reboot", |_| 0.0);
        assert_eq!(found[0].command.name, "Restart");
        assert!(found[0].highlights.is_empty());
        assert_eq!(names("log off")[0], "Sign out");
        assert_eq!(names("power off")[0], "Shut down");
    }

    #[test]
    fn lists_each_command_once() {
        // "sign out" and "log out" both match "out".
        let found = names("out");
        let unique: HashSet<_> = found.iter().collect();
        assert_eq!(found.len(), unique.len());
    }

    #[test]
    fn nothing_for_unrelated_text() {
        assert!(names("zzzz").is_empty());
    }

    #[test]
    fn risky_commands_ask_first() {
        for id in ["restart", "shutdown", "signout", "emptybin"] {
            assert!(command(id).unwrap().confirm.is_some(), "{id}");
        }
        for id in ["lock", "sleep"] {
            assert!(command(id).unwrap().confirm.is_none(), "{id}");
        }
    }
}
