//! What the worker tools take, as the `codync` server lists them (`assistant::tools`). The cloud connector describes
//! the same tools for hosted assistants (`cloud/src/routes/assistant-tools.ts`).

use serde_json::{Value, json};

fn thread_ref() -> Value {
    json!({
        "botId": {"type": "string", "description": "botId from thread_list or thread_launch."},
        "taskId": {"type": "string", "description": "taskId from thread_list or thread_launch."},
    })
}

fn with(mut properties: Value, more: Value) -> Value {
    if let (Some(p), Value::Object(m)) = (properties.as_object_mut(), more) {
        p.extend(m);
    }
    properties
}

const LIMIT: &str = "Act at most at this level for this call (readOnly, ask, auto or full): it can narrow what the user gave this app, never widen it.";

pub fn tools() -> Vec<Value> {
    let limit =
        json!({"accessLimit": {"type": "string", "enum": ["readOnly", "ask", "auto", "full"], "description": LIMIT}});
    let read_only = json!({"readOnlyHint": true, "openWorldHint": false});
    let writes = json!({"readOnlyHint": false, "idempotentHint": false, "openWorldHint": false});
    vec![
        json!({
            "name": "worker_capabilities",
            "description": "What you may do on this computer: your access level, the bots and folders you may use, whether you may start threads without a bot, the coding agents here (installed, signed in, login profiles), the approval levels a thread can run at and the limits. Pass agent for that agent's model ids (it starts the agent briefly; not over the cloud).",
            "inputSchema": {"type": "object", "properties": {"agent": {"type": "string", "description": "An agent id from this list."}}},
            "annotations": read_only,
        }),
        json!({
            "name": "thread_list",
            "description": "Threads you may see, newest work first: the ones you started, and with the all-threads scope the other threads of the bots you may use. Each has botId, taskId, title, status (waiting, working, needsYou, done, failed, stopped), activity, startedAt, endedAt and a result excerpt. Pass next as cursor for older ended ones.",
            "inputSchema": {"type": "object", "properties": with(json!({
                "botId": {"type": "string", "description": "Only this bot's threads."},
                "status": {"type": "string", "enum": ["waiting", "working", "needsYou", "done", "failed", "stopped"]},
                "limit": {"type": "integer", "minimum": 1, "maximum": 50},
                "cursor": {"type": "string", "description": "next from the previous call."},
            }), limit.clone())},
            "annotations": read_only,
        }),
        json!({
            "name": "thread_launch",
            "description": "Start a thread: a coding agent working on your message, on one of the user's bots (botId) or without a bot on any agent here (agent). It returns at once; follow it with thread_wait and thread_read. approval is how its requests are handled: ask (default: every request is an approval card on the user's devices), auto (the agent's own auto mode; what it asks for is a card), full (requests approved by themselves, in the agent mode you pick). You can't ask for more than worker_capabilities says you may. Approvals are never answered through these tools: the user answers them in Codync. Give clientRequestId so a retry never starts a second thread.",
            "inputSchema": {"type": "object", "properties": with(json!({
                "message": {"type": "string", "description": "The task, relevant context, file paths and the expected result. The thread is named from it."},
                "botId": {"type": "string", "description": "A bot from worker_capabilities. Leave out to run an agent without a bot."},
                "agent": {"type": "string", "description": "Without a bot: the agent id from worker_capabilities. With a bot: another agent than the bot's."},
                "folder": {"type": "string", "description": "Full path of the folder it works in: one of the bot's projects or a folder worker_capabilities lists. Without a bot and without a folder it gets a folder of its own."},
                "place": {"type": "string", "enum": ["here", "worktree"], "description": "here: in the folder itself; worktree: in a git worktree of its own. Left out, the bot's setting decides."},
                "model": {"type": "string", "description": "A model id (worker_capabilities with agent lists them); left out, the agent's default."},
                "modelOptions": {"type": "object", "additionalProperties": {"type": "string"}, "description": "The model's own settings: full access only."},
                "approval": {"type": "string", "enum": ["ask", "auto", "full"]},
                "mode": {"type": "string", "description": "The agent's own mode id, with approval full only."},
                "loginProfile": {"type": "string", "description": "One of the agent's login profiles from worker_capabilities."},
                "clientRequestId": {"type": "string", "description": "Your id for this launch: the same id again returns the same thread (replayed)."},
            }), limit.clone()), "required": ["message"]},
            "annotations": writes,
        }),
        json!({
            "name": "thread_send",
            "description": "Write to a thread you may act on, as a message in it. A running turn makes it wait (mode queue, the default); steer hands it to that turn now. A thread that ended starts again with it.",
            "inputSchema": {"type": "object", "properties": with(with(thread_ref(), json!({
                "message": {"type": "string"},
                "mode": {"type": "string", "enum": ["queue", "steer"]},
                "clientRequestId": {"type": "string", "description": "The same id again sends nothing more."},
            })), limit.clone()), "required": ["botId", "taskId", "message"]},
            "annotations": writes,
        }),
        json!({
            "name": "thread_wait",
            "description": "Wait until a thread changes: with cursor, for anything after that cursor; without, until it ends or needs the user (it answers at once when it already has). Returns status, activity, what waits on the user (needsYou), the result once it ended, and cursor. On timedOut call it again with the cursor.",
            "inputSchema": {"type": "object", "properties": with(thread_ref(), json!({
                "timeoutSecs": {"type": "integer", "minimum": 0, "maximum": 50, "description": "At most this long (over the cloud at most 8)."},
                "cursor": {"type": "string", "description": "cursor from the previous thread_wait."},
            })), "required": ["botId", "taskId"]},
            "annotations": read_only,
        }),
        json!({
            "name": "thread_read",
            "description": "Read a thread: view summary (status, result, its last reply) or steps (its transcript, oldest first, a page at a time; before is next from the previous page).",
            "inputSchema": {"type": "object", "properties": with(thread_ref(), json!({
                "view": {"type": "string", "enum": ["summary", "steps"]},
                "before": {"type": "integer", "description": "next from the previous page of steps."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "maxCharsPerItem": {"type": "integer", "minimum": 100, "maximum": 4000},
            })), "required": ["botId", "taskId"]},
            "annotations": read_only,
        }),
        json!({
            "name": "thread_steer",
            "description": "Steer a thread you may act on: first (to the front of what waits), stop (end it), retry (try a failed or stopped one again) or drop (take one that waits, or ended, away). A routine's run or another bot's request is only ever moved first.",
            "inputSchema": {"type": "object", "properties": with(with(thread_ref(), json!({
                "how": {"type": "string", "enum": ["first", "stop", "retry", "drop"]},
            })), limit.clone()), "required": ["botId", "taskId", "how"]},
            "annotations": writes,
        }),
        json!({
            "name": "thread_interrupt",
            "description": "Stop the turn a thread is running now.",
            "inputSchema": {"type": "object", "properties": with(thread_ref(), limit.clone()), "required": ["botId", "taskId"]},
            "annotations": writes,
        }),
        json!({
            "name": "thread_pending",
            "description": "What waits on the user in a thread: list (or read one by requestId). Approval requests are shown only so you can tell the user: they are answered on the user's devices, never here. A choice the thread asked (kind decision) can be answered with action answer, requestId and option.",
            "inputSchema": {"type": "object", "properties": with(with(thread_ref(), json!({
                "action": {"type": "string", "enum": ["list", "read", "answer"]},
                "requestId": {"type": "string"},
                "option": {"type": "string", "description": "One of the choice's options, word for word."},
            })), limit.clone()), "required": ["botId", "taskId"]},
            "annotations": writes,
        }),
        json!({
            "name": "routine_list",
            "description": "The routines of the bots you may use, with their recent runs.",
            "inputSchema": {"type": "object", "properties": {"botId": {"type": "string"}}},
            "annotations": read_only,
        }),
        json!({
            "name": "routine_run",
            "description": "Run one of a bot's routines now (full access only): it runs with the bot's own setup and approvals.",
            "inputSchema": {"type": "object", "properties": with(json!({
                "botId": {"type": "string"},
                "routineId": {"type": "string"},
                "clientRequestId": {"type": "string"},
            }), limit.clone()), "required": ["botId", "routineId"]},
            "annotations": writes,
        }),
        json!({
            "name": "workflow_list",
            "description": "The workflows on this computer.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": read_only,
        }),
        json!({
            "name": "workflow_run",
            "description": "Start a workflow (full access only): its steps message the bots in it, with their own tools and approvals.",
            "inputSchema": {"type": "object", "properties": with(json!({
                "workflowId": {"type": "string"},
                "input": {"type": "string"},
                "clientRequestId": {"type": "string"},
            }), limit), "required": ["workflowId"]},
            "annotations": writes,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    /// The cloud connector's copy of these tools (`cloud/src/routes/assistant-tools.ts`) names the same ones.
    #[test]
    fn the_cloud_connector_lists_the_same_worker_tools() {
        let cloud = include_str!("../../../../../cloud/src/routes/assistant-tools.ts");
        let named: BTreeSet<&str> = cloud
            .split("name: \"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .filter(|name| *name != "list_computers")
            .collect();
        let here: BTreeSet<String> =
            super::tools().iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect();
        let workers: BTreeSet<String> =
            named.iter().filter(|n| super::super::has(n)).map(|n| (*n).to_owned()).collect();
        assert_eq!(workers, here);
        assert!(named.iter().all(|n| super::super::access::class(n, &serde_json::Value::Null).is_some()), "{named:?}");
    }
}
