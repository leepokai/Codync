//! Keeps agents from talking too much. Every message can wake an agent and cost a turn, so the
//! receiving computer enforces budgets, whatever the sender's agents were told:
//!
//! - a chain of handoffs (A asks B, B's answer makes A ask C, …) ends after [`MAX_HOPS`];
//! - each pair of agents gets a small burst, then one message every [`REFILL`];
//! - a pair repeating itself word for word within [`REPEAT_WINDOW`] is refused.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

pub const MAX_HOPS: u8 = 8;
const BURST: f64 = 5.0;
pub const REFILL: Duration = Duration::from_mins(2);
pub const REPEAT_WINDOW: Duration = Duration::from_mins(10);

/// One budget per (sending computer, sending agent, receiving agent).
#[derive(Default)]
pub struct Guard {
    pairs: HashMap<String, Pair>,
}

struct Pair {
    tokens: f64,
    at: Instant,
    /// What it said last, and when.
    last: Option<(u64, Instant)>,
}

/// What a message says, for spotting repeats.
pub fn digest(parts: &[&str]) -> u64 {
    let mut hasher = DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

/// A chain of handoffs ends after [`MAX_HOPS`], on the sending computer and the receiving one.
pub fn check_hops(hops: u8) -> Result<(), String> {
    if hops > MAX_HOPS {
        return Err(format!("this chain already had {MAX_HOPS} handoffs; ask your user before going on"));
    }
    Ok(())
}

impl Guard {
    pub fn admit(&mut self, pair: String, hops: u8, said: u64, now: Instant) -> Result<(), String> {
        check_hops(hops)?;
        // A pair quiet for this long says nothing a new one wouldn't.
        let forget = REFILL.mul_f64(BURST).max(REPEAT_WINDOW);
        self.pairs.retain(|_, p| now.duration_since(p.at) < forget);
        let p = self.pairs.entry(pair).or_insert(Pair { tokens: BURST, at: now, last: None });
        if let Some((last, at)) = p.last
            && last == said
            && now.duration_since(at) < REPEAT_WINDOW
        {
            let ago = now.duration_since(at).as_secs();
            return Err(format!("the same message arrived {ago}s ago; it isn't delivered twice"));
        }
        let refilled = now.duration_since(p.at).as_secs_f64() / REFILL.as_secs_f64();
        p.tokens = (p.tokens + refilled).min(BURST);
        p.at = now;
        if p.tokens < 1.0 {
            let wait = REFILL.mul_f64(1.0 - p.tokens).as_secs();
            return Err(format!("too many messages between these two agents; slow down (next one in {wait}s)"));
        }
        p.tokens -= 1.0;
        p.last = Some((said, now));
        Ok(())
    }

    /// The last message couldn't be delivered: saying it again isn't a repeat.
    pub fn forget(&mut self, pair: &str) {
        if let Some(p) = self.pairs.get_mut(pair) {
            p.last = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_then_one_per_refill() {
        let mut guard = Guard::default();
        let t = Instant::now();
        for n in 0..5 {
            guard.admit("a>b".into(), 0, n, t).unwrap();
        }
        assert!(guard.admit("a>b".into(), 0, 9, t).unwrap_err().contains("slow down"));
        // Another pair has its own budget.
        guard.admit("a>c".into(), 0, 1, t).unwrap();
        // One refill later, exactly one more.
        guard.admit("a>b".into(), 0, 10, t + REFILL).unwrap();
        assert!(guard.admit("a>b".into(), 0, 11, t + REFILL).is_err());
    }

    #[test]
    fn a_repeat_is_refused_until_the_window_passes() {
        let mut guard = Guard::default();
        let t = Instant::now();
        let said = digest(&["review src/auth.rs"]);
        guard.admit("a>b".into(), 0, said, t).unwrap();
        assert!(guard.admit("a>b".into(), 0, said, t).unwrap_err().contains("isn't delivered twice"));
        // Something else is fine, and so is the same thing to another agent.
        guard.admit("a>c".into(), 0, said, t).unwrap();
        guard.admit("a>b".into(), 0, digest(&["and src/db.rs"]), t).unwrap();
        guard.admit("a>b".into(), 0, said, t + REPEAT_WINDOW).unwrap();
    }

    #[test]
    fn long_chains_stop() {
        let mut guard = Guard::default();
        let t = Instant::now();
        guard.admit("a>b".into(), MAX_HOPS, 1, t).unwrap();
        assert!(guard.admit("a>b".into(), MAX_HOPS + 1, 2, t).unwrap_err().contains("handoffs"));
    }

    #[test]
    fn quiet_pairs_are_forgotten() {
        let mut guard = Guard::default();
        let t = Instant::now();
        guard.admit("a>b".into(), 0, 1, t).unwrap();
        guard.admit("c>d".into(), 0, 1, t + REPEAT_WINDOW).unwrap();
        assert_eq!(guard.pairs.len(), 1);
    }
}
