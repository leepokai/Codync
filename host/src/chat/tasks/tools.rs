//! The `chat` server's work tools (`add_work`, `steer_work`, `work_status`), and what a task itself may not call.

use super::items::{self, Item, Shown, Status};
use super::{MAX_BRIEF, MAX_HOPS, MAX_TITLE, MAX_WAITING, dispatch, steer};
use crate::LockExt;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// The `chat` server's tools that are the box's.
pub const TOOLS: [&str; 3] = ["add_work", "steer_work", "work_status"];

/// A task can't do what only makes sense from the chat. `what`: the tool, as the refusal names it.
pub fn refuse_in_task(hub: &Hub, bot: &str, what: &str) -> Result<()> {
    if hub.store.bot(bot)?.is_some_and(|row| row.config.is_task()) {
        bail!("{what} isn't available in a task: say in your report what you need, and you will handle it in the chat");
    }
    Ok(())
}

/// Refuses a task the `chat` server's tools that ask the user, share with them, change the bot or its box
/// (`prompts::item_prompt` tells it so).
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
            "name": "add_work",
            "description": "Put long work in your box: another session of you does it away from this chat, and you stay free to answer the user. It starts when this turn ends and a place is free; until then it waits its turn. Its report comes back to you later as a message. One item per piece of work.",
            "inputSchema": {"type": "object", "properties": {
                "title": {"type": "string", "description": "A few words the user sees while it works, e.g. \"Fix the login redirect\"."},
                "brief": {"type": "string", "description": "Everything needed to do the work: the goal, what you already know, what done looks like."},
                "after": {"type": "string", "description": "The id of an item that is working or waiting: this one is done after it, in the same session, knowing what that one did. For more of the same work."},
            }, "required": ["title", "brief"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "steer_work",
            "description": "Change an item in your box. first: a waiting item moves to the front. now: a waiting item starts now; with every place taken a working item gives way (insteadOf, else the one started last), its session closes and it starts again later. replace: the item gets a new brief; one that is working starts again from it. stop: the item stops; what it already changed stays.",
            "inputSchema": {"type": "object", "properties": {
                "item": {"type": "string", "description": "The item's id."},
                "how": {"type": "string", "enum": ["first", "now", "replace", "stop"]},
                "brief": {"type": "string", "description": "The new brief, for replace."},
                "insteadOf": {"type": "string", "description": "For now: the id of the working item that gives way."},
            }, "required": ["item", "how"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "work_status",
            "description": "Your box: every item that is working or waiting, and those ended in the last day, with its status, what it does now, how long, and its result.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        }),
    ]
}

pub fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "add_work" => add(hub, bot, args),
        "steer_work" => steer_item(hub, bot, args),
        "work_status" => status(hub, bot),
        _ => bail!("unknown work tool: {name}"),
    }
}

/// How `add_work` ended: `work_added`'s `outcome`.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
enum Admission {
    Added,
    /// Not in the bot's own chat.
    Elsewhere,
    Invalid,
    /// Too many items wait already.
    Full,
    /// Too many in a row without the user.
    Hops,
    Failed,
}

fn add(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let added = admit(hub, bot, args);
    let outcome = added.as_ref().map_or_else(|(reason, _)| *reason, |_| Admission::Added);
    let properties = json!({"outcome": outcome, "after": args["after"].is_string()});
    analytics::capture(hub, Event::WorkAdded, properties.as_object().cloned().unwrap_or_default());
    let id = added.map_err(|(_, error)| error)?;
    dispatch(hub, bot);
    Ok(json!({
        "itemId": id, "status": Status::Waiting,
        "note": "It starts when this turn ends and a place is free. Its report comes back to you later as a message.",
    }))
}

