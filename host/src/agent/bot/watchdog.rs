//! Watches a running turn for two kinds of trouble:
//! - a loop: the same tool with the same input and the same result `LOOP_LIMIT` times in a row
//!   is a loop, not progress, and every round costs tokens. The turn is stopped. A call is counted
//!   when it finishes, with the input its updates settled on; one whose input never arrived isn't
//!   counted. Any other call in between resets the count, so retries, and polling whose result
//!   changes, go on.
//! - a tool call that's been running for `TOOL_HUNG_AFTER`: the chat says so once; the turn keeps running, since a
//!   long build is a tool call too.
//! - a stall: nothing from the agent for `STALL_AFTER`, with no tool running and no approval
//!   waiting, usually means a hung connection. The chat says so once; the turn keeps running.
//!
//! Ideas from rakazo's `tool-loop.ts` and `stuck-work.ts` (Apache-2.0); no code copied.

use super::{
    Actor,
    outcome::{Outcome, Trace},
};
use crate::store::EntryKind;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

/// Identical calls in a row that stop the turn.
const LOOP_LIMIT: u32 = 10;
/// Silence that's worth telling the user about.
const STALL_AFTER: Duration = Duration::from_mins(10);
/// A tool call that has run this long may be hung.
const TOOL_HUNG_AFTER: Duration = Duration::from_mins(30);

/// A tool call started and not finished: what it is so far.
struct Call {
    title: String,
    kind: Option<String>,
    input: Value,
    started: Instant,
}

impl Default for Call {
    fn default() -> Self {
        Self { title: String::new(), kind: None, input: Value::Null, started: Instant::now() }
    }
}

pub(super) struct Watchdog {
    last_call: Option<u64>,
    streak: u32,
    /// Why the turn must stop, until the actor acts on it.
    tripped: Option<String>,
    last_update: Instant,
    running: HashMap<String, Call>,
    warned: bool,
    /// A hung tool call was already reported this turn.
    hung_warned: bool,
    /// What the turn's finished tool calls did (`outcome`).
    trace: Trace,
}

impl Default for Watchdog {
    fn default() -> Self {
        Self {
            last_call: None,
            streak: 0,
            tripped: None,
            last_update: Instant::now(),
            running: HashMap::new(),
            warned: false,
            hung_warned: false,
            trace: Trace::default(),
        }
    }
}

