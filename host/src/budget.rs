//! A daily token budget for a bot, or for all of them. The host tallies the tokens each finished turn used; at 80%
//! of the limit the bot's chat gets a notice, and at 100% its routines pause so a runaway job can't spend the rest
//! of the day. The user's own messages are never refused. Local only: the tally comes from the agents' own usage
//! reports, not from a provider.
//!
//! Idea from mercury-agent's `bot-manager.ts` (MIT); no code copied.

use crate::LockExt;
use crate::store::Store;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

const STATE_KEY: &str = "budget.v1";
/// The key the fleet-wide limit and tally are kept under.
pub const FLEET: &str = "*";
const DAY_MS: i64 = 86_400_000;
/// The share of the limit that earns a warning.
const WARN_PERCENT: u64 = 80;

/// How a day's use stands against a limit.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Within,
    Warning,
    Exceeded,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
struct Tally {
    day: i64,
    tokens: u64,
    /// The level already announced today, so each is said once.
    #[serde(default)]
    announced: Option<LevelMark>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
enum LevelMark {
    Warned,
    Exceeded,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct State {
    /// Daily limit per bot id (`FLEET` for all bots together); a missing key has no limit.
    limits: HashMap<String, u64>,
    tallies: HashMap<String, Tally>,
}

/// What a finished turn did to a budget: nothing to say, or a level to announce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crossed {
    pub level: Level,
    pub used: u64,
    pub limit: u64,
}

pub fn day_of(now_ms: i64) -> i64 {
    now_ms.div_euclid(DAY_MS)
}

fn level(used: u64, limit: u64) -> Level {
    if used >= limit {
        Level::Exceeded
    } else if used.saturating_mul(100) >= limit.saturating_mul(WARN_PERCENT) {
        Level::Warning
    } else {
        Level::Within
    }
}

impl State {
    /// Adds `tokens` to `key`'s tally for `day`. Returns the level reached when it's one not yet announced today.
    fn add(&mut self, key: &str, day: i64, tokens: u64) -> Option<Crossed> {
        let tally = self.tallies.entry(key.to_owned()).or_default();
        if tally.day != day {
            *tally = Tally { day, ..Tally::default() };
        }
        tally.tokens += tokens;
        let limit = *self.limits.get(key)?;
        let reached = level(tally.tokens, limit);
        let mark = match reached {
            Level::Within => return None,
            Level::Warning => LevelMark::Warned,
            Level::Exceeded => LevelMark::Exceeded,
        };
        if matches!(tally.announced, Some(seen) if seen == mark || seen == LevelMark::Exceeded) {
            return None;
        }
        tally.announced = Some(mark);
        Some(Crossed { level: reached, used: tally.tokens, limit })
    }
}

#[derive(Default)]
pub struct Budgets(Mutex<State>);

impl Budgets {
    pub fn load(store: &Store) -> Self {
        let state = store.kv_read(STATE_KEY).ok().flatten().and_then(|s| serde_json::from_str(&s).ok());
        Self(Mutex::new(state.unwrap_or_default()))
    }

    fn save(store: &Store, state: &State) -> Result<()> {
        store.kv_set(STATE_KEY, &serde_json::to_string(state)?)
    }

    /// Sets (`Some`) or removes (`None`) a daily limit.
    pub fn set_limit(&self, store: &Store, key: &str, limit: Option<u64>) -> Result<()> {
        let mut state = self.0.locked();
        match limit {
            Some(tokens) if tokens > 0 => state.limits.insert(key.to_owned(), tokens),
            _ => state.limits.remove(key),
        };
        // A new limit is a fresh start for what's announced.
        if let Some(tally) = state.tallies.get_mut(key) {
            tally.announced = None;
        }
        Self::save(store, &state)
    }

    /// Counts a finished turn's tokens against the bot and the fleet, and says which budgets just crossed a level.
    pub fn record(&self, store: &Store, bot: &str, now_ms: i64, tokens: u64) -> Vec<(String, Crossed)> {
        if tokens == 0 {
            return Vec::new();
        }
        let day = day_of(now_ms);
        let mut state = self.0.locked();
        let crossed: Vec<_> =
            [bot, FLEET].into_iter().filter_map(|key| Some((key.to_owned(), state.add(key, day, tokens)?))).collect();
        if let Err(error) = Self::save(store, &state) {
            tracing::warn!(error = format!("{error:#}"), "couldn't save the token budget");
        }
        crossed
    }

