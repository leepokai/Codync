//! The `chat` server's task tools (`start_task`, `tell_task`, `task_status`, `stop_task`), and what a task itself
//! may not call.

use super::prompts::told;
use super::{MAX_BRIEF, MAX_HOPS, MAX_TITLE, PER_BOT, PER_COMPUTER, Status, Task, load, records, save, stop};
use crate::LockExt;
use crate::analytics::{self, Event};
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, TASK_PREFIX, now_ms};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;

/// The `chat` server's tools that are tasks'.
pub const TOOLS: [&str; 4] = ["start_task", "tell_task", "task_status", "stop_task"];

/// A task can't do what only makes sense from the chat. `what`: the tool, as the refusal names it.
pub fn refuse_in_task(hub: &Hub, bot: &str, what: &str) -> Result<()> {
    if hub.store.bot(bot)?.is_some_and(|row| row.config.is_task()) {
        bail!("{what} isn't available in a task: say in your report what you need, and you will handle it in the chat");
    }
    Ok(())
}

/// Refuses a task the `chat` server's tools that ask the user, share with them, change the bot or start a task
/// (`prompts::first_prompt` tells it so).
pub fn refuse_chat_only(hub: &Hub, bot: &str, name: &str) -> Result<()> {
    use crate::chat::{artifacts, decision, fallbacks, outbox, profile};
    let chat_only = TOOLS.contains(&name)
        || profile::TOOLS.contains(&name)
        || name == decision::TOOL
        || fallbacks::TOOLS.contains(&name)
        || artifacts::TOOLS.contains(&name)
        || name == outbox::SEND_FILE;
    if chat_only { refuse_in_task(hub, bot, name) } else { Ok(()) }
}

pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "start_task",
            "description": "Hand long work to another session of you that works away from this chat; you stay free to answer the user. It starts when this turn ends. Its report comes back to you later as a message. One task at a time: add to a running one with tell_task.",
            "inputSchema": {"type": "object", "properties": {
                "title": {"type": "string", "description": "A few words the user sees while it works, e.g. \"Fix the login redirect\"."},
                "brief": {"type": "string", "description": "Everything the task needs to do the work: the goal, what you already know, what done looks like."},
                "parallel": {"type": "boolean", "description": "Start it beside a task already running. Only when the user wants two unrelated things at once."},
            }, "required": ["title", "brief"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "tell_task",
            "description": "Pass something on to your task: a correction, more work, an answer it needs. A finished task takes it as more work and reports again.",
            "inputSchema": {"type": "object", "properties": {
                "message": {"type": "string"},
                "task": {"type": "string", "description": "The task's id; not needed when you have one task."},
            }, "required": ["message"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "task_status",
            "description": "What your tasks are doing now: status, current step, how long, their latest messages.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        }),
        json!({
            "name": "stop_task",
            "description": "Stop a task of yours. What it already changed stays.",
            "inputSchema": {"type": "object", "properties": {
                "task": {"type": "string", "description": "The task's id; not needed when you have one task."},
            }},
            "annotations": {"readOnlyHint": false, "idempotentHint": true},
        }),
    ]
}

pub fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "start_task" => start(hub, bot, args),
        "tell_task" => tell(hub, bot, args),
        "task_status" => Ok(status(hub, bot)),
        "stop_task" => {
            let id = target(hub, bot, args["task"].as_str(), |task| task.status.live())?;
            stop(hub, bot, Some(&id))?;
            Ok(json!({"stopped": true}))
        }
        _ => bail!("unknown task tool: {name}"),
    }
}

fn start(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let started = admit(hub, bot, args);
    let outcome = started.as_ref().map_or_else(|(reason, _)| *reason, |_| "started");
    let properties = json!({"outcome": outcome, "parallel": args["parallel"] == true});
    analytics::capture(hub, Event::TaskStarted, properties.as_object().cloned().unwrap_or_default());
    started.map_err(|(_, error)| error)
}

