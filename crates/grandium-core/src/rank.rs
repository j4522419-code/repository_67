//! Ranking search results: fuzzy matching, boosted by how often and how
//! recently each item was used.

use std::cmp::Ordering;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use unicode_segmentation::UnicodeSegmentation;

/// How much a usage boost of 1.0 multiplies the match score.
const BOOST_WEIGHT: f64 = 0.3;
/// Heavy use can at most triple a match score, so a poor match never
/// buries a good one completely.
const MAX_MULTIPLIER: f64 = 3.0;

/// A matched item, best first when returned from [`Ranker::rank`].
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    /// Position of the item in the list that was ranked.
    pub index: usize,
    pub score: f64,
    /// Matched parts of the title as `[start, end)` ranges in UTF-16 code
    /// units, ready for highlighting in the UI.
    pub highlights: Vec<[u32; 2]>,
}

pub struct Ranker {
    matcher: Matcher,
    buf: Vec<char>,
}

impl Default for Ranker {
    fn default() -> Self {
        let mut config = Config::DEFAULT;
        // People type the start of what they want: "chr" for Chrome.
        config.prefer_prefix = true;
        Self {
            matcher: Matcher::new(config),
            buf: Vec::new(),
        }
    }
}

impl Ranker {
    /// Returns up to `limit` items whose title matches `query`, best first.
    /// Each word of the query must match somewhere in the title.
    /// `boost` gives an item's usage boost, see [`crate::usage::Usage::boost`].
    pub fn rank<T>(
        &mut self,
        query: &str,
        items: &[T],
        title: impl Fn(&T) -> &str,
        boost: impl Fn(&T) -> f64,
        limit: usize,
    ) -> Vec<Ranked> {
        let pattern = Pattern::new(
            query,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        if pattern.atoms.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<(usize, f64)> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let haystack = Utf32Str::new(title(item), &mut self.buf);
                let fuzzy = pattern.score(haystack, &mut self.matcher)?;
                let multiplier = (1.0 + BOOST_WEIGHT * boost(item)).min(MAX_MULTIPLIER);
                Some((index, f64::from(fuzzy) * multiplier))
            })
            .collect();

        matches.sort_by(|(a, a_score), (b, b_score)| {
            b_score
                .partial_cmp(a_score)
                .unwrap_or(Ordering::Equal)
                // Equal scores: shorter titles are closer matches.
                .then_with(|| title(&items[*a]).len().cmp(&title(&items[*b]).len()))
                .then_with(|| title(&items[*a]).cmp(title(&items[*b])))
        });
        matches.truncate(limit);

