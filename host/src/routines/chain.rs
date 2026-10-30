//! One routine running after another (`docs/plans/routine-chaining.md`): when a run ends for good, the
//! routines whose `routine` event trigger names it are queued, with its outcome as their trigger data.
//! The events never leave the host, so these triggers need no webhook.

use super::{State, Status, Trigger, busy, json, new_run};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, HashSet};

/// The event source a finished run reports as.
pub const SOURCE: &str = "routine";
/// How many routines a chain may run one after another, so a loop stops on its own.
const MAX_DEPTH: u64 = 5;
const MAX_TEXT: usize = 16 * 1024;
const FILTERS: [&str; 3] = ["routineId", "messageContains", "passed"];

/// Checks a `routine` event trigger on its own: what it listens for and that it names its upstream.
pub fn validate(event: &str, filters: &BTreeMap<String, String>) -> Result<()> {
    if !["succeeded", "failed", "*"].contains(&event) {
        bail!("a routine event is succeeded, failed or *");
    }
    if let Some(key) = filters.keys().find(|k| !FILTERS.contains(&k.as_str())) {
        bail!("unsupported routine event filter: {key}");
    }
    if filters.get("routineId").is_none_or(|id| id.trim().is_empty()) {
        bail!("a routine event trigger requires filters.routineId");
    }
    if filters.get("passed").is_some_and(|p| p != "true" && p != "false") {
        bail!("filters.passed is \"true\" or \"false\"");
    }
    Ok(())
}

/// The routine a trigger runs after, when it is a `routine` event trigger.
pub fn upstream(trigger: &Trigger) -> Option<&str> {
    match trigger {
        Trigger::Event { source, filters, .. } if source == SOURCE => filters.get("routineId").map(String::as_str),
        _ => None,
    }
}

/// Checks the routines `triggers` run after against the saved ones: each exists, and none is routine `id`
/// itself or runs after it, directly or further up. A new routine (`id` is `None`) can't close a loop yet.
pub(super) fn check_links(s: &State, id: Option<&str>, triggers: &[Trigger]) -> Result<()> {
    for up in triggers.iter().filter_map(upstream) {
        if !s.routines.iter().any(|r| r.id == up && !r.deleted) {
            bail!("routine {up} to run after was not found");
        }
        let Some(id) = id else { continue };
        let mut seen = HashSet::new();
        let mut todo = vec![up.to_owned()];
        while let Some(next) = todo.pop() {
            if next == id {
                bail!("routines can't run after each other in a loop");
            }
            if !seen.insert(next.clone()) {
                continue;
            }
            let found = s.routines.iter().find(|r| r.id == next && !r.deleted);
            todo.extend(found.into_iter().flat_map(|r| r.triggers.iter().filter_map(upstream).map(str::to_owned)));
        }
    }
    Ok(())
}

/// Queues the routines that run after run `i`, which has just finished with `reply` (none or `(pass)`:
/// nothing to report) and still holds the event it ran with. A cancelled run, or a failed one that will be
/// tried again on its own, triggers nothing: only a run's final outcome counts.
pub(super) fn emit(s: &mut State, i: usize, reply: Option<&str>) {
    let run = &s.runs[i];
    let event = match run.status {
        Status::Succeeded => "succeeded",
        Status::Failed | Status::Interrupted if run.retry_at.is_none() => "failed",
        _ => return,
    };
    let Some(upstream) = s.routines.iter().find(|r| r.id == run.routine_id) else { return };
    let depth = if run.event["source"] == SOURCE { run.event["depth"].as_u64().unwrap_or(0) + 1 } else { 1 };
    let passed = reply.is_none_or(crate::chat::group::is_pass);
    let text = if passed { "" } else { reply.unwrap_or_default() };
    let payload = json!({
        "source": SOURCE,
        "event": event,
        "routineId": upstream.id,
        "routineName": upstream.name,
        "botId": upstream.bot_id,
        "runId": run.id,
        "passed": passed.to_string(),
        "text": &text[..text.floor_char_boundary(MAX_TEXT)],
        "finishedAt": run.finished_at,
        "depth": depth,
    });
    let (from, name) = (upstream.id.clone(), upstream.name.clone());
    let next: Vec<usize> = (0..s.routines.len())
        .filter(|&j| {
            let r = &s.routines[j];
            !r.deleted && r.enabled && r.id != from && r.triggers.iter().any(|t| t.matches(&payload))
        })
        .collect();
    for j in next {
        // `depth` counts the routines before the one it would queue, so that one is number `depth + 1`.
        let skipped = if depth >= MAX_DEPTH {
            Some(format!("Did not run after \"{name}\": a chain runs at most {MAX_DEPTH} routines in a row"))
        } else if busy(s, &s.routines[j].id) {
            Some(format!("Did not run after \"{name}\": it was still running or queued"))
        } else {
            None
        };
        if let Some(why) = skipped {
            s.routines[j].last_error = Some(why);
        } else {
            let queued = new_run(&s.routines[j], payload.clone(), None);
            s.runs.push(queued);
        }
    }
}

/// The name of the routine a trigger runs after, as the apps show it.
pub(super) fn upstream_name(s: &State, trigger: &Trigger) -> Option<String> {
    let id = upstream(trigger)?;
    let found = s.routines.iter().find(|r| r.id == id && !r.deleted);
    Some(found.map_or_else(|| "a deleted routine".into(), |r| format!("\"{}\"", r.name)))
}

/// How a `routine` event trigger reads, `upstream` naming the routine it runs after.
pub fn describe(upstream: &str, event: &str, filters: &BTreeMap<String, String>) -> String {
    let when = match event {
        "succeeded" => "succeeds",
        "failed" => "fails",
        _ => "finishes",
    };
    let mut text = format!("After {upstream} {when}");
    match filters.get("passed").map(String::as_str) {
        Some("false") => text.push_str(" with something to report"),
        Some("true") => text.push_str(" with nothing to report"),
        _ => {}
    }
    if let Some(words) = filters.get("messageContains") {
        text.push_str(" · result contains \"");
        text.push_str(words);
        text.push('"');
    }
    text
}