/// Whether a tool's input says anything (adapters may send `{}` first and fill it in later).
fn known(input: &Value) -> bool {
    match input {
        Value::Null => false,
        Value::Object(map) => !map.is_empty(),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

impl Watchdog {
    /// A new turn starts with a clean slate.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Notes one `session/update` of the turn.
    pub(super) fn seen(&mut self, update: &Value) {
        self.last_update = Instant::now();
        let kind = update["sessionUpdate"].as_str();
        if !matches!(kind, Some("tool_call" | "tool_call_update")) {
            return;
        }
        let id = update["toolCallId"].as_str().unwrap_or_default().to_owned();
        let call = self.running.entry(id.clone()).or_default();
        if let Some(title) = update["title"].as_str() {
            title.clone_into(&mut call.title);
        }
        if let Some(kind) = update["kind"].as_str() {
            call.kind = Some(kind.to_owned());
        }
        if known(&update["rawInput"]) {
            call.input = update["rawInput"].clone();
        }
        if matches!(update["status"].as_str(), Some("completed" | "failed"))
            && let Some(call) = self.running.remove(&id)
        {
            self.trace.finished(call.kind.as_deref(), update["status"] == "failed");
            self.count(&call, &update["content"]);
        }
    }

    fn count(&mut self, call: &Call, result: &Value) {
        if !known(&call.input) {
            return;
        }
        let mut hasher = DefaultHasher::new();
        (&call.title, &call.kind, call.input.to_string(), result.to_string()).hash(&mut hasher);
        let fingerprint = hasher.finish();
        self.streak = if self.last_call == Some(fingerprint) { self.streak + 1 } else { 1 };
        self.last_call = Some(fingerprint);
        if self.streak == LOOP_LIMIT {
            let title = if call.title.is_empty() { "a tool" } else { call.title.as_str() };
            self.tripped = Some(format!(
                "Stopped: the agent called {title} with the same input and got the same result {LOOP_LIMIT} times in a row. Send a message to continue differently."
            ));
        }
    }

    /// What the turn did so far, by the tools it ran.
    pub(super) fn outcome(&self) -> Outcome {
        self.trace.outcome()
    }

    /// Tool calls the turn finished without failing.
    pub(super) fn tool_calls(&self) -> u32 {
        self.trace.calls()
    }

    /// Why the turn must stop for a loop, once.
    pub(super) fn tripped(&mut self) -> Option<String> {
        self.tripped.take()
    }

    /// A note for the user, once per turn, when a tool call has been running far longer than calls usually do.
    fn hung(&mut self, now: Instant) -> Option<String> {
        if self.hung_warned {
            return None;
        }
        let call = self.running.values().find(|c| now.saturating_duration_since(c.started) >= TOOL_HUNG_AFTER)?;
        self.hung_warned = true;
        let title = if call.title.is_empty() { "A tool" } else { call.title.as_str() };
        Some(format!(
            "{title} has been running for over {} minutes. It may be hung; stop the bot to end it, or keep waiting.",
            TOOL_HUNG_AFTER.as_secs() / 60
        ))
    }

    /// A note for the user, once per turn, when the agent went silent with nothing to wait for.
    fn stalled(&mut self, now: Instant, waiting_on_user: bool) -> Option<String> {
        if self.warned || waiting_on_user || !self.running.is_empty() || now - self.last_update < STALL_AFTER {
            return None;
        }
        self.warned = true;
        Some(format!(
            "No word from the agent for {} minutes. It may be stuck; stop it to send the message again, or keep waiting.",
            STALL_AFTER.as_secs() / 60
        ))
    }
}

impl Actor {
    /// Tells the chat once when the running turn has gone silent (called on the flush tick).
    pub(super) fn check_stall(&mut self) {
        let Some(turn) = self.turn else { return };
        if let Some(note) = self.watchdog.stalled(Instant::now(), !self.perms.is_empty()) {
            tracing::warn!(bot = %self.cfg.id, "a turn has gone silent");
            self.hub.add_entry(&self.lane, EntryKind::Notice, turn, &json!({"text": note, "style": "info"}));
        }
        if let Some(note) = self.watchdog.hung(Instant::now()) {
            tracing::warn!(bot = %self.cfg.id, "a tool call may be hung");
            self.hub.add_entry(&self.lane, EntryKind::Notice, turn, &json!({"text": note, "style": "info"}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(title: &str, path: &str) -> Value {
        json!({"sessionUpdate":"tool_call","toolCallId":uuid::Uuid::new_v4().to_string(),"title":title,"kind":"read","rawInput":{"path":path},"status":"completed"})
    }

    #[test]
    fn only_an_unbroken_streak_of_the_same_call_trips() {
        let mut dog = Watchdog::default();
        for _ in 0..LOOP_LIMIT - 1 {
            dog.seen(&call("Read config.toml", "config.toml"));
        }
        dog.seen(&call("Read main.rs", "main.rs"));
        assert!(dog.tripped().is_none(), "a different call breaks the streak");
        for _ in 0..LOOP_LIMIT {
            dog.seen(&call("Read config.toml", "config.toml"));
        }
        assert!(dog.tripped().unwrap().contains("Read config.toml"));
        assert!(dog.tripped().is_none(), "it's told once");
    }

    #[test]
    fn input_filled_in_later_and_changing_results_count_as_different_calls() {
        let mut dog = Watchdog::default();
        for n in 0..LOOP_LIMIT * 2 {
            let id = format!("call-{n}");
            // The adapter sends an empty input first, then the real one.
            dog.seen(&json!({"sessionUpdate":"tool_call","toolCallId":id,"title":"Terminal","rawInput":{}}));
            dog.seen(&json!({"sessionUpdate":"tool_call_update","toolCallId":id,"rawInput":{"command":format!("step {n}")},"status":"completed"}));
        }
        assert!(dog.tripped().is_none(), "different commands behind the same title");
        for n in 0..LOOP_LIMIT * 2 {
            let id = format!("poll-{n}");
            dog.seen(&json!({"sessionUpdate":"tool_call","toolCallId":id,"title":"Check CI","rawInput":{"run":7}}));
            dog.seen(&json!({"sessionUpdate":"tool_call_update","toolCallId":id,"status":"completed","content":[{"text":format!("{n}% done")}]}));
        }
        assert!(dog.tripped().is_none(), "polling whose result moves on");
        for n in 0..LOOP_LIMIT {
            dog.seen(&json!({"sessionUpdate":"tool_call","toolCallId":format!("blank-{n}"),"title":"Tool","status":"completed"}));
        }
        assert!(dog.tripped().is_none(), "calls whose input never arrived aren't counted");
    }

    #[test]
    fn a_tool_running_far_too_long_is_reported_once() {
        let mut dog = Watchdog::default();
        dog.seen(&json!({"sessionUpdate":"tool_call","toolCallId":"t","title":"npm install","status":"in_progress"}));
        assert!(dog.hung(Instant::now()).is_none());
        let later = Instant::now() + TOOL_HUNG_AFTER + Duration::from_secs(1);
        assert!(dog.hung(later).unwrap().contains("npm install"));
        assert!(dog.hung(later).is_none(), "it's told once a turn");
        let mut done = Watchdog::default();
        done.seen(&json!({"sessionUpdate":"tool_call","toolCallId":"t","title":"ls","status":"completed"}));
        assert!(done.hung(later).is_none(), "a finished call isn't hung");
    }

    #[test]
    fn silence_is_a_stall_only_with_nothing_to_wait_for() {
        let mut dog = Watchdog::default();
        let later = Instant::now() + STALL_AFTER + Duration::from_secs(1);
        dog.seen(
            &json!({"sessionUpdate":"tool_call","toolCallId":"build","title":"cargo build","status":"in_progress"}),
        );
        assert!(dog.stalled(later, false).is_none(), "a running tool explains the silence");
        dog.seen(&json!({"sessionUpdate":"tool_call_update","toolCallId":"build","status":"completed"}));
        let later = Instant::now() + STALL_AFTER + Duration::from_secs(1);
        assert!(dog.stalled(later, true).is_none(), "an approval explains it too");
        assert!(dog.stalled(Instant::now(), false).is_none());
        assert!(dog.stalled(later, false).is_some());
        assert!(dog.stalled(later, false).is_none(), "it's told once a turn");
    }
}
