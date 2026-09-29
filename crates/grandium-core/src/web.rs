//! Web search engines and their search URLs.

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

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

pub const ENGINES: &[Engine] = &[GOOGLE, YOUTUBE, WIKIPEDIA];

/// Used for the "search the web" suggestion under plain searches.
pub const DEFAULT: &Engine = &GOOGLE;

pub fn engine(keyword: &str) -> Option<&'static Engine> {
    ENGINES
        .iter()
        .find(|engine| engine.keyword.eq_ignore_ascii_case(keyword))
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
        assert_eq!(engine("x"), None);
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
