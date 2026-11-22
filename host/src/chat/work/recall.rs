//! `recall_task`: the bot looks back at its tasks, ended ones too, up to 30 days: those in its box and its past ones
//! (`tasks::past`).

use super::tasks::{self, Task};
use crate::analytics::{self, Event};
use crate::chat::calls::context::local_time;
use crate::hub::Hub;
use crate::store::{Store, now_ms};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

/// Steps of a thread one page holds.
const STEPS: i64 = 30;
/// How much of one step a page carries.
const STEP: usize = 600;
/// Tasks a list holds at most, and how much of each result.
const LISTED: usize = 20;
const GIST: usize = 280;

/// What `recall_task` finds of `bot`'s tasks: the one `args` names, those with its query, or the latest.
pub(super) fn call(hub: &Hub, bot: &str, args: &Value) -> Result<Value> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let mut all = tasks::load(&hub.store, bot);
    let now = now_ms();
    all.retain(|i| i.ended_at.is_none_or(|at| tasks::within_past(at, now)));
    all.extend(tasks::past(&hub.store, bot, now));
    let (by, recalled) = if let Some(id) = text("task") {
        ("task", one(&hub.store, &all, id, args["before"].as_i64().unwrap_or(i64::MAX)))
    } else {
        let query = text("query");
        (if query.is_some() { "query" } else { "latest" }, Ok(list(&all, query)))
    };
    let outcome = match &recalled {
        Ok(found) if found["tasks"].as_array().is_some_and(Vec::is_empty) => "none",
        Ok(_) => "found",
        Err(_) => "failed",
    };
    let properties = json!({"by": by, "outcome": outcome});
    analytics::capture(hub, Event::WorkRecalled, properties.as_object().cloned().unwrap_or_default());
    recalled
}

/// Task `id` of `all` in full, with the page of its thread's steps before the seq `before`.
fn one(store: &Store, all: &[Task], id: &str, before: i64) -> Result<Value> {
    let task = all.iter().find(|i| i.id == id);
    let task = task.ok_or_else(|| anyhow!("no task {id} in the last 30 days: recall_task with query finds one"))?;
    let followed_by: Vec<&str> = all.iter().filter(|i| i.after.as_deref() == Some(id)).map(|i| i.id.as_str()).collect();
    let (steps, earlier) = match &task.thread {
        Some(thread) => steps(store, thread, before)?,
        None => (Vec::new(), None),
    };
    Ok(json!({
        "taskId": task.id, "title": task.title, "status": task.status, "brief": task.brief,
        "usersWords": task.words, "usersNotes": task.notes, "chatPassedOn": task.relayed, "result": task.result,
        "after": task.after, "followedBy": followed_by, "created": local_time(task.created_at),
        "started": task.started_at.map(local_time), "ended": task.ended_at.map(local_time), "steps": steps,
        "earlierSteps": earlier,
    }))
}

/// The tasks of `all` whose title, brief or result has `query` (any case), all of them without one: what still runs
/// or waits first, then the latest to end, [`LISTED`] at most.
fn list(all: &[Task], query: Option<&str>) -> Value {
    let query = query.map(str::to_lowercase);
    let has = |i: &Task| {
        let fields = [Some(&i.title), Some(&i.brief), i.result.as_ref()];
        query.as_deref().is_none_or(|q| fields.into_iter().flatten().any(|t| t.to_lowercase().contains(q)))
    };
    let mut found: Vec<&Task> = all.iter().filter(|i| has(i)).collect();
    found.sort_by_key(|i| std::cmp::Reverse(i.ended_at.unwrap_or(i64::MAX)));
    let tasks: Vec<Value> = found
        .into_iter()
        .take(LISTED)
        .map(|i| {
            let gist = i.result.as_deref().map(|r| codync_agent::acp::truncate(r, GIST));
            let ended = i.ended_at.map(local_time);
            json!({"taskId": i.id, "title": i.title, "status": i.status, "ended": ended, "result": gist})
        })
        .collect();
    json!({"tasks": tasks})
}

