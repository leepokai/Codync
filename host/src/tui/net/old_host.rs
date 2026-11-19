// Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
//
//! The terminal client also talks to a host that isn't its own build (`--url` through an SSH tunnel, a service
//! still running the binary from before an update). Such a host calls a task an `item` and a thread a `task`.
//! This is the one place that knows: the rest of the client speaks the names of 2.51.0.

use serde_json::{Map, Value};

/// The first host that speaks tasks and threads.
const RENAMED_IN: &str = "2.51.0";

fn rename(map: &mut Map<String, Value>, old: &str, new: &str) {
    if let Some(value) = map.remove(old) {
        map.insert(new.to_owned(), value);
    }
}

/// A call as the host of `version` takes it: the old method and argument names for one before 2.51.0. A version
/// not known yet (no `hello` so far) counts as this client's own.
pub fn outgoing(version: &str, method: &'static str, mut body: Value) -> (&'static str, Value) {
    if crate::compat::at_least(version, RENAMED_IN) {
        return (method, body);
    }
    let Some(args) = body.as_object_mut() else { return (method, body) };
    if method == "threadEntries" {
        rename(args, "threadId", "taskId");
        return ("taskEntries", body);
    }
    rename(args, "taskId", "itemId");
    if method == "updateBot" {
        rename(args, "maxThreads", "maxAgents");
        rename(args, "threadWorktrees", "taskWorktrees");
    }
    (method, body)
}

/// An event or an answer as this client reads it, whichever host sent it. By shape, not by version: the first
/// events can arrive before `hello` says which host this is, and a host from 2.51.0 sends none of the old shapes.
pub fn incoming(v: &mut Value) {
    if let Some(top) = v.as_object_mut() {
        if top.get("type").and_then(Value::as_str) == Some("taskEntry") {
            top.insert("type".into(), "threadEntry".into());
            rename(top, "task", "thread");
        } else if top.get("task").is_some_and(Value::is_object) && top.get("entries").is_some_and(Value::is_array) {
            // `taskEntries`' answer.
            rename(top, "task", "thread");
        }
    }
    walk(v);
}

fn walk(v: &mut Value) {
    match v {
        Value::Object(map) => {
            // A bot.
            rename(map, "maxAgents", "maxThreads");
            rename(map, "taskWorktrees", "threadWorktrees");
            if let Some(Value::Array(work)) = map.get_mut("work") {
                for task in work.iter_mut().filter_map(Value::as_object_mut) {
                    rename(task, "task", "thread");
                }
            }
            // An entry a thread put in its bot's chat: `data.task {id, title}` was the thread, `data.item` the task.
            if map.contains_key("kind")
                && let Some(Value::Object(data)) = map.get_mut("data")
                && data.get("task").is_some_and(Value::is_object)
            {
                rename(data, "task", "thread");
                let task = data.remove("item").unwrap_or(Value::Null);
                data.insert("task".into(), task);
            }
            map.values_mut().for_each(walk);
        }
        Value::Array(list) => list.iter_mut().for_each(walk),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_host_before_the_rename_is_called_by_the_old_names() {
        let old = |method, body| outgoing("2.50.0", method, body);
        assert_eq!(
            old("threadEntries", json!({"botId": "b", "threadId": "task-1"})),
            ("taskEntries", json!({"botId": "b", "taskId": "task-1"}))
        );
        for method in ["send", "steer", "unqueue", "workFirst", "workCutIn", "dropWork", "stopWork", "retryWork"] {
            assert_eq!(
                old(method, json!({"botId": "b", "taskId": "i1"})),
                (method, json!({"botId": "b", "itemId": "i1"}))
            );
        }
        assert_eq!(
            old("updateBot", json!({"id": "b", "maxThreads": 2, "threadWorktrees": false})),
            ("updateBot", json!({"id": "b", "maxAgents": 2, "taskWorktrees": false}))
        );
        // A host from 2.51.0, or one that hasn't said its version yet, gets the call as written.
        for version in ["2.51.0", "2.52.1-dev", ""] {
            let body = json!({"botId": "b", "taskId": "i1", "maxThreads": 2});
            assert_eq!(outgoing(version, "stopWork", body.clone()), ("stopWork", body), "{version}");
            let body = json!({"botId": "b", "threadId": "thread-1"});
            assert_eq!(outgoing(version, "threadEntries", body.clone()), ("threadEntries", body), "{version}");
        }
    }

    #[test]
    fn what_a_host_before_the_rename_sends_reads_as_the_new_names() {
        let read = |mut v: Value| {
            incoming(&mut v);
            v
        };
        let old_bot = json!({"id": "b", "maxAgents": 2, "taskWorktrees": true,
            "work": [{"id": "i1", "status": "working", "task": "task-1"}, {"id": "i2", "status": "waiting", "task": null}]});
        let bot = json!({"id": "b", "maxThreads": 2, "threadWorktrees": true,
            "work": [{"id": "i1", "status": "working", "thread": "task-1"}, {"id": "i2", "status": "waiting", "thread": null}]});
        assert_eq!(read(json!({"type": "bot", "bot": old_bot.clone()})), json!({"type": "bot", "bot": bot.clone()}));
        assert_eq!(
            read(json!({"type": "snapshot", "bots": [old_bot]})),
            json!({"type": "snapshot", "bots": [bot.clone()]})
        );

        let old_card = json!({"id": "e", "kind": "permission", "data": {"status": "pending",
            "task": {"id": "task-1", "title": "Fix login"}, "item": "i1"}});
        let card = json!({"id": "e", "kind": "permission", "data": {"status": "pending",
            "thread": {"id": "task-1", "title": "Fix login"}, "task": "i1"}});
        assert_eq!(
            read(json!({"type": "entry", "entry": old_card.clone()})),
            json!({"type": "entry", "entry": card.clone()})
        );
        assert_eq!(read(json!({"entries": [old_card]})), json!({"entries": [card.clone()]}), "a page of history");

        let said = json!({"id": "m", "kind": "agent", "data": {"text": "hi"}});
        assert_eq!(
            read(json!({"type": "taskEntry", "rev": 7, "task": "task-1", "entry": said.clone()})),
            json!({"type": "threadEntry", "rev": 7, "thread": "task-1", "entry": said.clone()})
        );
        assert_eq!(
            read(json!({"task": {"id": "task-1", "title": "Fix login"}, "entries": [said.clone()]})),
            json!({"thread": {"id": "task-1", "title": "Fix login"}, "entries": [said]}),
            "taskEntries' answer"
        );

        // What a host from 2.51.0 sends stays as it is.
        for new in [json!({"type": "bot", "bot": bot}), json!({"type": "entry", "entry": card})] {
            assert_eq!(read(new.clone()), new);
        }
    }
}
