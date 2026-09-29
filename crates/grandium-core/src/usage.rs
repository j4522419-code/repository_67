//! Remembers what gets picked, so frequent and recent choices rank higher
//! ("frecency").

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const DAY: u64 = 24 * 60 * 60;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    items: HashMap<String, Entry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Entry {
    count: u32,
    /// Unix time in seconds.
    last_used: u64,
}

impl Usage {
    /// Records that the item `id` was picked at `now` (Unix seconds).
    pub fn record(&mut self, id: &str, now: u64) {
        let entry = self.items.entry(id.to_string()).or_insert(Entry {
            count: 0,
            last_used: now,
        });
        entry.count = entry.count.saturating_add(1);
        entry.last_used = entry.last_used.max(now);
    }

    /// How much to favor `id`: 0 if never used, growing slowly with each
    /// use and fading over the weeks after the last one.
    pub fn boost(&self, id: &str, now: u64) -> f64 {
        let Some(entry) = self.items.get(id) else {
            return 0.0;
        };
        let age = now.saturating_sub(entry.last_used);
        let recency = match age {
            a if a < DAY => 1.0,
            a if a < 7 * DAY => 0.8,
            a if a < 30 * DAY => 0.5,
            a if a < 90 * DAY => 0.3,
            _ => 0.1,
        };
        f64::from(entry.count).ln_1p() * recency
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_790_000_000;

    #[test]
    fn unknown_items_get_no_boost() {
        assert_eq!(Usage::default().boost("app:chrome", NOW), 0.0);
    }

    #[test]
    fn more_use_means_more_boost() {
        let mut usage = Usage::default();
        usage.record("a", NOW);
        let once = usage.boost("a", NOW);
        usage.record("a", NOW);
        usage.record("a", NOW);
        assert!(once > 0.0);
        assert!(usage.boost("a", NOW) > once);
    }

    #[test]
    fn boost_fades_with_time() {
        let mut usage = Usage::default();
        usage.record("a", NOW);
        let today = usage.boost("a", NOW);
        let next_week = usage.boost("a", NOW + 8 * DAY);
        let next_year = usage.boost("a", NOW + 365 * DAY);
        assert!(today > next_week && next_week > next_year && next_year > 0.0);
    }

    #[test]
    fn clock_going_backwards_keeps_latest_time() {
        let mut usage = Usage::default();
        usage.record("a", NOW);
        usage.record("a", NOW - DAY);
        assert_eq!(usage.items["a"].last_used, NOW);
    }

    #[test]
    fn round_trips_through_json() {
        let mut usage = Usage::default();
        usage.record("app:chrome", NOW);
        let json = serde_json::to_string(&usage).unwrap();
        assert_eq!(serde_json::from_str::<Usage>(&json).unwrap(), usage);
        // Files from older versions, or empty ones, still load.
        assert_eq!(
            serde_json::from_str::<Usage>("{}").unwrap(),
            Usage::default()
        );
    }
}
