//! The host's own watchdog. A host whose runtime stops answering (every worker blocked, a lock that never comes
//! free) is aborted, so the service that runs it starts a fresh one (launchd's `KeepAlive`, systemd's
//! `Restart=always`, `codync-hostw` on Windows), which resumes the turns that were running. Only under the service:
//! nothing else would start the host again. `CODYNC_WATCHDOG_STALL_SECS` sets the stall, and turns it on outside the
//! service too (the end-to-end test).

use crate::LockExt;
use crate::hub::Hub;
use crate::store::now_ms;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// How often the runtime says it is alive, and how often the watchdog looks at most.
const BEAT: Duration = Duration::from_secs(10);
const LOOK: Duration = Duration::from_secs(30);
/// How long the runtime may go without a beat before the host is restarted.
const STALL: Duration = Duration::from_mins(5);
/// A stall this soon after the watchdog's last restart only logs: a hang that comes back at start would otherwise
/// restart the host, and resume its turns, every few minutes.
const AGAIN: Duration = Duration::from_mins(30);
/// Written, with the time, just before the abort. The next start reports it (`host_restarted_stuck`) and keeps the
/// time as [`LAST`].
const MARK: &str = "watchdog.restart";
const LAST: &str = "watchdog.last";

static BEATEN: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Fine,
    Restart,
    /// Stalled again soon after a restart: logged, left running.
    StuckAgain,
}

/// What a look decides, at `now`, from the last beat and the watchdog's last restart (all in ms).
fn verdict(now: i64, beaten: i64, restarted: Option<i64>, stall: Duration) -> Verdict {
    if now - beaten < ms(stall) {
        Verdict::Fine
    } else if restarted.is_some_and(|at| now - at < ms(AGAIN)) {
        Verdict::StuckAgain
    } else {
        Verdict::Restart
    }
}

/// Whether a nap of `look` from `before` to `now` (wall clock, ms) took the computer's sleep with it: the thread's
/// clock stops while the computer sleeps, the wall clock doesn't.
pub fn slept(before: i64, now: i64, look: Duration) -> bool {
    now - before > 2 * ms(look)
}

fn ms(d: Duration) -> i64 {
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

/// Whether the last process was restarted by the watchdog: its mark becomes the last restart's time.
pub fn restarted(data: &Path) -> bool {
    let Ok(at) = std::fs::read_to_string(data.join(MARK)) else { return false };
    if let Err(error) = std::fs::write(data.join(LAST), at).and_then(|()| std::fs::remove_file(data.join(MARK))) {
        tracing::warn!(error = format!("{error:#}"), "couldn't keep the watchdog's restart");
    }
    true
}

/// Starts the watchdog where something starts the host again after an abort.
pub fn start(hub: &Arc<Hub>, data: PathBuf, service: bool) {
    let stall = std::env::var("CODYNC_WATCHDOG_STALL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|s| *s > 0)
        .map(Duration::from_secs);
    if !service && stall.is_none() {
        return;
    }
    let stall = stall.unwrap_or(STALL);
    BEATEN.store(now_ms(), Ordering::Relaxed);
    tokio::spawn(beat(Arc::downgrade(hub), stall));
    if let Err(error) = std::thread::Builder::new().name("watchdog".into()).spawn(move || watch(&data, stall)) {
        tracing::warn!(error = format!("{error:#}"), "couldn't start the watchdog");
    }
}

/// Beats for as long as the runtime and the hub's emit lock answer.
async fn beat(hub: Weak<Hub>, stall: Duration) {
    let mut beat = tokio::time::interval(BEAT.min(stall / 2));
    loop {
        beat.tick().await;
        let Some(hub) = hub.upgrade() else { return };
        // The lock every event leaves under: one that never comes free stops the beat too.
        let emitted = tokio::task::spawn_blocking(move || drop(hub.emit_lock.locked())).await;
        if emitted.is_ok() && !hangs_for_test() {
            BEATEN.store(now_ms(), Ordering::Relaxed);
        }
    }
}

/// The watchdog's own thread, outside the runtime: looks at the last beat until the host has to restart.
fn watch(data: &Path, stall: Duration) {
    let look = LOOK.min(stall / 2);
    let mut told = false;
    loop {
        let before = now_ms();
        std::thread::sleep(look);
        let now = now_ms();
        // The nap ran long by the wall clock: the computer slept, and the runtime with it. The wait starts over.
        if slept(before, now, look) {
            BEATEN.store(now, Ordering::Relaxed);
            continue;
        }
        let restarted = std::fs::read_to_string(data.join(LAST)).ok().and_then(|at| at.trim().parse().ok());
        let beaten = BEATEN.load(Ordering::Relaxed);
        match verdict(now, beaten, restarted, stall) {
            Verdict::Fine => told = false,
            Verdict::StuckAgain => {
                if !std::mem::replace(&mut told, true) {
                    tracing::error!(stalled_ms = now - beaten, "the host is stuck again soon after a restart");
                }
            }
            Verdict::Restart => restart(data, now, now - beaten),
        }
    }
}

/// Leaves the mark the next start reports, and aborts.
fn restart(data: &Path, now: i64, stalled_ms: i64) -> ! {
    tracing::error!(stalled_ms, "the host stopped answering: restarting it");
    if let Err(error) = std::fs::write(data.join(MARK), now.to_string()) {
        // Restarted all the same: the next start just won't report it, nor hold back a second stall soon after.
        tracing::error!(error = format!("{error:#}"), "couldn't leave the watchdog's mark");
    }
    std::process::abort();
}

/// A debug build told to act hung (`CODYNC_WATCHDOG_TEST_HANG`), for the end-to-end test.
fn hangs_for_test() -> bool {
    cfg!(debug_assertions) && std::env::var_os("CODYNC_WATCHDOG_TEST_HANG").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stalled_host_restarts_once_and_a_quick_second_stall_only_logs() {
        let (stall, minute) = (STALL, 60_000);
        let now = 100 * minute;
        assert_eq!(verdict(now, now - minute, None, stall), Verdict::Fine);
        assert_eq!(verdict(now, now - 5 * minute, None, stall), Verdict::Restart);
        assert_eq!(verdict(now, now - 5 * minute, Some(now - 10 * minute), stall), Verdict::StuckAgain);
        assert_eq!(verdict(now, now - 5 * minute, Some(now - 31 * minute), stall), Verdict::Restart);
    }

    #[test]
    fn a_computer_that_slept_is_not_a_hung_host() {
        assert!(!slept(0, 31_000, LOOK), "a nap a little late");
        assert!(slept(0, 8 * 60 * 60 * 1000, LOOK), "a night asleep");
    }

    #[test]
    fn the_next_start_reports_a_restart_once_and_keeps_its_time() {
        let data = std::env::temp_dir().join(format!("codync-watchdog-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&data).expect("a folder");
        assert!(!restarted(&data));
        std::fs::write(data.join(MARK), "42").expect("a mark");
        assert!(restarted(&data));
        assert!(!restarted(&data), "reported once");
        assert_eq!(std::fs::read_to_string(data.join(LAST)).ok().as_deref(), Some("42"));
        let _ = std::fs::remove_dir_all(data);
    }
}
