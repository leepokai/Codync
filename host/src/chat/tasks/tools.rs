//! The `chat` server's work tools (`add_work`, `steer_work`, `work_status`), and what a task itself may not call.

use super::items::{self, Item, Shown, Status};
use super::steer::{self, Steer};
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
pub const TOOLS: [&str; 3] = ["add_work", "steer_work", "work_status"];

/// A task can't do what only makes sense from the chat. `what`: the tool, as the refusal names it.
pub fn refuse_in_task(hub: &Hub, bot: &str, what: &str) -> Result<()> {
    if hub.store.bot(bot)?.is_some_and(|row| row.config.is_task()) {
        bail!("{what} isn't available in a task: say in your report what you need, and you will handle it in the chat");
    }
    Ok(())
}

/// The bot a call to the servers that are the bot's own (`routines`, `team`) acts as. A task on a routine run or on
/// another bot's request is its bot there, as that turn was when it ran in the chat (a run saves its progress and
/// sets follow-ups; an asked bot runs its own routine); a task on the bot's own work is refused.
pub fn acting_as(hub: &Hub, bot: &str, what: &str) -> Result<String> {
    let Some(cfg) = hub.store.bot(bot)?.map(|row| row.config).filter(crate::store::BotConfig::is_task) else {
        return Ok(bot.to_owned());
    };
    if sources::source_of(&hub.store, &cfg).is_bot() {
        refuse_in_task(hub, bot, what)?;
    }
    Ok(cfg.owner().to_owned())
}

/// Refuses a task the `chat` server's tools that change the bot or its box (`prompts::item_prompt` tells it so).
/// What asks the user or shares with them goes to the task's actor, which posts it in the bot's chat.
pub fn refuse_chat_only(hub: &Hub, bot: &str, name: &str) -> Result<()> {
    use crate::chat::{fallbacks, profile};
    let chat_only = TOOLS.contains(&name) || profile::TOOLS.contains(&name) || fallbacks::TOOLS.contains(&name);
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
    let properties = json!({"outcome": outcome, "after": args["after"].is_string(), "source": "bot"});
    analytics::capture(hub, Event::WorkAdded, properties.as_object().cloned().unwrap_or_default());
    let id = added.map_err(|(_, error)| error)?;
    dispatch(hub, bot);
    Ok(json!({
        "itemId": id, "status": Status::Waiting,
        "note": "It starts when this turn ends and a place is free. Its report comes back to you later as a message.",
    }))
}

type Refusal = (Admission, anyhow::Error);

/// A title and a brief `add_work` takes: both there, neither too long.
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

/// Puts `item` last in `items`, unless too many wait already or it follows nothing that is working or waiting.
fn place(items: &mut Vec<Item>, mut item: Item) -> Result<String, Refusal> {
    if items.iter().filter(|i| i.status == Status::Waiting).count() >= MAX_WAITING {
        let why = anyhow!("{MAX_WAITING} items already wait: let some finish, or drop one, before adding more");
        return Err((Admission::Full, why));
    }
    if let Some(after) = &item.after
        && !items.iter().any(|i| i.id == *after && !i.status.ended())
    {
        return Err((Admission::Invalid, anyhow!("after names no item that is working or waiting: see work_status")));
    }
    item.order = items::last(items);
    let id = item.id.clone();
    items.push(item);
    Ok(id)
}

/// Puts an item in `bot`'s box, or says why not: the refusal's `work_added` outcome and its words for the bot.
fn admit(hub: &Hub, bot: &str, args: &Value) -> Result<String, Refusal> {
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
    let (title, brief) = checked(text("title"), text("brief"))?;
    let hops = hub.tasks.hops(bot) + 1;
    if hops > MAX_HOPS {
        let why =
            anyhow!("too much work was added in a row without the user: tell the user where things stand and wait");
        return Err((Admission::Hops, why));
    }
    let item = Item::waiting(title, brief, users_words(hub, bot), text("after"), hops);
    let id = items::change(hub, bot, |items| Ok(place(items, item))).map_err(failed)??;
    hub.tasks.added.locked().entry(bot.to_owned()).or_default().push(id.clone());
    Ok(id)
}

/// The user's own typed message the bot's running turn answers.
fn users_words(hub: &Hub, bot: &str) -> Option<String> {
    let entry = hub.store.entry(&hub.runtime(bot).message?)?;
    entry.data["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

fn steer_item(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let id = text("item").ok_or_else(|| anyhow!("item is required: its id, from work_status"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(after: Option<&str>) -> Item {
        Item::waiting("Fix login", "x", None, after, 1)
    }

    #[test]
    fn a_box_takes_twenty_waiting_items_and_refuses_the_next() {
        let mut items = Vec::new();
        for n in 0..MAX_WAITING {
            place(&mut items, waiting(None)).unwrap_or_else(|(_, why)| panic!("item {n}: {why}"));
        }
        assert_eq!(items.last().map(|i| i.order), i64::try_from(MAX_WAITING).ok(), "each one last");
        let (outcome, why) = place(&mut items, waiting(None)).expect_err("the box is full");
        assert_eq!(outcome, Admission::Full);
        assert_eq!(why.to_string(), "20 items already wait: let some finish, or drop one, before adding more");

        // Only the waiting count: one that started leaves room for another.
        items[0].status = Status::Working;
        let first = items[0].id.clone();
        place(&mut items, waiting(Some(&first))).unwrap_or_else(|(_, why)| panic!("after a working item: {why}"));
        items[1].status = Status::Done;
        let ended = items[1].id.clone();
        let (outcome, _) = place(&mut items, waiting(Some(&ended))).expect_err("it follows an ended item");
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
}
