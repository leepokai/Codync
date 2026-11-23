//! How a reading of an agent ends, and one reading at a time of those that start the agent.

use crate::LockExt;
use serde::Serialize;
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Mutex;

use super::{App, Window};

/// Where reading an agent's usage stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stage {
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
    /// Its report wasn't one this crate can read: the agent's format changed.
    Parse,
}

/// Agents being read: a second reading meanwhile doesn't start a second copy.
static READING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

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

/// Whether `agent` is being read now, by a reader that starts it: a failure meanwhile is that reading's to tell.
pub fn busy(agent: &str) -> bool {
    READING.locked().contains(agent)
}

/// What a reader brings back.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Read(Vec<Window>),
    /// Skipped on purpose (Copilot between its reads): what it last read stands.
    Skipped,
    /// It couldn't tell, or the agent was being read already ([`busy`]).
    Failed,
}

impl From<Option<Vec<Window>>> for Outcome {
    fn from(windows: Option<Vec<Window>>) -> Self {
        windows.map_or(Self::Failed, Self::Read)
    }
}

/// What `reading` finds, unless `agent` is being read already. `None`: no reading this time.
pub(super) async fn guarded(
    app: &dyn App,
    agent: &str,
    reading: impl Future<Output = Result<Vec<Window>, Stage>>,
) -> Option<Vec<Window>> {
    let _reading = Reading::start(agent)?;
    let outcome = reading.await;
    report(app, agent, &outcome);
    outcome.ok()
}

/// Tells the app how a check ended.
pub(super) fn report<T>(app: &dyn App, agent: &str, reading: &Result<T, Stage>) {
    app.report(agent, reading.as_ref().map(|_| ()).map_err(|stage| *stage));
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
