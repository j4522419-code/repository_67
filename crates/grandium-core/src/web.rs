//! Web search engines, their search URLs, and opening links in a chosen
//! browser.

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::run::split_command_line;

#[derive(Debug, PartialEq, Eq)]
pub struct Engine {
    /// Short name that identifies the engine in result IDs.
    pub keyword: &'static str,
    pub name: &'static str,
    /// `{query}` is replaced by the encoded search text.
    url: &'static str,
}

pub const GOOGLE: Engine = Engine {
    keyword: "g",
    name: "Google",
    url: "https://www.google.com/search?q={query}",
};
pub const YOUTUBE: Engine = Engine {
    keyword: "yt",
    name: "YouTube",
    url: "https://www.youtube.com/results?search_query={query}",
};
pub const WIKIPEDIA: Engine = Engine {
    keyword: "w",
    name: "Wikipedia",
    url: "https://en.wikipedia.org/w/index.php?search={query}",
};

pub const BING: Engine = Engine {
    keyword: "b",
    name: "Bing",
    url: "https://www.bing.com/search?q={query}",
};
pub const DUCKDUCKGO: Engine = Engine {
    keyword: "ddg",
    name: "DuckDuckGo",
    url: "https://duckduckgo.com/?q={query}",
};
pub const BRAVE: Engine = Engine {
    keyword: "brave",
    name: "Brave Search",
    url: "https://search.brave.com/search?q={query}",
};
pub const ECOSIA: Engine = Engine {
    keyword: "ecosia",
    name: "Ecosia",
    url: "https://www.ecosia.org/search?q={query}",
};

pub const ENGINES: &[Engine] = &[GOOGLE, BING, DUCKDUCKGO, BRAVE, ECOSIA, YOUTUBE, WIKIPEDIA];

/// The engines that can be picked for everyday web searches (the "Search
/// … for" suggestion under plain searches).
pub const SEARCH_ENGINES: &[&Engine] = &[&GOOGLE, &BING, &DUCKDUCKGO, &BRAVE, &ECOSIA];

pub fn engine(keyword: &str) -> Option<&'static Engine> {
    ENGINES
        .iter()
        .find(|engine| engine.keyword.eq_ignore_ascii_case(keyword))
}

/// The chosen everyday search engine, or Google if the choice is unknown.
pub fn search_engine(keyword: &str) -> &'static Engine {
    SEARCH_ENGINES
        .iter()
        .find(|engine| engine.keyword.eq_ignore_ascii_case(keyword))
        .copied()
        .unwrap_or(&GOOGLE)
}

/// How to open `url` with a browser, given the command Windows registers
/// for it (like `"C:\Program Files\...\chrome.exe"`): the program, and its
/// arguments with the address added (or put where the command says `%1`).
pub fn browser_command(command: &str, url: &str) -> (String, String) {
    let (program, args) = split_command_line(command);
    let args = if args.contains("%1") {
        args.replace("%1", url)
    } else if args.is_empty() {
        format!("\"{url}\"")
    } else {
        format!("{args} \"{url}\"")
    };
    (program, args)
}

impl Engine {
    pub fn search_url(&self, query: &str) -> String {
        let encoded = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
        self.url.replace("{query}", &encoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_engines_by_keyword() {
        assert_eq!(engine("g"), Some(&GOOGLE));
        assert_eq!(engine("YT"), Some(&YOUTUBE));
        assert_eq!(engine("ddg"), Some(&DUCKDUCKGO));
        assert_eq!(engine("x"), None);
    }

    #[test]
    fn everyday_search_engine_falls_back_to_google() {
        assert_eq!(search_engine("b"), &BING);
        assert_eq!(search_engine("nope"), &GOOGLE);
        // YouTube is a site search, not an everyday engine.
        assert_eq!(search_engine("yt"), &GOOGLE);
    }

    #[test]
    fn opens_links_in_a_chosen_browser() {
        let url = "https://duckduckgo.com/?q=cats";
        let run = |command| browser_command(command, url);
        assert_eq!(
            run(r#""C:\Program Files\Google\Chrome\Application\chrome.exe""#),
            (
                r"C:\Program Files\Google\Chrome\Application\chrome.exe".to_string(),
                format!("\"{url}\"")
            )
        );
        assert_eq!(
            run(r#""C:\Firefox\firefox.exe" -osint -url "%1""#),
            (
                r"C:\Firefox\firefox.exe".to_string(),
                format!("-osint -url \"{url}\"")
            )
        );
        assert_eq!(
            run(r#""C:\Brave\brave.exe" --profile-directory=Default"#).1,
            format!("--profile-directory=Default \"{url}\"")
        );
    }

    #[test]
    fn encodes_the_search_text() {
        assert_eq!(
            GOOGLE.search_url("c++ & rust?"),
            "https://www.google.com/search?q=c%2B%2B%20%26%20rust%3F"
        );
        assert_eq!(
            WIKIPEDIA.search_url("Zürich"),
            "https://en.wikipedia.org/w/index.php?search=Z%C3%BCrich"
        );
    }
}