/// Puts an item in `bot`'s box, or says why not: the refusal's `work_added` outcome and its words for the bot.
fn admit(hub: &Hub, bot: &str, args: &Value) -> Result<String, (Admission, anyhow::Error)> {
    let failed = |error: anyhow::Error| (Admission::Failed, error);
    let row = hub.store.bot(bot).map_err(failed)?.filter(|r| !r.deleted);
    row.ok_or_else(|| failed(anyhow!("unknown bot")))?;
    // A room's turn, another bot's request, a routine run: their reply goes elsewhere, and so would the report's.
    let own_chat = hub.runtime(bot).lane.is_some_and(|lane| lane.chat == bot);
    if !own_chat || hub.team.answering(bot) || hub.routines.running(bot) {
        let why = anyhow!("add_work works only in your own chat with the user: its report comes back there");
        return Err((Admission::Elsewhere, why));
    }
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let (Some(title), Some(brief)) = (text("title"), text("brief")) else {
        return Err((Admission::Invalid, anyhow!("title and brief are required")));
    };
    if title.chars().count() > MAX_TITLE || brief.len() > MAX_BRIEF {
        let why = anyhow!("title is at most {MAX_TITLE} characters and brief at most {MAX_BRIEF} bytes");
        return Err((Admission::Invalid, why));
    }
    let hops = hub.tasks.hops(bot) + 1;
    if hops > MAX_HOPS {
        let why =
            anyhow!("too much work was added in a row without the user: tell the user where things stand and wait");
        return Err((Admission::Hops, why));
    }
    let mut refused = None;
    let added = items::change(hub, bot, |items| {
        if items.iter().filter(|i| i.status == Status::Waiting).count() >= MAX_WAITING {
            let why = anyhow!("{MAX_WAITING} items already wait: let some finish, or drop one, before adding more");
            refused = Some((Admission::Full, why));
        } else if let Some(after) = text("after")
            && !items.iter().any(|i| i.id == after && !i.status.ended())
        {
            let why = anyhow!("after names no item that is working or waiting: see work_status");
            refused = Some((Admission::Invalid, why));
        }
        if refused.is_some() {
            return Ok(None);
        }
        let item = Item {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_owned(),
            brief: brief.to_owned(),
            words: users_words(hub, bot),
            notes: Vec::new(),
            status: Status::Waiting,
            order: items::last(items),
            after: text("after").map(str::to_owned),
            task: None,
            cut_in: false,
            restarted: false,
            hops,
            created_at: now_ms(),
            started_at: None,
            ended_at: None,
            result: None,
            report: None,
            start: None,
            fork_from: None,
            fork_failed: false,
        };
        let id = item.id.clone();
        items.push(item);
        Ok(Some(id))
    });
    match (added.map_err(failed)?, refused) {
        (Some(id), _) => {
            hub.tasks.added.locked().entry(bot.to_owned()).or_default().push(id.clone());
            Ok(id)
        }
        (None, refused) => Err(refused.unwrap_or_else(|| failed(anyhow!("couldn't add the work")))),
    }
}

/// The user's own typed message the bot's running turn answers.
fn users_words(hub: &Hub, bot: &str) -> Option<String> {
    let entry = hub.store.entry(&hub.runtime(bot).message?)?;
    entry.data["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// What `steer_work` does to an item: `work_steered`'s `how`.
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
enum How {
    First,
    Now,
    Replace,
    Stop,
}

fn steer_item(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let id = text("item").ok_or_else(|| anyhow!("item is required: its id, from work_status"))?;
    let how = How::deserialize(&args["how"]).map_err(|_| anyhow!("how is first, now, replace or stop"))?;
    match how {
        How::First => steer::first(hub, bot, id)?,
        How::Now => steer::cut_in(hub, bot, id, text("insteadOf"))?,
        How::Replace => {
            let brief = text("brief").ok_or_else(|| anyhow!("replace needs the new brief"))?;
            if brief.len() > MAX_BRIEF {
                bail!("brief is at most {MAX_BRIEF} bytes");
            }
            steer::replace(hub, bot, id, brief)?;
        }
        How::Stop => steer::stop(hub, bot, Some(id))?,
    }
    // What the user said about it stays with it: a task that starts it again reads it.
    if let Some(words) = users_words(hub, bot) {
        items::change(hub, bot, |items| {
            if let Some(item) = items.iter_mut().find(|i| i.id == id && i.notes.last() != Some(&words)) {
                item.notes.push(words);
            }
            Ok(())
        })?;
    }
    let properties = json!({"how": how, "by": "bot"});
    analytics::capture(hub, Event::WorkSteered, properties.as_object().cloned().unwrap_or_default());
    let status = items::load(&hub.store, bot).iter().find(|i| i.id == id).map(|i| i.status);
    Ok(json!({"itemId": id, "status": status}))
}

fn status(hub: &Hub, bot: &str) -> Result<Value> {
    let cfg = hub.store.bot(bot)?.ok_or_else(|| anyhow!("unknown bot"))?.config;
    let now = now_ms();
    let work: Vec<Value> = items::shown(hub, &cfg)
        .into_iter()
        .map(|Shown { item, status, activity, wait }| {
            let seconds = item.started_at.map(|at| (item.ended_at.unwrap_or(now) - at) / 1000);
            json!({
                "itemId": item.id, "title": item.title, "status": status, "doingNow": activity, "wait": wait,
                "seconds": seconds, "result": item.result,
            })
        })
        .collect();
    Ok(json!({"work": work}))
}
