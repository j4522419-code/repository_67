//! What kind of search the typed text asks for. Plain text searches
//! everything; slash commands like `/google cats` pick one thing.

use crate::web::{self, Engine};

#[derive(Debug, PartialEq)]
pub enum Query<'a> {
    /// Plain text: search everything.
    Everything(&'a str),
    /// `/calc 1+2` or `=1+2`: the calculator only.
    Calculator(&'a str),
    /// `/google cats`: search the web with one engine.
    Web(&'static Engine, &'a str),
    /// `/system` or `/system sle`: system commands only.
    System(&'a str),
    /// `/run regedit`: run anything, like the Run box (Win+R).
    Run(&'a str),
    /// `/clip` or `/clip meeting`: clipboard history.
    Clipboard(&'a str),
    /// `/files` or `/files budget`: files and folders only.
    Files(&'a str),
    /// `/` or `/goo`, before a space: picking a slash command.
    Commands(&'a str),
}

#[derive(Debug, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: &'static str,
    /// Shorter names that work too: `/g` for `/google`.
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    /// A built-in icon name for the UI.
    pub glyph: &'static str,
    kind: Kind,
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Web(&'static Engine),
    Calculator,
    System,
    Run,
    Clipboard,
    Files,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "google",
        aliases: &["g"],
        description: "Search Google",
        glyph: "web",
        kind: Kind::Web(&web::GOOGLE),
    },
    SlashCommand {
        name: "youtube",
        aliases: &["yt"],
        description: "Search YouTube",
        glyph: "web",
        kind: Kind::Web(&web::YOUTUBE),
    },
    SlashCommand {
        name: "wiki",
        aliases: &["w", "wikipedia"],
        description: "Search Wikipedia",
        glyph: "web",
        kind: Kind::Web(&web::WIKIPEDIA),
    },
    SlashCommand {
        name: "files",
        aliases: &["f", "file"],
        description: "Find files and folders",
        glyph: "file",
        kind: Kind::Files,
    },
    SlashCommand {
        name: "calc",
        aliases: &["="],
        description: "Calculator",
        glyph: "calculator",
        kind: Kind::Calculator,
    },
    SlashCommand {
        name: "run",
        aliases: &[],
        description: "Run a command or open a path, like Win+R",
        glyph: "run",
        kind: Kind::Run,
    },
    SlashCommand {
        name: "clip",
        aliases: &["clipboard", "c"],
        description: "Clipboard history",
        glyph: "clipboard",
        kind: Kind::Clipboard,
    },
    SlashCommand {
        name: "system",
        aliases: &["sys"],
        description: "Lock, sleep, restart, shut down…",
        glyph: "shutdown",
        kind: Kind::System,
    },
];

impl SlashCommand {
    fn named(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
            || self.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    }

    fn query<'a>(&self, text: &'a str) -> Query<'a> {
        match self.kind {
            Kind::Web(engine) => Query::Web(engine, text),
            Kind::Calculator => Query::Calculator(text),
            Kind::System => Query::System(text),
            Kind::Run => Query::Run(text),
            Kind::Clipboard => Query::Clipboard(text),
            Kind::Files => Query::Files(text),
        }
    }
}

/// Slash commands whose name or alias starts with `prefix`, exact matches
/// first. An empty prefix lists them all.
pub fn matching_commands(prefix: &str) -> Vec<&'static SlashCommand> {
    let prefix = prefix.to_ascii_lowercase();
    let starts = |name: &str| name.starts_with(&prefix);
    let (mut exact, mut partial) = (Vec::new(), Vec::new());
    for command in SLASH_COMMANDS {
        if command.named(&prefix) {
            exact.push(command);
        } else if starts(command.name) || command.aliases.iter().any(|a| starts(a)) {
            partial.push(command);
        }
    }
    exact.append(&mut partial);
    exact
}

pub fn parse(input: &str) -> Query<'_> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix('=') {
        return Query::Calculator(rest.trim());
    }
    if let Some(rest) = input.strip_prefix('/') {
        return match rest.split_once(char::is_whitespace) {
            Some((name, text)) => match SLASH_COMMANDS.iter().find(|c| c.named(name)) {
                Some(command) => command.query(text.trim()),
                None => Query::Commands(name),
            },
            None => Query::Commands(rest),
        };
    }
    Query::Everything(input.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_searches_everything() {
        assert_eq!(parse("  chrome "), Query::Everything("chrome"));
        assert_eq!(parse("g cats"), Query::Everything("g cats"));
        assert_eq!(parse(""), Query::Everything(""));
    }

    #[test]
    fn slash_commands_pick_one_thing() {
        assert_eq!(parse("/google cats"), Query::Web(&web::GOOGLE, "cats"));
        assert_eq!(parse("/g cats"), Query::Web(&web::GOOGLE, "cats"));
        assert_eq!(
            parse("/YT lofi beats"),
            Query::Web(&web::YOUTUBE, "lofi beats")
        );
        assert_eq!(parse("/wiki "), Query::Web(&web::WIKIPEDIA, ""));
        assert_eq!(parse("/calc 2+2"), Query::Calculator("2+2"));
        assert_eq!(parse("/run %temp%"), Query::Run("%temp%"));
        assert_eq!(parse("/system sle"), Query::System("sle"));
        assert_eq!(parse("/sys "), Query::System(""));
        assert_eq!(parse("/clip meeting"), Query::Clipboard("meeting"));
        assert_eq!(parse("/c "), Query::Clipboard(""));
        assert_eq!(parse("/files tax 2025"), Query::Files("tax 2025"));
        assert_eq!(parse("/f "), Query::Files(""));
    }

    #[test]
    fn equals_sign_is_a_calculator_shortcut() {
        assert_eq!(parse("=2+2"), Query::Calculator("2+2"));
        assert_eq!(parse(" = 5 "), Query::Calculator("5"));
    }

    #[test]
    fn slash_without_a_space_picks_a_command() {
        assert_eq!(parse("/"), Query::Commands(""));
        assert_eq!(parse("/goo"), Query::Commands("goo"));
        assert_eq!(parse("/google"), Query::Commands("google"));
        assert_eq!(parse("/nope cats"), Query::Commands("nope"));
    }

    #[test]
    fn lists_matching_commands_exact_first() {
        let names =
            |prefix| -> Vec<&str> { matching_commands(prefix).iter().map(|c| c.name).collect() };
        assert_eq!(names("").len(), SLASH_COMMANDS.len());
        assert_eq!(names("goo"), vec!["google"]);
        // "w" is Wikipedia's alias, so it comes first.
        assert_eq!(names("w")[0], "wiki");
        assert_eq!(names("s"), vec!["system"]);
        assert_eq!(names("c"), vec!["clip", "calc"]);
        assert_eq!(names("fi"), vec!["files"]);
        assert!(names("zzz").is_empty());
    }
}
