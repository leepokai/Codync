//! Pure in-memory circuit breaker and cooldown tracking per decision sidecar target.
//! Never persists state to disk; resets cleanly on host restart.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Long cooldown on HTTP 402 (payment required / exhausted quota).
pub(crate) const COOLDOWN_PAYMENT_REQUIRED: Duration = Duration::from_mins(30);
/// Short cooldown after repeated transient errors (429 / 5xx / timeout / transport).
pub(crate) const COOLDOWN_TRANSIENT: Duration = Duration::from_mins(3);
/// Consecutive transient failures required to open the short circuit.
pub(crate) const FAILURE_THRESHOLD: u32 = 3;

#[derive(Clone, Debug, Default)]
struct TargetState {
    consecutive_failures: u32,
    cooldown_until: Option<Instant>,
    reason: Option<&'static str>,
}

/// In-memory circuit breaker keyed by [`SidecarTarget::breaker_key`].
#[derive(Default)]
pub(crate) struct Breaker {
    states: RwLock<HashMap<String, TargetState>>,
}

impl Breaker {
    pub(crate) fn is_cooling_down(&self, key: &str) -> Option<&'static str> {
        self.is_cooling_down_at(key, Instant::now())
    }

    pub(crate) fn is_cooling_down_at(&self, key: &str, now: Instant) -> Option<&'static str> {
        let guard = self.states.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = guard.get(key)?;
        match state.cooldown_until {
            Some(until) if now < until => Some(state.reason.unwrap_or("cooling_down")),
            _ => None,
        }
    }

    pub(crate) fn record_success(&self, key: &str) {
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.remove(key);
    }

    pub(crate) fn record_payment_required(&self, key: &str) {
        self.record_payment_required_at(key, Instant::now());
    }

    pub(crate) fn record_payment_required_at(&self, key: &str, now: Instant) {
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = guard.entry(key.to_owned()).or_default();
        state.consecutive_failures = FAILURE_THRESHOLD;
        state.cooldown_until = Some(now + COOLDOWN_PAYMENT_REQUIRED);
        state.reason = Some("http_402");
    }

    /// Increments the consecutive transient failure counter and enters cooldown once
    /// [`FAILURE_THRESHOLD`] is reached. Returns `true` when cooldown becomes active.
    pub(crate) fn record_transient_failure(&self, key: &str, reason: &'static str) -> bool {
        self.record_transient_failure_at(key, Instant::now(), reason)
    }

    pub(crate) fn record_transient_failure_at(&self, key: &str, now: Instant, reason: &'static str) -> bool {
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = guard.entry(key.to_owned()).or_default();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.reason = Some(reason);
        if state.consecutive_failures >= FAILURE_THRESHOLD {
            state.cooldown_until = Some(now + COOLDOWN_TRANSIENT);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_required_triggers_thirty_minute_cooldown_and_recovers_on_probe_success() {
        let breaker = Breaker::default();
        let t0 = Instant::now();
        assert_eq!(breaker.is_cooling_down_at("t1", t0), None);

        breaker.record_payment_required_at("t1", t0);
        assert_eq!(breaker.is_cooling_down_at("t1", t0 + Duration::from_secs(60)), Some("http_402"));
        assert_eq!(
            breaker.is_cooling_down_at(
                "t1",
                (t0 + COOLDOWN_PAYMENT_REQUIRED).checked_sub(Duration::from_secs(1)).unwrap()
            ),
            Some("http_402")
        );
        // Once cooldown expires, one probe attempt is permitted.
        assert_eq!(breaker.is_cooling_down_at("t1", t0 + COOLDOWN_PAYMENT_REQUIRED), None);

        // Success clears failure history so a single future transient fault won't immediately cool down.
        breaker.record_success("t1");
        assert!(!breaker.record_transient_failure_at("t1", t0 + COOLDOWN_PAYMENT_REQUIRED, "http_500"));
    }

    #[test]
    fn three_consecutive_transient_failures_cool_down_and_failed_probe_reopens() {
        let breaker = Breaker::default();
        let t0 = Instant::now();
        assert!(!breaker.record_transient_failure_at("t2", t0, "http_500"));
        assert_eq!(breaker.is_cooling_down_at("t2", t0), None);
        assert!(!breaker.record_transient_failure_at("t2", t0, "http_429"));
        assert_eq!(breaker.is_cooling_down_at("t2", t0), None);
        assert!(breaker.record_transient_failure_at("t2", t0, "timeout"));
        assert_eq!(breaker.is_cooling_down_at("t2", t0 + Duration::from_secs(30)), Some("timeout"));

        let after_expiry = t0 + COOLDOWN_TRANSIENT;
        assert_eq!(breaker.is_cooling_down_at("t2", after_expiry), None);
        // Failed probe immediately re-enters cooldown.
        assert!(breaker.record_transient_failure_at("t2", after_expiry, "http_503"));
        assert_eq!(breaker.is_cooling_down_at("t2", after_expiry + Duration::from_secs(1)), Some("http_503"));
    }
}