    /// Whether `bot`'s budget, or the fleet's, has reached its warning level today: the point where a bot with the
    /// saver on starts spending less (see `agent::bot::session`).
    pub fn saving(&self, bot: &str, now_ms: i64) -> bool {
        let day = day_of(now_ms);
        let state = self.0.locked();
        [bot, FLEET].into_iter().any(|key| {
            let Some(limit) = state.limits.get(key) else { return false };
            let used = state.tallies.get(key).filter(|t| t.day == day).map_or(0, |t| t.tokens);
            level(used, *limit) != Level::Within
        })
    }

    /// Today's limits and use, for the clients.
    pub fn status(&self, now_ms: i64) -> Value {
        let day = day_of(now_ms);
        let state = self.0.locked();
        let used = |key: &str| state.tallies.get(key).filter(|t| t.day == day).map_or(0, |t| t.tokens);
        let rows: Vec<Value> = state
            .limits
            .iter()
            .map(|(key, limit)| json!({"botId": key, "limit": limit, "used": used(key), "level": level(used(key), *limit)}))
            .collect();
        json!({"budgets": rows})
    }
}

/// The tokens a turn's `usage` report counts against a budget: what was read fresh, written to the cache and
/// produced. Cache reads are cheap and would swamp the count.
pub fn spent(usage: &Value) -> u64 {
    ["inputTokens", "outputTokens", "cachedWriteTokens"].iter().map(|k| usage[*k].as_u64().unwrap_or(0)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(limit: u64) -> State {
        State { limits: HashMap::from([("a".to_owned(), limit)]), ..State::default() }
    }

    #[test]
    fn the_saver_starts_at_the_warning_level_and_ends_with_the_day() {
        let budgets = Budgets(Mutex::new(state(100)));
        let today = 5 * DAY_MS;
        assert!(!budgets.saving("a", today), "nothing used yet");
        budgets.0.locked().add("a", 5, 79);
        assert!(!budgets.saving("a", today), "just under the warning");
        budgets.0.locked().add("a", 5, 1);
        assert!(budgets.saving("a", today), "80 % is the warning level");
        assert!(!budgets.saving("a", today + DAY_MS), "tomorrow starts clean");
        assert!(!budgets.saving("other", today), "a bot with no limit of its own is not affected");
    }

    #[test]
    fn levels_follow_the_share_of_the_limit_used() {
        assert_eq!(level(79, 100), Level::Within);
        assert_eq!(level(80, 100), Level::Warning);
        assert_eq!(level(100, 100), Level::Exceeded);
    }

    #[test]
    fn each_level_is_announced_once_a_day() {
        let mut s = state(100);
        assert_eq!(s.add("a", 1, 50), None);
        assert_eq!(s.add("a", 1, 35).map(|c| c.level), Some(Level::Warning));
        assert_eq!(s.add("a", 1, 5), None, "the warning was already given");
        assert_eq!(s.add("a", 1, 20).map(|c| c.level), Some(Level::Exceeded));
        assert_eq!(s.add("a", 1, 20), None, "nothing more to say once exceeded");
    }

    #[test]
    fn a_new_day_starts_the_tally_over() {
        let mut s = state(100);
        assert_eq!(s.add("a", 1, 120).map(|c| c.level), Some(Level::Exceeded));
        assert_eq!(s.add("a", 2, 10), None);
        assert_eq!(s.tallies["a"].tokens, 10);
    }

    #[test]
    fn a_bot_without_a_limit_is_only_tallied() {
        let mut s = State::default();
        assert_eq!(s.add("b", 1, 1_000_000), None);
        assert_eq!(s.tallies["b"].tokens, 1_000_000);
    }

    #[test]
    fn spent_leaves_out_cache_reads() {
        let usage = json!({"inputTokens": 10, "outputTokens": 5, "cachedWriteTokens": 3, "cachedReadTokens": 99_000});
        assert_eq!(spent(&usage), 18);
        assert_eq!(spent(&json!({})), 0);
    }

    #[test]
    fn days_roll_over_at_midnight_utc() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(DAY_MS - 1), 0);
        assert_eq!(day_of(DAY_MS), 1);
    }
}
