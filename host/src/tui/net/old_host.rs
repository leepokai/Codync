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

/// `new`'s value also under `old`: each host reads the name it knows and ignores the other.
fn also(map: &mut Map<String, Value>, new: &str, old: &str) {
    if let Some(value) = map.get(new).cloned() {
        map.insert(old.to_owned(), value);
    }
}

/// The calls that name a task by `taskId` (`itemId` before).
const TASK_CALLS: [&str; 8] =
    ["send", "steer", "unqueue", "workFirst", "workCutIn", "dropWork", "stopWork", "retryWork"];

/// A call as either host takes it. What names a task or a bot's thread settings carries both names whatever
/// `version` says, so a version that is missing or left from the last connection never sends a thread's message
/// to the chat. Only `threadEntries` goes by `version` (unknown counts as this client's own): a wrong guess there
/// is a read that fails.
pub fn outgoing(version: &str, method: &'static str, mut body: Value) -> (&'static str, Value) {
    let Some(args) = body.as_object_mut() else { return (method, body) };
    if method == "threadEntries" {
        if crate::compat::at_least(version, RENAMED_IN) {
            return (method, body);
        }
        rename(args, "threadId", "taskId");
        return ("taskEntries", body);
    }
    if TASK_CALLS.contains(&method) {
        also(args, "taskId", "itemId");
    } else if method == "updateBot" || method == "createBot" {
        also(args, "maxThreads", "maxAgents");
        also(args, "threadWorktrees", "taskWorktrees");
    }
    (method, body)
}

/// An event or an answer as this client reads it, whichever host sent it. By shape, not by version: the first
/// events can arrive before `hello` says which host this is, and a host from 2.51.0 sends none of the old shapes.
pub fn incoming(v: &mut Value) {
    let Some(top) = v.as_object_mut() else { return };
    if top.get("type").and_then(Value::as_str) == Some("taskEntry") {
        top.insert("type".into(), "threadEntry".into());
        rename(top, "task", "thread");
    } else if top.get("task").is_some_and(Value::is_object) && top.get("entries").is_some_and(Value::is_array) {
        // `taskEntries`' answer.
        rename(top, "task", "thread");
    }
    for (one, many, read) in [("bot", "bots", bot as fn(&mut Value)), ("entry", "entries", entry)] {
        top.get_mut(one).into_iter().for_each(read);
        top.get_mut(many).and_then(Value::as_array_mut).into_iter().flatten().for_each(read);
    }
}

fn bot(v: &mut Value) {
    let Some(bot) = v.as_object_mut() else { return };
    rename(bot, "maxAgents", "maxThreads");
    rename(bot, "taskWorktrees", "threadWorktrees");
    let work = bot.get_mut("work").and_then(Value::as_array_mut).into_iter().flatten();
    for task in work.filter_map(Value::as_object_mut) {
        rename(task, "task", "thread");
    }
}

/// An entry a thread put in its bot's chat: `data.task {id, title}` was the thread, `data.item` the task.
fn entry(v: &mut Value) {
    if let Some(Value::Object(data)) = v.get_mut("data")
        && data.get("task").is_some_and(Value::is_object)
    {
        rename(data, "task", "thread");
        let task = data.remove("item").unwrap_or(Value::Null);
        data.insert("task".into(), task);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_call_names_a_task_and_a_bots_threads_for_either_host() {
        // Whatever the version, the last connection's included: both names go.
        for version in ["2.50.0", "2.51.0", "2.52.1-dev", ""] {
            for method in TASK_CALLS {
                assert_eq!(
                    outgoing(version, method, json!({"botId": "b", "taskId": "i1"})),
                    (method, json!({"botId": "b", "taskId": "i1", "itemId": "i1"})),
                    "{version}"
                );
            }
            for method in ["updateBot", "createBot"] {
                assert_eq!(
                    outgoing(version, method, json!({"id": "b", "maxThreads": 2, "threadWorktrees": false})),
                    (
                        method,
                        json!({"id": "b", "maxThreads": 2, "threadWorktrees": false,
                            "maxAgents": 2, "taskWorktrees": false})
                    ),
                    "{version}"
                );
            }
            // A message to the chat names no task and gains nothing.
            let chat = json!({"botId": "b", "text": "hi"});
            assert_eq!(outgoing(version, "send", chat.clone()), ("send", chat));
        }
        assert_eq!(
            outgoing("2.50.0", "threadEntries", json!({"botId": "b", "threadId": "task-1"})),
            ("taskEntries", json!({"botId": "b", "taskId": "task-1"}))
        );
        // A host from 2.51.0, or one that hasn't said its version yet, reads a thread as written.
        for version in ["2.51.0", "2.52.1-dev", ""] {
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

        // What the host only passes through stays as it is, at any depth.
        let tool = json!({"type": "entry", "entry": {"id": "t", "kind": "tool",
            "data": {"detail": {"kind": "x", "maxAgents": 1, "data": {"task": {"id": "1"}}}}}});
        assert_eq!(read(tool.clone()), tool);

        // What a host from 2.51.0 sends stays as it is.
        for new in [json!({"type": "bot", "bot": bot}), json!({"type": "entry", "entry": card})] {
            assert_eq!(read(new.clone()), new);
        }
    }
}
