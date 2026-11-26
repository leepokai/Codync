//! Interleaving points for the pause-race tests.
//!
//! Production never sets `CODYNC_RACE_GATE_DIR`. [`wait`] then returns at once and does not
//! touch the disk. A test points the variable at a directory and creates `<name>.hold` before
//! the actor reaches that point. The actor writes `<name>.entered` and blocks until the hold
//! file is removed. An unarmed name (no hold file) also returns at once, so a test turns on
//! only the point it needs.
//!
//! The points are named for the ordering they hold still:
//! - `unboxed_request_queued` — a `BotRequest` has passed the intake epoch check and sits in
//!   `self.queue` behind a running turn.
//! - `before_next_in_queue` — a turn just finished and the queued work has not been started.
//!   A pause and a resume can both be saved, with their `Reconfigure`s still unread.
//! - `before_boxed_request_insert` — the boxed-request arm passed the epoch check and has not
//!   called `work::add_request`. Pause cleanup can run in that gap.
//! - `routine_queued` — a `Queued::Routine` sits behind a running turn.
//! - `workflow_step_queued` — a `Queued::WorkflowStep` sits behind a running turn.
//! - `before_refuse_paused` — a pause notification has cancelled the running turn and has not
//!   yet refused the queue. A resume and a fresh `runRoutine` can land first.
//! - `before_fallback` — a turn failed and `fall_back` has not looked at `stop_requested`.
//! - `after_inflight_marked` — `start_turn` has recorded `turn.inflight` and has not returned
//!   to the command loop, so a pause notification stays unread.
//! - `before_startup_pause_read` — a restarted actor has the pause snapshot from `spawn` and has not
//!   yet taken the paused or not-paused branch.
//! - `after_pause_changed` — a pause or resume notice has finished its effects. A test counts one
//!   entry per notice so it can look at the turn only after the whole backlog has been applied.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long an armed gate may hold the actor. A forgotten hold file must not block a pause forever.
const HOLD_LIMIT: Duration = Duration::from_secs(15);

fn dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var_os("CODYNC_RACE_GATE_DIR").map(PathBuf::from)).as_ref()
}

/// Blocks while `<name>.hold` exists in the gate directory. No-op in a release build, when the
/// directory is unset, or when that file is absent. A hold that outlives [`HOLD_LIMIT`] is logged
/// and the actor continues.
pub(crate) async fn wait(name: &str) {
    #[cfg(debug_assertions)]
    wait_armed(name).await;
    #[cfg(not(debug_assertions))]
    let _ = name;
}

#[cfg(debug_assertions)]
async fn wait_armed(name: &str) {
    let Some(dir) = dir() else { return };
    let hold = dir.join(format!("{name}.hold"));
    if !hold.exists() {
        return;
    }
    let entered = dir.join(format!("{name}.entered"));
    let _ = std::fs::write(&entered, "1");
    let deadline = Instant::now() + HOLD_LIMIT;
    while hold.exists() {
        if Instant::now() >= deadline {
            tracing::warn!(gate = name, "a pause race gate was still held; continuing");
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let _ = std::fs::remove_file(&entered);
}
