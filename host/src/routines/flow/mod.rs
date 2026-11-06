//! Routine flows: a routine's reply handed on from bot to bot.
//!
//! The routine's own bot runs as it always has. When its run ends, its reply becomes the flow's
//! first step; each link of the routine then puts that reply to the next bot, with the link's
//! instruction, the way a bot message does (`chat::team::flow`). A bot with several inputs waits
//! for all of them and gets them in one request. Every bot taking part in a run leaves a step
//! (`store::flow_steps`). Rules: no cycles (checked when connecting and again when running), a
//! depth limit, the routine's run timeout for each step, a failure skipping what hangs below it,
//! and a cancelled flow cancelling its running steps. Steps a restart interrupted fail at startup.
//!
//! A link applies only to runs that began after it was made, so connecting a bot never sends it
//! the reply of an earlier run.

mod api;
mod canvas;
mod engine;
pub mod graph;
mod recover;
mod registry;
mod tools;

pub use api::{call as api_call, forget_bot, forget_routine};
pub use canvas::catch_up;
pub use engine::{spawn, start_for};
pub use recover::recover;
pub use registry::Registry;
pub use tools::{TOOLS, call as tool_call, tools as tool_list};

#[cfg(test)]
mod tests;

/// The longest instruction a link may carry.
pub const MAX_INSTRUCTION_BYTES: usize = 8_000;
/// The environment variable that sets how many hops deep a flow may reach (1 to 16).
pub const MAX_DEPTH_ENV: &str = "CODYNC_ROUTINE_FLOW_MAX_DEPTH";

/// How many hops from the routine's own bot a flow may reach.
pub fn max_depth() -> usize {
    std::env::var(MAX_DEPTH_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map_or(graph::DEFAULT_MAX_DEPTH, |depth| depth.clamp(1, graph::MAX_DEPTH_LIMIT))
}
