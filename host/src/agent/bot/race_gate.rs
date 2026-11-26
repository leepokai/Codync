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
//! - `before_refuse_paused` — a pause notification has cancelled the running turn and has not
//!   yet refused the queue. A resume and a fresh `runRoutine` can land first.
//! - `before_fallback` — a turn failed and `fall_back` has not looked at `stop_requested`.
//! - `after_inflight_marked` — `start_turn` has recorded `turn.inflight` and has not returned
//!   to the command loop, so a pause notification stays unread.
//! - `before_startup_pause_read` — a restarted actor has not yet read `paused()`.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

fn dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var_os("CODYNC_RACE_GATE_DIR").map(PathBuf::from)).as_ref()
}

/// Blocks while `<name>.hold` exists in the gate directory. No-op when the directory is unset
/// or that file is absent.
pub(crate) async fn wait(name: &str) {
    let Some(dir) = dir() else { return };
    let hold = dir.join(format!("{name}.hold"));
    if !hold.exists() {
        return;
    }
    let entered = dir.join(format!("{name}.entered"));
    let _ = std::fs::write(&entered, "1");
    while hold.exists() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let _ = std::fs::remove_file(&entered);
}
