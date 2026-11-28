//! The mark of a running turn in the bot's own chat (`turn.inflight.<bot>`), which a restarted host resumes.

use crate::store::{Store, now_ms};

/// What the mark holds: when the turn started and, for a follow-up's turn, its place in its
/// chain, so a restart resumes it as that follow-up and what it sets next still counts toward the limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Inflight {
    pub(super) started: i64,
    /// A follow-up's turn: how many follow-ups led to it.
    pub(super) follow_up: Option<u32>,
}

impl Inflight {
    const FOLLOW_UP: &str = "follow_up";

    /// The turn of `bot` that is marked as running, if one is.
    pub(super) fn load(store: &Store, bot: &str) -> Option<Self> {
        Self::parse(&store.kv_get(&key(bot))?)
    }

    /// The kv row that marks `turn` as the one `bot` is running; `None` when it is over.
    pub(super) fn row(bot: &str, turn: Option<Self>) -> (String, String) {
        (key(bot), turn.map(Self::mark).unwrap_or_default())
    }

    fn parse(mark: &str) -> Option<Self> {
        let mut parts = mark.split(' ');
        let started = parts.next()?.parse().ok()?;
        let follow_up = match parts.next() {
            None => None,
            Some(Self::FOLLOW_UP) => Some(parts.next()?.parse().ok()?),
            Some(_) => return None,
        };
        Some(Self { started, follow_up })
    }

    fn mark(self) -> String {
        match self.follow_up {
            Some(chain) => format!("{} {} {chain}", self.started, Self::FOLLOW_UP),
            None => self.started.to_string(),
        }
    }
}

/// Whether `bot`'s actor resumes a turn when it starts: the previous host process never finished one (crash, update,
/// restart), and it isn't stale: an hour-old turn is more likely unwanted than awaited (gawkbot's rule).
pub(crate) fn resumable(store: &Store, bot: &str) -> bool {
    Inflight::load(store, bot).is_some_and(|turn| now_ms() - turn.started < STALE_RESUME_MS)
}

/// Drops the mark, so a later start of `bot` does not resume the turn the last process cut off.
pub(crate) fn clear(store: &Store, bot: &str) {
    let (key, mark) = Inflight::row(bot, None);
    if let Err(error) = store.kv_set(&key, &mark) {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't drop a cut-off turn's resume mark");
    }
}

/// A cut-off turn older than this isn't resumed.
const STALE_RESUME_MS: i64 = 60 * 60 * 1000;

fn key(bot: &str) -> String {
    format!("turn.inflight.{bot}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_follow_ups_turn_keeps_its_place_in_the_chain_across_a_restart() {
        let turn = Inflight { started: 1_794_650_400_000, follow_up: Some(7) };
        assert_eq!(turn.mark(), "1794650400000 follow_up 7");
        assert_eq!(Inflight::parse(&turn.mark()), Some(turn));
        let plain = Inflight { started: 42, follow_up: None };
        assert_eq!(Inflight::parse(&plain.mark()), Some(plain));
    }

    #[test]
    fn what_is_no_mark_resumes_nothing() {
        assert_eq!(Inflight::parse(""), None, "no turn is running");
        assert_eq!(Inflight::parse("42 something"), None);
        assert_eq!(Inflight::parse("42 follow_up"), None, "a follow-up's mark says its place in the chain");
        assert_eq!(Inflight::parse("42 follow_up soon"), None);
    }
}
