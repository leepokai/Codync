//! The `chat` server's work tools (`add_task`, `steer_task`, `work_status`), and what a thread itself may not call.

use super::steer::{self, Steer};
use super::tasks::{self, Shown, Status, Task};
use super::{MAX_BRIEF, MAX_HOPS, MAX_TITLE, MAX_WAITING, dispatch, sources};
use crate::LockExt;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// The `chat` server's tools that are the box's.
pub const TOOLS: [&str; 3] = ["add_task", "steer_task", "work_status"];

/// A thread can't do what only makes sense from the chat. `what`: the tool, as the refusal names it.
pub fn refuse_in_thread(hub: &Hub, bot: &str, what: &str) -> Result<()> {
    if hub.store.bot(bot)?.is_some_and(|row| row.config.is_thread()) {
        bail!(
            "{what} isn't available in a thread: say in your report what you need, and you will handle it in the chat"
        );
    }
    Ok(())
}

/// The bot a call to the servers that are the bot's own (`routines`, `team`) acts as. A thread on a routine run or on
/// another bot's request is its bot there, as that turn was when it ran in the chat (a run saves its progress and
/// sets follow-ups; an asked bot runs its own routine); a thread on the bot's own work is refused.
pub fn acting_as(hub: &Hub, bot: &str, what: &str) -> Result<String> {
    let Some(cfg) = hub.store.bot(bot)?.map(|row| row.config).filter(crate::store::BotConfig::is_thread) else {
        return Ok(bot.to_owned());
    };
    if sources::source_of(&hub.store, &cfg).is_bot() {
        refuse_in_thread(hub, bot, what)?;
    }
    Ok(cfg.owner().to_owned())
}

/// Refuses a thread the `chat` server's tools that change the bot or its box (`prompts::task_prompt` tells it so).
/// What asks the user or shares with them goes to the thread's actor, which posts it in the bot's chat.
pub fn refuse_chat_only(hub: &Hub, bot: &str, name: &str) -> Result<()> {
    use crate::chat::{fallbacks, profile};
    let chat_only = TOOLS.contains(&name) || profile::TOOLS.contains(&name) || fallbacks::TOOLS.contains(&name);
    if chat_only { refuse_in_thread(hub, bot, name) } else { Ok(()) }
}

pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "add_task",
            "description": "Put long work in your event box as a task: one of your threads (you again, in a session of its own) does it away from this chat, and you stay free to answer the user. It starts when this turn ends and a thread is free; until then it waits its turn. Its report comes back to you later as a message. One task per piece of work.",
            "inputSchema": {"type": "object", "properties": {
                "title": {"type": "string", "description": "A few words the user sees while it works, e.g. \"Fix the login redirect\"."},
                "brief": {"type": "string", "description": "Everything needed to do the work: the goal, what you already know, what done looks like."},
                "after": {"type": "string", "description": "The id of a task that is working or waiting: this one is done after it, in the same thread, knowing what that one did. For more of the same work."},
            }, "required": ["title", "brief"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "steer_task",
            "description": "Change a task in your event box. first: a waiting task moves to the front. now: a waiting task starts now; with every thread busy a working task gives way (insteadOf, else the one started last), its thread closes and it starts again later. replace: the task gets a new brief; one that is working starts again from it. stop: the task stops; what it already changed stays.",
            "inputSchema": {"type": "object", "properties": {
                "task": {"type": "string", "description": "The task's id."},
                "how": {"type": "string", "enum": ["first", "now", "replace", "stop"]},
                "brief": {"type": "string", "description": "The new brief, for replace."},
                "insteadOf": {"type": "string", "description": "For now: the id of the working task that gives way."},
            }, "required": ["task", "how"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "work_status",
            "description": "Your event box: every task that is working or waiting, and those ended in the last day, with its status, what it does now, how long, and its result.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        }),
    ]
}

pub fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "add_task" => add(hub, bot, args),
        "steer_task" => steer_task(hub, bot, args),
        "work_status" => status(hub, bot),
        _ => bail!("unknown work tool: {name}"),
    }
}

/// How `add_task` ended: `work_added`'s `outcome`.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
enum Admission {
    Added,
    /// Not in the bot's own chat.
    Elsewhere,
    Invalid,
    /// Too many tasks wait already.
    Full,
    /// Too many in a row without the user.
    Hops,
    Failed,
}

fn add(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let added = admit(hub, bot, args);
    let outcome = added.as_ref().map_or_else(|(reason, _)| *reason, |_| Admission::Added);
    let properties = json!({"outcome": outcome, "after": args["after"].is_string(), "source": "bot"});
    analytics::capture(hub, Event::WorkAdded, properties.as_object().cloned().unwrap_or_default());
    let id = added.map_err(|(_, error)| error)?;
    dispatch(hub, bot);
    Ok(json!({
        "taskId": id, "status": Status::Waiting,
        "note": "It starts when this turn ends and a thread is free. Its report comes back to you later as a message.",
    }))
}

