//! `$session_id`: one id for a stretch of activity, so an error's page in `PostHog` shows the
//! events that led up to it. A new one after 30 minutes without events or 24 hours in all,
//! as `PostHog`'s own SDKs do, and on each start of the host.

use std::time::{Duration, SystemTime};
use uuid::Uuid;

const IDLE: Duration = Duration::from_mins(30);
const LONGEST: Duration = Duration::from_hours(24);

#[derive(Default)]
pub struct Session {
    /// The id, when it started and its last event. Wall-clock time, unlike `Instant`, also
    /// counts the hours a computer sleeps.
    current: Option<(Uuid, SystemTime, SystemTime)>,
}

impl Session {
    /// The id for an event at `now`; the first event after the last session ran out starts a new one.
    pub fn id(&mut self, now: SystemTime) -> Uuid {
        let since = |t: SystemTime| now.duration_since(t).unwrap_or_default();
        match &mut self.current {
            Some((id, started, last)) if since(*last) <= IDLE && since(*started) <= LONGEST => {
                *last = now;
                *id
            }
            _ => {
                // PostHog reads a session's start from its UUIDv7.
                let id = Uuid::now_v7();
                self.current = Some((id, now, now));
                id
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_ends_after_half_an_hour_idle_or_a_day_in_all() {
        let mut s = Session::default();
        let t0 = SystemTime::now();
        let first = s.id(t0);
        assert_eq!(first.get_version_num(), 7);
        let minutes = |m: u64| Duration::from_secs(m * 60);
        assert_eq!(s.id(t0 + minutes(29)), first, "still active");
        let start = t0 + minutes(60);
        let second = s.id(start);
        assert_ne!(second, first, "idle for 31 minutes");

        // Busy without a break: a new one once it has run for a day.
        let mut t = start;
        while t <= start + minutes(24 * 60) {
            assert_eq!(s.id(t), second);
            t += minutes(20);
        }
        assert_ne!(s.id(t), second);
    }
}