/// Records a task for `bot`, or says why not: the refusal's `task_started` outcome and its words for the bot.
fn admit(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value, (&'static str, anyhow::Error)> {
    let failed = |error: anyhow::Error| ("failed", error);
    let row = hub.store.bot(bot).map_err(failed)?.filter(|r| !r.deleted);
    let row = row.ok_or_else(|| failed(anyhow!("unknown bot")))?;
    if hub.runtime(bot).lane.is_none_or(|lane| lane.chat != bot) || hub.team.answering(bot) {
        let why = anyhow!("start_task works only in your own chat with the user: its report comes back there");
        return Err(("elsewhere", why));
    }
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let (Some(title), Some(brief)) = (text("title"), text("brief")) else {
        return Err(("invalid", anyhow!("title and brief are required")));
    };
    if title.chars().count() > MAX_TITLE || brief.len() > MAX_BRIEF {
        let why = anyhow!("title is at most {MAX_TITLE} characters and brief at most {MAX_BRIEF} bytes");
        return Err(("invalid", why));
    }
    let live: Vec<_> = records(&hub.store, bot).into_iter().filter(|(_, t)| t.status.live()).collect();
    if let Some((_, running)) = live.first()
        && args["parallel"] != true
    {
        let why = anyhow!(
            "a task is already running (\"{}\"): add to it with tell_task. Pass parallel: true only when the user \
             wants two unrelated things at once",
            running.title
        );
        return Err(("live", why));
    }
    let everywhere = hub.store.task_bots().map_err(failed)?;
    let everywhere = everywhere.iter().filter(|r| load(&hub.store, &r.config.id).is_some_and(|t| t.status.live()));
    if live.len() >= PER_BOT || everywhere.count() >= PER_COMPUTER {
        let why = anyhow!("too many tasks are running: wait for one to finish, or add to one with tell_task");
        return Err(("limit", why));
    }
    if crate::usage::near_limit(hub, &row.config) {
        let why = anyhow!("your agent is near its usage limit: do the work here, or tell the user it has to wait");
        return Err(("usage", why));
    }
    let hops = hub.tasks.hops(bot) + 1;
    if hops > MAX_HOPS {
        let why =
            anyhow!("too many tasks were started in a row without the user: tell the user where things stand and wait");
        return Err(("hops", why));
    }
    let cfg = BotConfig {
        id: format!("{TASK_PREFIX}{}", uuid::Uuid::new_v4()),
        task_of: Some(bot.to_owned()),
        created_at: now_ms(),
        // Its alerts are its bot's: the cards it raises in the chat, and the turn its report starts there.
        notify: Some(false),
        lead: false,
        pinned: false,
        position: 0,
        section: None,
        auto_name: false,
        created_by: None,
        offered_to_network: false,
        reachable_by_assistants: false,
        // One screen and one team: those stay with the chat.
        computer: false,
        manage_team: false,
        ..row.config
    };
    let task = Task {
        title: title.to_owned(),
        brief: brief.to_owned(),
        words: users_words(hub, bot),
        status: Status::Starting,
        start: None,
        fork_from: None,
        fork_failed: false,
        hops,
        created_at: cfg.created_at,
        ended_at: None,
        report: None,
    };
    {
        let _g = hub.emit_lock.locked();
        hub.store.save_bot(&cfg).map_err(failed)?;
        save(&hub.store, &cfg.id, &task).map_err(failed)?;
    }
    hub.emit_bot(bot);
    Ok(json!({
        "taskId": cfg.id, "status": "starting",
        "note": "It starts when this turn ends. Its report comes back to you later as a message.",
    }))
}

/// The user's own typed message the bot's running turn answers.
fn users_words(hub: &Hub, bot: &str) -> Option<String> {
    let entry = hub.store.entry(&hub.runtime(bot).message?)?;
    entry.data["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// Picks one of `bot`'s tasks: the one named, or the newest that `wanted` when none is.
fn target(hub: &Hub, bot: &str, named: Option<&str>, wanted: impl Fn(&Task) -> bool) -> Result<String> {
    let all = records(&hub.store, bot);
    if let Some(id) = named.map(str::trim).filter(|id| !id.is_empty()) {
        return all
            .iter()
            .find(|(cfg, _)| cfg.id == id)
            .map(|(cfg, _)| cfg.id.clone())
            .ok_or_else(|| anyhow!("no such task"));
    }
    let found = all.iter().rev().find(|(_, task)| wanted(task)).or_else(|| all.last());
    found.map(|(cfg, _)| cfg.id.clone()).ok_or_else(|| anyhow!("you have no task"))
}

fn tell(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let message = args["message"].as_str().map(str::trim).filter(|m| !m.is_empty());
    let message = message.ok_or_else(|| anyhow!("message is required"))?;
    if message.len() > MAX_BRIEF {
        bail!("message is at most {MAX_BRIEF} bytes");
    }
    let id = target(hub, bot, args["task"].as_str(), |task| task.status.live())?;
    let cfg = hub.store.bot(&id)?.ok_or_else(|| anyhow!("no such task"))?.config;
    let mut task = load(&hub.store, &id).ok_or_else(|| anyhow!("no such task"))?;
    if task.status == Status::Starting {
        // Not launched yet: it reads this with its brief.
        task.brief = format!("{}\n\n{message}", task.brief);
        save(&hub.store, &id, &task)?;
        return Ok(json!({"told": true, "taskId": id}));
    }
    if !task.status.live() {
        // Finished: this is more work, and it reports again.
        task.status = Status::Working;
        task.ended_at = None;
        save(&hub.store, &id, &task)?;
        if !hub.has_actor(&id) {
            hub.spawn_bot(cfg.clone());
        }
    }
    let words = users_words(hub, bot);
    crate::chat::messages::post(hub, &cfg, json!({"text": told(message, words.as_deref()), "from": "host"}))?;
    analytics::capture(hub, Event::TaskTold, serde_json::Map::new());
    hub.emit_bot(bot);
    Ok(json!({"told": true, "taskId": id}))
}

fn status(hub: &Hub, bot: &str) -> Value {
    let tasks: Vec<Value> = records(&hub.store, bot)
        .into_iter()
        .map(|(cfg, task)| {
            let rt = hub.runtime(&cfg.id);
            let messages: Vec<String> = hub
                .store
                .history(&cfg.id, i64::MAX, 60)
                .unwrap_or_default()
                .iter()
                .filter(|e| e.kind == "agent" && e.data["final"] == true)
                .rev()
                .take(3)
                .filter_map(|e| e.data["text"].as_str().map(|t| crate::agent::acp::truncate(t, 600)))
                .collect();
            let until = task.ended_at.unwrap_or_else(now_ms);
            json!({
                "taskId": cfg.id, "title": task.title, "status": task.status,
                "waitingOnApproval": rt.status == BotStatus::NeedsInput,
                "doingNow": rt.activity, "seconds": (until - task.created_at) / 1000,
                "latestMessages": messages.into_iter().rev().collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({"tasks": tasks})
}
