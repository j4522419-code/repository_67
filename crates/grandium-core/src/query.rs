//! What kind of search the typed text asks for, based on its prefix.

use crate::web::{self, Engine};

#[derive(Debug, PartialEq)]
pub enum Query<'a> {
    /// Plain text: search everything.
    Everything(&'a str),
    /// `=1+2`: the calculator only.
    Calculator(&'a str),
    /// `g cats`: search the web with one engine.
    Web(&'static Engine, &'a str),
    /// `>` or `> sle`: system commands only.
    System(&'a str),
}

pub fn parse(input: &str) -> Query<'_> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix('=') {
        return Query::Calculator(rest.trim());
    }
    if let Some(rest) = input.strip_prefix('>') {
        return Query::System(rest.trim());
    }
    // A web keyword only counts with a space after it: "g cats", not "gimp".
    if let Some((keyword, rest)) = input.split_once(' ') {
        if let Some(engine) = web::engine(keyword) {
            return Query::Web(engine, rest.trim());
        }
    }
    Query::Everything(input.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_searches_everything() {
        assert_eq!(parse("  chrome "), Query::Everything("chrome"));
        assert_eq!(parse("gimp"), Query::Everything("gimp"));
        assert_eq!(parse("g"), Query::Everything("g"));
        assert_eq!(parse(""), Query::Everything(""));
    }

    #[test]
    fn equals_sign_means_calculator() {
        assert_eq!(parse("=2+2"), Query::Calculator("2+2"));
        assert_eq!(parse(" = 5 "), Query::Calculator("5"));
    }

    #[test]
    fn angle_bracket_means_system_commands() {
        assert_eq!(parse(">"), Query::System(""));
        assert_eq!(parse("> sle"), Query::System("sle"));
    }

    #[test]
    fn keyword_and_space_means_web_search() {
        assert_eq!(parse("g cats"), Query::Web(&web::GOOGLE, "cats"));
        assert_eq!(
            parse("YT lofi beats"),
            Query::Web(&web::YOUTUBE, "lofi beats")
        );
        assert_eq!(parse("w "), Query::Web(&web::WIKIPEDIA, ""));
    }
}
