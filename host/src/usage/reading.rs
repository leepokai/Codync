//! How a reading of an agent ends, and one reading at a time of those the host has to start.

use crate::LockExt;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Mutex;

use super::Window;

/// Where reading an agent's usage stopped (`usage_read_failed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum UsageReadStage {
    /// The agent didn't start.
    Start,
    /// It wouldn't open or resume a session.
    Session,
    /// The session doesn't offer the command, so it was never sent.
    NoCommand,
    /// The agent refused the request or closed on it.
    Prompt,
    /// It didn't finish in time (a signed-out agent waits here).
    Timeout,
    /// Its report wasn't one the host can read: the agent's format changed.
    Parse,
}

/// Agents being read: a refresh during a poll doesn't start a second copy.
static READING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// Agents whose last reading failed, so a failure is reported once, not every poll.
static FAILING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

struct Reading(String);

impl Reading {
    fn start(agent: &str) -> Option<Self> {
        READING.locked().insert(agent.to_owned()).then(|| Self(agent.to_owned()))
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        READING.locked().remove(&self.0);
    }
}

/// What `reading` finds, unless `agent` is being read already. `None`: no reading this time.
pub(super) async fn guarded(
    hub: &Hub,
    agent: &str,
    reading: impl Future<Output = Result<Vec<Window>, UsageReadStage>>,
) -> Option<Vec<Window>> {
    let _reading = Reading::start(agent)?;
    let outcome = reading.await;
    report(hub, agent, &outcome);
    outcome.ok()
}

/// How a reading ended, for usage analytics: a failure once, not on every poll.
pub(super) fn report<T>(hub: &Hub, agent: &str, reading: &Result<T, UsageReadStage>) {
    let Err(stage) = reading else {
        FAILING.locked().remove(agent);
        return;
    };
    tracing::debug!(agent, ?stage, "couldn't read the agent's usage");
    if FAILING.locked().insert(agent.to_owned()) {
        let properties = json!({"agent": agent, "stage": stage});
        analytics::capture(hub, Event::UsageReadFailed, properties.as_object().cloned().unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_reading_of_the_same_agent_waits_for_the_first() {
        let first = Reading::start("test-agent").expect("nothing is reading it");
        assert!(Reading::start("test-agent").is_none());
        assert!(Reading::start("another-test-agent").is_some(), "other agents read meanwhile");
        drop(first);
        assert!(Reading::start("test-agent").is_some());
    }
}
