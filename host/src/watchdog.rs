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
            Verdict::Restart => restart(data, now, beaten, stall),
        }
    }
}

/// Parses `(pid, ppid, pgrp)` from a Linux `/proc/<pid>/stat` line, handling spaces or
/// parentheses inside the executable's `comm` field.
#[cfg(target_os = "linux")]
fn parse_proc_stat_ppid_pgrp(stat: &str) -> Option<(i32, i32, i32)> {
    let rparen = stat.rfind(')')?;
    let proc_id: i32 = stat.split_whitespace().next()?.parse().ok()?;
    let rest: Vec<&str> = stat.get(rparen + 1..)?.split_whitespace().collect();
    // Fields after `(comm)`: 0=state, 1=ppid, 2=pgrp.
    let parent_id: i32 = rest.get(1)?.parse().ok()?;
    let group_id: i32 = rest.get(2)?.parse().ok()?;
    Some((proc_id, parent_id, group_id))
}

/// Picks PIDs to notify before `std::process::abort()`: direct children of `self_pid`, plus
/// process-group peers when this process leads its own group (`own_group == self_pid`), never
/// `self_pid` itself or an inherited parent/test-runner process group.
#[cfg(target_os = "linux")]
fn select_abort_signal_targets(entries: &[(i32, i32, i32)], self_pid: i32, own_group: i32) -> Vec<i32> {
    let leads_group = own_group == self_pid;
    entries
        .iter()
        .copied()
        .filter(|&(proc_id, parent_id, group_id)| {
            proc_id > 0 && proc_id != self_pid && (parent_id == self_pid || (leads_group && group_id == own_group))
        })
        .map(|(proc_id, _, _)| proc_id)
        .collect()
}

/// Best-effort `SIGTERM` notification to child processes (and owned process group) before aborting.
#[cfg(target_os = "linux")]
fn notify_before_abort() -> usize {
    // SAFETY: `getpid` and `getpgrp` take no pointers and cannot fail.
    let (self_pid, own_group) = unsafe { (libc::getpid(), libc::getpgrp()) };
    let mut entries = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for entry in dir.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().bytes().all(|b| b.is_ascii_digit())
                && let Ok(stat) = std::fs::read_to_string(entry.path().join("stat"))
                && let Some(parsed) = parse_proc_stat_ppid_pgrp(&stat)
            {
                entries.push(parsed);
            }
        }
    }
    let targets = select_abort_signal_targets(&entries, self_pid, own_group);
    let mut signalled = 0;
    for pid in targets {
        // SAFETY: Sends `SIGTERM` only to child or owned-group PIDs excluding `self_pid`.
        if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
            signalled += 1;
        }
    }
    if signalled > 0 {
        std::thread::sleep(Duration::from_millis(50));
    }
    signalled
}

#[cfg(all(unix, not(target_os = "linux")))]
fn notify_before_abort() -> usize {
    // SAFETY: `getpid` and `getpgrp` take no pointers and cannot fail.
    let (self_pid, own_group) = unsafe { (libc::getpid(), libc::getpgrp()) };
    if own_group == self_pid {
        // SAFETY: Ignore `SIGTERM` in the host itself right before signalling our owned process
        // group so `std::process::abort()` still fires `SIGABRT` deterministically.
        let rc = unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
            libc::killpg(own_group, libc::SIGTERM)
        };
        if rc == 0 {
            std::thread::sleep(Duration::from_millis(50));
            return 1;
        }
    }
    0
}

#[cfg(not(unix))]
fn notify_before_abort() -> usize {
    0
}

/// Leaves the mark the next start reports, notifies child processes, logs diagnostics, and aborts.
fn restart(data: &Path, now: i64, beaten: i64, stall: Duration) -> ! {
    let mark_path = data.join(MARK);
    let mark_written = match std::fs::write(&mark_path, now.to_string()) {
        Ok(()) => true,
        Err(error) => {
            // Restarted all the same: the next start just won't report it, nor hold back a second stall soon after.
            tracing::error!(
                error = format!("{error:#}"),
                mark = %mark_path.display(),
                "couldn't leave the watchdog's mark"
            );
            false
        }
    };
    let notified_children = notify_before_abort();
    tracing::error!(
        pid = std::process::id(),
        now_ms = now,
        beaten_ms = beaten,
        stalled_ms = now - beaten,
        stall_ms = ms(stall),
        mark_written,
        mark = %mark_path.display(),
        notified_children,
        "the host stopped answering: restarting it"
    );
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

    #[test]
    fn watchdog_verdict_boundary_transitions_match_thresholds() {
        let stall = STALL;
        let now = 100 * 60_000;
        assert_eq!(verdict(now, now - ms(stall) + 1, None, stall), Verdict::Fine);
        assert_eq!(verdict(now, now - ms(stall), None, stall), Verdict::Restart);
        assert_eq!(verdict(now, now - ms(stall), Some(now - ms(AGAIN) + 1), stall), Verdict::StuckAgain);
        assert_eq!(verdict(now, now - ms(stall), Some(now - ms(AGAIN)), stall), Verdict::Restart);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proc_stat_parser_and_abort_target_selection_protect_parent_groups() {
        let parsed = parse_proc_stat_ppid_pgrp("42 (codync (screen) 1) S 10 10 10 0 -1 4194304").unwrap();
        assert_eq!(parsed, (42, 10, 10));

        let entries = vec![
            (1, 0, 1),
            (99, 1, 99),     // test runner / parent process
            (100, 99, 99),   // self when inheriting runner pgid
            (101, 100, 99),  // direct child in inherited pgid
            (102, 100, 102), // direct child in its own pgid (e.g. cua-driver)
            (103, 99, 99),   // sibling in inherited pgid: MUST NOT be signalled
        ];
        assert_eq!(select_abort_signal_targets(&entries, 100, 99), vec![101, 102]);

        let owned = vec![
            (200, 1, 200),   // self as group leader (systemd service)
            (201, 200, 200), // direct child
            (202, 201, 200), // grandchild in owned group
            (203, 1, 300),   // unrelated process
        ];
        assert_eq!(select_abort_signal_targets(&owned, 200, 200), vec![201, 202]);
    }
}