type Refusal = (Admission, anyhow::Error);

/// A title and a brief `add_task` takes: both there, neither too long.
fn checked<'a>(title: Option<&'a str>, brief: Option<&'a str>) -> Result<(&'a str, &'a str), Refusal> {
    let (Some(title), Some(brief)) = (title, brief) else {
        return Err((Admission::Invalid, anyhow!("title and brief are required")));
    };
    if title.chars().count() > MAX_TITLE || brief.len() > MAX_BRIEF {
        let why = anyhow!("title is at most {MAX_TITLE} characters and brief at most {MAX_BRIEF} bytes");
        return Err((Admission::Invalid, why));
    }
    Ok((title, brief))
}

/// Puts `task` last in `tasks`, unless too many wait already or it follows nothing that is working or waiting.
fn place(tasks: &mut Vec<Task>, mut task: Task) -> Result<String, Refusal> {
    if tasks.iter().filter(|i| i.status == Status::Waiting).count() >= MAX_WAITING {
        let why = anyhow!("{MAX_WAITING} tasks already wait: let some finish, or drop one, before adding more");
        return Err((Admission::Full, why));
    }
    if let Some(after) = &task.after
        && !tasks.iter().any(|i| i.id == *after && !i.status.ended())
    {
        return Err((Admission::Invalid, anyhow!("after names no task that is working or waiting: see work_status")));
    }
    task.order = tasks::last(tasks);
    let id = task.id.clone();
    tasks.push(task);
    Ok(id)
}

/// Puts a task in `bot`'s box, or says why not: the refusal's `work_added` outcome and its words for the bot.
fn admit(hub: &Hub, bot: &str, args: &Value) -> Result<String, Refusal> {
    let failed = |error: anyhow::Error| (Admission::Failed, error);
    let row = hub.store.bot(bot).map_err(failed)?.filter(|r| !r.deleted);
    row.ok_or_else(|| failed(anyhow!("unknown bot")))?;
    // A room's turn, another bot's request, a routine run: their reply goes elsewhere, and so would the report's.
    let own_chat = hub.runtime(bot).lane.is_some_and(|lane| lane.chat == bot);
    if !own_chat || hub.team.answering(bot) || hub.routines.running(bot) {
        let why = anyhow!("add_task works only in your own chat with the user: its report comes back there");
        return Err((Admission::Elsewhere, why));
    }
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let (title, brief) = checked(text("title"), text("brief"))?;
    let hops = hub.work.hops(bot) + 1;
    if hops > MAX_HOPS {
        crate::chat::team::limit::stopped(hub, bot, bot, crate::chat::team::limit::Via::Work);
        let why = anyhow!(
            "too much work was added in a row without the user: the user has a Continue button in the chat; tell them where things stand and end your turn"
        );
        return Err((Admission::Hops, why));
    }
    let task = Task::waiting(title, brief, users_words(hub, bot), text("after"), hops);
    let id = tasks::change(hub, bot, |tasks| Ok(place(tasks, task))).map_err(failed)??;
    hub.work.added.locked().entry(bot.to_owned()).or_default().push(id.clone());
    Ok(id)
}

