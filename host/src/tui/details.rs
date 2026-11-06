//! Thinking details: whether the working line unfolds the turn's steps. This terminal's choice, never the host's; off
//! by default, so the chat keeps to its messages.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

const FILE: &str = "tui-thinking-details";

static ENABLED: LazyLock<AtomicBool> =
    LazyLock::new(|| AtomicBool::new(super::choice::read(FILE).is_some_and(|value| value.trim() == "on")));

pub(in crate::tui) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Turns thinking details on or off and keeps the choice for the next run; returns the new state.
pub(in crate::tui) fn toggle() -> bool {
    let on = !ENABLED.fetch_xor(true, Ordering::Relaxed);
    if let Err(error) = super::choice::write(FILE, if on { "on" } else { "off" }) {
        tracing::warn!(%error, "couldn't save the terminal's thinking details choice");
    }
    on
}
