//! Pure in-memory circuit breaker and cooldown tracking per decision sidecar target.
//! Never persists state to disk; resets cleanly on host restart.

use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Long cooldown on HTTP 402 (payment required / exhausted quota).
pub(crate) const COOLDOWN_PAYMENT_REQUIRED: Duration = Duration::from_mins(30);
/// Short cooldown after repeated transient errors (429 / 5xx / timeout / transport).
pub(crate) const COOLDOWN_TRANSIENT: Duration = Duration::from_mins(3);
/// Consecutive transient failures required to open the short circuit.
pub(crate) const FAILURE_THRESHOLD: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ticket {
    pub(crate) epoch: u64,
    pub(crate) is_probe: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    Admit(Ticket),
    Skip { reason: &'static str },
}

#[derive(Clone, Debug, Default)]
struct TargetState {
    epoch: u64,
    consecutive_failures: u32,
    cooldown_until: Option<Instant>,
    reason: Option<&'static str>,
    probe_in_flight: bool,
}

/// In-memory circuit breaker keyed by [`SidecarTarget::breaker_key`].
pub(crate) struct Breaker {
    states: RwLock<HashMap<String, TargetState>>,
    next_epoch: AtomicU64,
}

impl Default for Breaker {
    fn default() -> Self {
        Self { states: RwLock::new(HashMap::new()), next_epoch: AtomicU64::new(1) }
    }
}

impl Breaker {
    fn alloc_epoch(&self) -> u64 {
        self.next_epoch.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn try_acquire(&self, key: &str) -> Admission {
        self.try_acquire_at(key, Instant::now())
    }

    /// Atomically checks cooldown state and reserves at most one half-open probe after expiry.
    pub(crate) fn try_acquire_at(&self, key: &str, now: Instant) -> Admission {
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = guard.get_mut(key) else {
            return Admission::Admit(Ticket { epoch: 0, is_probe: false });
        };
        match state.cooldown_until {
            Some(until) if now < until => Admission::Skip { reason: state.reason.unwrap_or("cooling_down") },
            Some(_) if state.probe_in_flight => Admission::Skip { reason: "probe_in_flight" },
            Some(_) => {
                state.probe_in_flight = true;
                state.epoch = self.alloc_epoch();
                Admission::Admit(Ticket { epoch: state.epoch, is_probe: true })
            }
            None => Admission::Admit(Ticket { epoch: state.epoch, is_probe: false }),
        }
    }

    /// Releases a reserved probe slot if the caller is cancelled before recording an outcome.
    pub(crate) fn abort_probe(&self, key: &str, ticket: Ticket) {
        if !ticket.is_probe {
            return;
        }
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(state) = guard.get_mut(key)
            && state.epoch == ticket.epoch
        {
            state.probe_in_flight = false;
        }
    }

    pub(crate) fn record_success(&self, key: &str, ticket: Ticket) {
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = guard.get_mut(key) else {
            return;
        };
        // Stale reply from before a cooldown/probe transition cannot undo newer cooldown state.
        if state.epoch != ticket.epoch {
            return;
        }
        state.consecutive_failures = 0;
        state.cooldown_until = None;
        state.reason = None;
        state.probe_in_flight = false;
        if ticket.is_probe {
            state.epoch = self.alloc_epoch();
        }
    }

    pub(crate) fn record_payment_required(&self, key: &str, ticket: Ticket) {
        self.record_payment_required_at(key, Instant::now(), ticket);
    }

    pub(crate) fn record_payment_required_at(&self, key: &str, now: Instant, ticket: Ticket) {
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = guard.entry(key.to_owned()).or_default();
        if state.epoch != ticket.epoch {
            let upgrade_transient =
                !ticket.is_probe && state.cooldown_until.is_some() && state.reason != Some("http_402");
            if !upgrade_transient {
                return;
            }
        }
        state.consecutive_failures = FAILURE_THRESHOLD;
        state.cooldown_until = Some(now + COOLDOWN_PAYMENT_REQUIRED);
        state.reason = Some("http_402");
        state.probe_in_flight = false;
        state.epoch = self.alloc_epoch();
    }

    /// Increments the consecutive transient failure counter and enters cooldown once
    /// [`FAILURE_THRESHOLD`] is reached. Returns `true` when cooldown becomes active.
    pub(crate) fn record_transient_failure(&self, key: &str, ticket: Ticket, reason: &'static str) -> bool {
        self.record_transient_failure_at(key, Instant::now(), ticket, reason)
    }

    pub(crate) fn record_transient_failure_at(
        &self,
        key: &str,
        now: Instant,
        ticket: Ticket,
        reason: &'static str,
    ) -> bool {
        let mut guard = self.states.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = guard.entry(key.to_owned()).or_default();
        if state.epoch != ticket.epoch {
            return false;
        }
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.reason = Some(reason);
        if state.consecutive_failures >= FAILURE_THRESHOLD {
            state.cooldown_until = Some(now + COOLDOWN_TRANSIENT);
            state.probe_in_flight = false;
            state.epoch = self.alloc_epoch();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn payment_required_triggers_thirty_minute_cooldown_and_recovers_on_probe_success() {
        let breaker = Breaker::default();
        let t0 = Instant::now();
        let Admission::Admit(ticket) = breaker.try_acquire_at("t1", t0) else { unreachable!() };

        breaker.record_payment_required_at("t1", t0, ticket);
        assert_eq!(breaker.try_acquire_at("t1", t0 + Duration::from_secs(60)), Admission::Skip { reason: "http_402" });
        assert_eq!(
            breaker.try_acquire_at("t1", (t0 + COOLDOWN_PAYMENT_REQUIRED).checked_sub(Duration::from_secs(1)).unwrap()),
            Admission::Skip { reason: "http_402" }
        );
        // Once cooldown expires, one probe attempt is admitted and concurrent callers wait.
        let Admission::Admit(probe) = breaker.try_acquire_at("t1", t0 + COOLDOWN_PAYMENT_REQUIRED) else {
            unreachable!()
        };
        assert!(probe.is_probe);
        assert_eq!(
            breaker.try_acquire_at("t1", t0 + COOLDOWN_PAYMENT_REQUIRED),
            Admission::Skip { reason: "probe_in_flight" }
        );

        // Success clears failure history so a single future transient fault won't immediately cool down.
        breaker.record_success("t1", probe);
        let Admission::Admit(next) = breaker.try_acquire_at("t1", t0 + COOLDOWN_PAYMENT_REQUIRED) else {
            unreachable!()
        };
        assert!(!next.is_probe);
        assert!(!breaker.record_transient_failure_at("t1", t0 + COOLDOWN_PAYMENT_REQUIRED, next, "http_500"));
    }

    #[test]
    fn three_consecutive_transient_failures_cool_down_and_failed_probe_reopens() {
        let breaker = Breaker::default();
        let t0 = Instant::now();
        for reason in ["http_500", "http_429"] {
            let Admission::Admit(t) = breaker.try_acquire_at("t2", t0) else { unreachable!() };
            assert!(!breaker.record_transient_failure_at("t2", t0, t, reason));
        }
        let Admission::Admit(t3) = breaker.try_acquire_at("t2", t0) else { unreachable!() };
        assert!(breaker.record_transient_failure_at("t2", t0, t3, "timeout"));
        assert_eq!(breaker.try_acquire_at("t2", t0 + Duration::from_secs(30)), Admission::Skip { reason: "timeout" });

        let after_expiry = t0 + COOLDOWN_TRANSIENT;
        let Admission::Admit(probe) = breaker.try_acquire_at("t2", after_expiry) else { unreachable!() };
        assert!(probe.is_probe);
        // Failed probe immediately re-enters cooldown.
        assert!(breaker.record_transient_failure_at("t2", after_expiry, probe, "http_503"));
        assert_eq!(
            breaker.try_acquire_at("t2", after_expiry + Duration::from_secs(1)),
            Admission::Skip { reason: "http_503" }
        );
    }

    #[test]
    fn concurrent_stale_replies_cannot_overwrite_402_or_recovery() {
        let breaker = Breaker::default();
        let t0 = Instant::now();
        let Admission::Admit(slow_req) = breaker.try_acquire_at("t3", t0) else { unreachable!() };
        let Admission::Admit(fast_402) = breaker.try_acquire_at("t3", t0) else { unreachable!() };

        breaker.record_payment_required_at("t3", t0, fast_402);
        // Late 500 or late 200 from slow_req must not downgrade or clear the 30-minute 402 cooldown.
        assert!(!breaker.record_transient_failure_at("t3", t0 + Duration::from_secs(1), slow_req, "http_500"));
        breaker.record_success("t3", slow_req);
        assert_eq!(
            breaker.try_acquire_at("t3", t0 + COOLDOWN_TRANSIENT + Duration::from_secs(1)),
            Admission::Skip { reason: "http_402" }
        );

        // Probe succeeds; any pre-cooldown stale reply still cannot poison the recovered state.
        let t_exp = t0 + COOLDOWN_PAYMENT_REQUIRED;
        let Admission::Admit(probe) = breaker.try_acquire_at("t3", t_exp) else { unreachable!() };
        breaker.record_success("t3", probe);
        assert!(!breaker.record_transient_failure_at("t3", t_exp, slow_req, "http_500"));
        assert!(matches!(breaker.try_acquire_at("t3", t_exp), Admission::Admit(t) if !t.is_probe));
    }

    #[test]
    fn aborted_probe_releases_single_probe_reservation() {
        let breaker = Breaker::default();
        let t0 = Instant::now();
        let Admission::Admit(t) = breaker.try_acquire_at("t4", t0) else { unreachable!() };
        breaker.record_payment_required_at("t4", t0, t);

        let t_exp = t0 + COOLDOWN_PAYMENT_REQUIRED;
        let Admission::Admit(probe) = breaker.try_acquire_at("t4", t_exp) else { unreachable!() };
        assert_eq!(breaker.try_acquire_at("t4", t_exp), Admission::Skip { reason: "probe_in_flight" });

        breaker.abort_probe("t4", probe);
        assert!(matches!(breaker.try_acquire_at("t4", t_exp), Admission::Admit(p) if p.is_probe));
    }
}