/// The user's own typed message the bot's running turn answers.
fn users_words(hub: &Hub, bot: &str) -> Option<String> {
    let entry = hub.store.entry(&hub.runtime(bot).message?)?;
    entry.data["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

fn steer_task(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let id = text("task").ok_or_else(|| anyhow!("task is required: its id, from work_status"))?;
    let refused = || anyhow!("how is first, now, replace or stop");
    let how = Steer::deserialize(&args["how"]).map_err(|_| refused())?;
    match how {
        Steer::First => steer::first(hub, bot, id)?,
        Steer::Now => steer::cut_in(hub, bot, id, text("insteadOf"))?,
        Steer::Replace => {
            let brief = text("brief").ok_or_else(|| anyhow!("replace needs the new brief"))?;
            if brief.len() > MAX_BRIEF {
                bail!("brief is at most {MAX_BRIEF} bytes");
            }
            steer::replace(hub, bot, id, brief)?;
        }
        Steer::Stop => steer::stop(hub, bot, Some(id))?,
        Steer::Drop | Steer::Retry => return Err(refused()),
    }
    // What the user said about it stays with it: a thread that starts it again reads it.
    if let Some(words) = users_words(hub, bot) {
        tasks::change(hub, bot, |tasks| {
            if let Some(task) = tasks.iter_mut().find(|i| i.id == id && i.notes.last() != Some(&words)) {
                task.notes.push(words);
            }
            Ok(())
        })?;
    }
    let properties = json!({"how": how, "by": "bot"});
    analytics::capture(hub, Event::WorkSteered, properties.as_object().cloned().unwrap_or_default());
    let status = tasks::load(&hub.store, bot).iter().find(|i| i.id == id).map(|i| i.status);
    Ok(json!({"taskId": id, "status": status}))
}

fn status(hub: &Hub, bot: &str) -> Result<Value> {
    let cfg = hub.store.bot(bot)?.ok_or_else(|| anyhow!("unknown bot"))?.config;
    let now = now_ms();
    let tasks: Vec<Value> = tasks::shown(hub, &cfg)
        .into_iter()
        .map(|Shown { task, status, activity, wait }| {
            let seconds = task.started_at.map(|at| (task.ended_at.unwrap_or(now) - at) / 1000);
            json!({
                "taskId": task.id, "title": task.title, "status": status, "doingNow": activity, "wait": wait,
                "seconds": seconds, "result": task.result,
            })
        })
        .collect();
    Ok(json!({"tasks": tasks}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(after: Option<&str>) -> Task {
        Task::waiting("Fix login", "x", None, after, 1)
    }

    #[test]
    fn a_box_takes_twenty_waiting_tasks_and_refuses_the_next() {
        let mut tasks = Vec::new();
        for n in 0..MAX_WAITING {
            place(&mut tasks, waiting(None)).unwrap_or_else(|(_, why)| panic!("task {n}: {why}"));
        }
        assert_eq!(tasks.last().map(|i| i.order), i64::try_from(MAX_WAITING).ok(), "each one last");
        let (outcome, why) = place(&mut tasks, waiting(None)).expect_err("the box is full");
        assert_eq!(outcome, Admission::Full);
        assert_eq!(why.to_string(), "20 tasks already wait: let some finish, or drop one, before adding more");

        // Only the waiting count: one that started leaves room for another.
        tasks[0].status = Status::Working;
        let first = tasks[0].id.clone();
        place(&mut tasks, waiting(Some(&first))).unwrap_or_else(|(_, why)| panic!("after a working task: {why}"));
        tasks[1].status = Status::Done;
        let ended = tasks[1].id.clone();
        let (outcome, _) = place(&mut tasks, waiting(Some(&ended))).expect_err("it follows an ended task");
        assert_eq!(outcome, Admission::Invalid);
    }

    #[test]
    fn a_title_or_a_brief_too_long_is_refused() {
        let (title, brief) = ("t".repeat(MAX_TITLE), "b".repeat(MAX_BRIEF));
        assert!(checked(Some(&title), Some(&brief)).is_ok(), "at the limits");
        let long_title = format!("{title}t");
        let long_brief = format!("{brief}b");
        for (title, brief) in [(long_title.as_str(), "b"), ("t", long_brief.as_str())] {
            let (outcome, why) = checked(Some(title), Some(brief)).expect_err("too long");
            assert_eq!(outcome, Admission::Invalid);
            assert_eq!(why.to_string(), "title is at most 80 characters and brief at most 32000 bytes");
        }
        let (outcome, _) = checked(Some("t"), None).expect_err("no brief");
        assert_eq!(outcome, Admission::Invalid);
    }

    /// #386: work added too often in a row leaves the user one Continue notice a turn, as the handoff limit does.
    #[tokio::test]
    async fn work_past_the_limit_leaves_the_user_one_continue_notice() {
        let dir = std::env::temp_dir().join(format!("codync-work-limit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::store::Store::open(&dir.join("t.db")).unwrap();
        store
            .save_bot(&serde_json::from_value(json!({"id": "bot", "name": "B", "backend": "claude"})).unwrap())
            .unwrap();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        let hub = Hub::new(store, "test".into(), identity, "test".into(), 0);
        hub.set_runtime("bot", |r| r.lane = Some(crate::store::Lane::main("bot")));
        hub.work.answering("bot", MAX_HOPS);
        for _ in 0..2 {
            let (outcome, _) =
                admit(&hub, "bot", &json!({"title": "Dig", "brief": "Look"})).expect_err("past the limit");
            assert_eq!(outcome, Admission::Hops);
        }
        let notices = hub.store.history("bot", i64::MAX, 100).unwrap();
        let continues = notices.iter().filter(|e| e.kind == "notice" && e.data["continueChain"] == true).count();
        assert_eq!(continues, 1, "one notice a turn, however often the bot tries");
        let _ = std::fs::remove_dir_all(dir);
    }
}