        matches
            .into_iter()
            .map(|(index, score)| Ranked {
                index,
                score,
                highlights: self.highlights(&pattern, title(&items[index])),
            })
            .collect()
    }

    fn highlights(&mut self, pattern: &Pattern, title: &str) -> Vec<[u32; 2]> {
        let mut indices = Vec::new();
        let haystack = Utf32Str::new(title, &mut self.buf);
        let ascii = matches!(haystack, Utf32Str::Ascii(_));
        pattern.indices(haystack, &mut self.matcher, &mut indices);
        indices.sort_unstable();
        indices.dedup();

        // The matcher counts bytes for ASCII text and grapheme clusters
        // otherwise. Convert either to UTF-16 ranges, which is what
        // JavaScript strings index by.
        let units: Vec<[u32; 2]> = if ascii {
            indices
                .iter()
                .filter(|&&i| title.is_char_boundary(i as usize))
                .map(|&i| {
                    let start = utf16_len(&title[..i as usize]);
                    let ch = title[i as usize..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf16);
                    [start, start + ch as u32]
                })
                .collect()
        } else {
            let mut ranges = Vec::new();
            let mut offset = 0;
            let mut wanted = indices.iter().peekable();
            for (n, grapheme) in title.graphemes(true).enumerate() {
                let len = utf16_len(grapheme);
                if wanted.next_if(|&&i| i as usize == n).is_some() {
                    ranges.push([offset, offset + len]);
                }
                offset += len;
            }
            ranges
        };

        merge_adjacent(units)
    }
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn merge_adjacent(ranges: Vec<[u32; 2]>) -> Vec<[u32; 2]> {
    let mut merged: Vec<[u32; 2]> = Vec::with_capacity(ranges.len());
    for [start, end] in ranges {
        match merged.last_mut() {
            Some(last) if last[1] == start => last[1] = end,
            _ => merged.push([start, end]),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(query: &str, items: &[&str]) -> Vec<String> {
        titles_with_boost(query, items, |_| 0.0)
    }

    fn titles_with_boost(query: &str, items: &[&str], boost: impl Fn(&str) -> f64) -> Vec<String> {
        Ranker::default()
            .rank(query, items, |s| s, |s| boost(s), 10)
            .into_iter()
            .map(|r| items[r.index].to_string())
            .collect()
    }

    const APPS: &[&str] = &[
        "Google Chrome",
        "Chrome Remote Desktop",
        "Calculator",
        "Visual Studio Code",
        "Microsoft Edge",
        "Notepad",
        "Notepad++",
        "Windows Terminal",
    ];

    #[test]
    fn empty_query_matches_nothing() {
        assert!(titles("", APPS).is_empty());
        assert!(titles("   ", APPS).is_empty());
    }

    #[test]
    fn prefix_of_a_word_matches() {
        let found = titles("chr", APPS);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&"Google Chrome".to_string()));
    }

    #[test]
    fn initials_match() {
        assert_eq!(titles("vsc", APPS)[0], "Visual Studio Code");
        assert_eq!(titles("wt", APPS)[0], "Windows Terminal");
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(titles("CALC", APPS), vec!["Calculator"]);
    }

    #[test]
    fn every_word_must_match() {
        assert_eq!(titles("studio code", APPS), vec!["Visual Studio Code"]);
        assert!(titles("studio chrome", APPS).is_empty());
    }

    #[test]
    fn shorter_title_wins_a_tie() {
        assert_eq!(titles("notepad", APPS), vec!["Notepad", "Notepad++"]);
    }

    #[test]
    fn usage_boost_reorders_close_matches() {
        let found = titles_with_boost(
            "notepad",
            APPS,
            |s| if s == "Notepad++" { 2.0 } else { 0.0 },
        );
        assert_eq!(found, vec!["Notepad++", "Notepad"]);
    }

    #[test]
    fn usage_boost_is_capped() {
        // A huge boost can't make a weak match beat a much better one forever.
        let unbounded = |s: &str| if s == "Microsoft Edge" { 1_000.0 } else { 0.0 };
        let items = ["Edge", "Microsoft Edge"];
        let ranked = Ranker::default().rank("edge", &items, |s| s, |s| unbounded(s), 10);
        let edge = ranked.iter().find(|r| r.index == 0).unwrap().score;
        let ms_edge = ranked.iter().find(|r| r.index == 1).unwrap().score;
        assert!(ms_edge <= edge * MAX_MULTIPLIER);
    }

    #[test]
    fn limit_is_respected() {
        let ranked = Ranker::default().rank("e", APPS, |s| s, |_| 0.0, 3);
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn highlights_ascii_matches_as_merged_ranges() {
        let ranked = Ranker::default().rank("chr", &["Google Chrome"], |s| s, |_| 0.0, 1);
        assert_eq!(ranked[0].highlights, vec![[7, 10]]);
    }

    #[test]
    fn highlights_use_utf16_offsets() {
        // "é" is one UTF-16 unit; "𝄞" (outside the BMP) is two.
        let ranked = Ranker::default().rank("rd", &["𝄞 Café Records"], |s| s, |_| 0.0, 1);
        let title: Vec<u16> = "𝄞 Café Records".encode_utf16().collect();
        let highlighted: String = ranked[0]
            .highlights
            .iter()
            .map(|[s, e]| String::from_utf16(&title[*s as usize..*e as usize]).unwrap())
            .collect();
        assert_eq!(highlighted.to_lowercase(), "rd");
    }
}