/// A page of `thread`'s steps before the seq `before`, oldest first, and the `before` for the page before it.
fn steps(store: &Store, thread: &str, before: i64) -> Result<(Vec<Value>, Option<i64>)> {
    let page = store.history(thread, before, STEPS)?;
    let earlier = page.first().map(|e| e.seq).filter(|_| page.len() == usize::try_from(STEPS).unwrap_or(0));
    let steps = page
        .iter()
        .filter_map(|e| {
            let text = match e.kind.as_str() {
                "user" | "agent" | "notice" => e.data["text"].as_str()?,
                "tool" | "permission" => e.data["title"].as_str()?,
                _ => return None,
            };
            let text = codync_agent::acp::truncate(text, STEP);
            Some(json!({"kind": e.kind, "text": text, "status": e.data["status"]}))
        })
        .collect();
    Ok((steps, earlier))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_task_finds_a_task_ended_weeks_ago_what_follows_it_and_its_threads_steps() {
        use crate::chat::calls::context::tests::{bot, hub};
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let (now, day) = (now_ms(), 24 * 60 * 60 * 1000);
        let recent = json!([{"id": "i2", "title": "Fix login", "brief": "Go on", "status": "done", "order": 0,
            "createdAt": now - day, "endedAt": now - day, "after": "i1", "thread": "thread-2", "result": "Fixed"},
        {"id": "i3", "title": "Audit", "brief": "Audit the login flow", "status": "done", "order": 1, "createdAt": 0,
            "endedAt": now - 40 * day}]);
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &recent.to_string()).expect("saved");
        let ended = |id: &str, ago: i64| -> Task {
            serde_json::from_value(
                json!({"id": id, "title": "Audit", "brief": "Audit the login flow", "status": "done",
                "order": 0, "createdAt": 0, "endedAt": now - ago * day, "result": "Found 3 bugs", "thread": "gone"}),
            )
            .expect("a task")
        };
        tasks::put_past(&hub.store, "b1", vec![ended("i1", 20), ended("i0", 40)], now).expect("kept");
        let thread = crate::store::Lane::main("thread-2");
        for n in 0..35 {
            let step = json!({"text": format!("step {n}")});
            hub.store.insert_entry(&thread, crate::store::EntryKind::Agent, 1, &step).expect("a step");
        }
        let recall = |args: Value| call(&hub, "b1", &args);
        let ids = |found: Value| -> Vec<String> {
            found["tasks"]
                .as_array()
                .expect("a list")
                .iter()
                .map(|i| i["taskId"].as_str().unwrap().to_owned())
                .collect()
        };

        let audit = recall(json!({"task": "i1"})).expect("20 days back");
        assert_eq!((audit["result"].as_str(), &audit["followedBy"]), (Some("Found 3 bugs"), &json!(["i2"])));
        assert_eq!(audit["steps"], json!([]), "its thread went after a week");
        let login = recall(json!({"task": "i2"})).expect("its thread is still here");
        assert_eq!(login["steps"].as_array().map(Vec::len), Some(30));
        assert_eq!(login["steps"][29]["text"], "step 34", "the latest page first");
        let before = login["earlierSteps"].as_i64().expect("steps before it");
        let first = recall(json!({"task": "i2", "before": before})).expect("the page before");
        assert_eq!((first["steps"].as_array().map(Vec::len), first["earlierSteps"].is_null()), (Some(5), true));
        assert!(recall(json!({"task": "i0"})).is_err(), "past 30 days");
        assert!(recall(json!({"task": "i3"})).is_err(), "past 30 days, though still in the box");

        assert_eq!(ids(recall(json!({"query": "LOGIN flow"})).expect("found")), ["i1"]);
        assert_eq!(ids(recall(json!({})).expect("the latest")), ["i2", "i1"]);
        hub.ceilings.set("b1", codync_network::Access::Ask);
        assert!(hub.ceilings.check_tool("b1", "recall_task").is_ok(), "it only reads: allowed under a ceiling");
    }
}
