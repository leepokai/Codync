//! The voice's tools (`callTool`): what it may do for the user on the call. They run here, on the bot's chat.

use super::context::{self, RECENT};
use crate::agent::bot::Cmd;
use crate::hub::{Hub, Runtime};
use crate::store::{BotConfig, Entry, EntryKind};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::sync::Arc;

/// The tools' declarations in JSON Schema form (each client spells them for its provider).
pub fn declarations() -> Value {
    json!([
        {"name": "send_to_bot", "description": "Send a message to the bot on the computer, as if the user typed it. Its reply arrives later.",
         "parameters": {"type": "object", "properties": {"text": {"type": "string", "description": "The message"}}, "required": ["text"]}},
        {"name": "bot_status", "description": "Whether the bot is idle or working, what it is doing, and any approval it is waiting for.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "recent_messages", "description": "The latest messages in the chat with the bot, oldest first.",
         "parameters": {"type": "object", "properties": {"count": {"type": "integer", "description": "How many, 1-20"}}}},
        {"name": "answer_approval", "description": "Answer the bot's pending approval request with one of its options.",
         "parameters": {"type": "object", "properties": {"option": {"type": "string", "description": "The option's name"}}, "required": ["option"]}},
        {"name": "stop_bot", "description": "Stop what the bot is doing now.",
         "parameters": {"type": "object", "properties": {}}},
        {"name": "search_memory", "description": "Search what the bot remembers about the user and their work.",
         "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "Words to look for"}}, "required": ["query"]}},
    ])
}

/// Runs one of the voice's tool calls on call `call`'s bot, after adding what was said that the app still held
/// (`lines`). A failure is the tool's answer, for the voice to say.
pub async fn call(hub: &Arc<Hub>, call: &str, name: &str, args: &Value, lines: &Value) -> Value {
    let run = async {
        let bot = super::bot_of(&hub.store, call).ok_or_else(|| anyhow!("The call has ended."))?;
        super::add_lines(hub, call, lines)?;
        let cfg =
            hub.store.bot(&bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("The bot isn't available."))?.config;
        match name {
            "send_to_bot" => relay(hub, &cfg, call, args["text"].as_str().unwrap_or_default()),
            "bot_status" => Ok(status(hub, &cfg)),
            "recent_messages" => Ok(recent(hub, &cfg, args["count"].as_i64().unwrap_or(6))),
            "answer_approval" => answer(hub, &cfg, args["option"].as_str().unwrap_or_default()),
            "stop_bot" => stop(hub, &cfg),
            "search_memory" if !cfg.is_group() => {
                crate::chat::memory::tool_call(hub, &cfg.id, "search_memory", args).await
            }
            _ => Err(anyhow!("Unknown tool {name}.")),
        }
    };
    run.await.unwrap_or_else(|e| json!({"error": format!("{e:#}")}))
}

/// `send_to_bot`: the voice's words to the bot, with what the user said since the bot last heard from the call,
/// word for word. The message is the call's, so the chat shows it only there.
fn relay(hub: &Arc<Hub>, cfg: &BotConfig, call: &str, text: &str) -> Result<Value> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(json!({"error": "Nothing to send."}));
    }
    let said = super::take_said(&hub.store, call)?;
    let text = if said.is_empty() || said == text {
        text.to_owned()
    } else {
        format!("{text}\n\n[The user's own words on the call: «{said}»]")
    };
    let data = json!({"text": text, "attachments": [], "call": call, "from": "voice"});
    crate::chat::messages::post(hub, cfg, data)?;
    Ok(json!({"sent": true, "note": "The bot's reply will arrive as a separate message."}))
}

fn status(hub: &Hub, cfg: &BotConfig) -> Value {
    let rt = runtime(hub, cfg);
    let mut out = json!({"status": rt.status, "activity": rt.activity});
    let recent = hub.store.history(&cfg.id, i64::MAX, RECENT).unwrap_or_default();
    if let Some(w) = waiting(&recent) {
        out["approval"] = json!({"request": w.request, "options": w.options});
    }
    out
}

fn recent(hub: &Hub, cfg: &BotConfig, count: i64) -> Value {
    let said = context::said(hub, cfg, count.clamp(1, 20), 1_500);
    json!({"messages": said.into_iter().map(|(from, text)| json!({"from": from, "text": text})).collect::<Vec<_>>()})
}

/// The bot's runtime; a group's is that of the member working in it.
pub(super) fn runtime(hub: &Hub, cfg: &BotConfig) -> Runtime {
    if cfg.is_group() { hub.group_runtime(cfg) } else { hub.runtime(&cfg.id) }
}

/// An approval the bot waits for: its card, what it asks and its options' names.
pub(super) struct Waiting<'a> {
    pub card: &'a Entry,
    pub request: &'a str,
    pub options: Vec<&'a str>,
}

/// The newest approval card still pending in `recent` (oldest first).
pub(super) fn waiting(recent: &[Entry]) -> Option<Waiting<'_>> {
    let card =
        recent.iter().rev().find(|e| e.kind == EntryKind::Permission.as_str() && e.data["status"] == "pending")?;
    let (request, options) = approval(&card.data);
    Some(Waiting { card, request, options })
}

/// What an approval card asks and its options' names.
pub(super) fn approval(data: &Value) -> (&str, Vec<&str>) {
    let request = data["title"].as_str().or_else(|| data["command"].as_str()).unwrap_or_default();
    (request, data["options"].as_array().into_iter().flatten().filter_map(|o| o["name"].as_str()).collect())
}

fn answer(hub: &Hub, cfg: &BotConfig, option: &str) -> Result<Value> {
    let recent = hub.store.history(&cfg.id, i64::MAX, RECENT).unwrap_or_default();
    let Some(card) = waiting(&recent).map(|w| w.card) else {
        return Ok(json!({"error": "Nothing is waiting for approval."}));
    };
    let options = card.data["options"].as_array().cloned().unwrap_or_default();
    let Some(pick) = pick(&options, option) else {
        let names: Vec<String> = options.iter().map(option_name).collect();
        return Ok(json!({"error": "No such option.", "options": names}));
    };
    // A card in a group belongs to the bot that asked.
    let owner = card.data["author"].as_str().unwrap_or(&card.bot_id).to_owned();
    let option_id = pick["optionId"].as_str().map(str::to_owned);
    hub.send_cmd(&owner, Cmd::Permission { entry_id: card.id.clone(), option_id })?;
    Ok(json!({"answered": pick["name"]}))
}

fn option_name(o: &Value) -> String {
    o["name"].as_str().unwrap_or_default().to_lowercase()
}

/// The option the user picked by name (any case) or id, else the one whose name holds what they said or the other
/// way round.
fn pick<'a>(options: &'a [Value], wanted: &str) -> Option<&'a Value> {
    let lower = wanted.to_lowercase();
    options.iter().find(|o| option_name(o) == lower || o["optionId"] == wanted).or_else(|| {
        let near =
            |o: &&Value| !lower.is_empty() && (option_name(o).contains(&lower) || lower.contains(&option_name(o)));
        options.iter().find(near)
    })
}

fn stop(hub: &Arc<Hub>, cfg: &BotConfig) -> Result<Value> {
    if cfg.is_group() {
        hub.groups.stop(hub, &cfg.id);
    } else {
        hub.send_cmd(&cfg.id, Cmd::Stop)?;
    }
    Ok(json!({"stopped": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{BotKind, EntryKind, Lane};

    fn options() -> Vec<Value> {
        ["Allow", "Always", "Reject"]
            .iter()
            .map(|name| json!({"optionId": name.to_lowercase(), "name": name}))
            .collect()
    }

    #[test]
    fn an_approval_is_answered_by_name_id_or_part_of_either() {
        let options = options();
        let picked = |wanted: &str| pick(&options, wanted).map(|o| o["name"].as_str().unwrap().to_owned());
        assert_eq!(picked("ALLOW").as_deref(), Some("Allow"), "any case");
        assert_eq!(picked("always").as_deref(), Some("Always"), "an id");
        assert_eq!(picked("rej").as_deref(), Some("Reject"), "part of a name");
        assert_eq!(picked("Allow once").as_deref(), Some("Allow"), "a name within what was said");
        assert_eq!(picked("maybe"), None);
        assert_eq!(picked(""), None);
    }

    #[test]
    fn the_status_names_what_waits_for_approval_and_a_groups_busy_member() {
        let hub = context::tests::hub();
        let bot = context::tests::bot("b1", "Mika", "/tmp");
        hub.store.save_bot(&bot).unwrap();
        assert_eq!(status(&hub, &bot), json!({"status": "idle", "activity": ""}));
        let card = json!({"title": "Edit a.txt", "status": "pending", "options": options()});
        hub.store.insert_entry(&Lane::main("b1"), EntryKind::Permission, 1, &card).unwrap();
        assert_eq!(
            status(&hub, &bot)["approval"],
            json!({"request": "Edit a.txt", "options": ["Allow", "Always", "Reject"]})
        );

        let group =
            BotConfig { kind: BotKind::Group, members: vec!["b1".into()], ..context::tests::bot("g", "Team", "") };
        hub.set_runtime("b1", |rt| {
            rt.status = crate::hub::BotStatus::Working;
            rt.activity = "Running tests".into();
            rt.lane = Some(Lane::main("g"));
        });
        let busy = status(&hub, &group);
        assert_eq!(
            (busy["status"].as_str(), busy["activity"].as_str()),
            (Some("working"), Some("Mika: Running tests"))
        );
    }

    #[test]
    fn recent_messages_name_who_wrote_each_and_keep_to_twenty() {
        let hub = context::tests::hub();
        let bot = context::tests::bot("b1", "Mika", "/tmp");
        hub.store.save_bot(&bot).unwrap();
        let chat = Lane::main("b1");
        hub.store.insert_entry(&chat, EntryKind::User, 1, &json!({"text": "check git"})).unwrap();
        hub.store
            .insert_entry(&chat, EntryKind::User, 2, &json!({"text": "Check git", "from": "voice", "call": "c"}))
            .unwrap();
        hub.store
            .insert_entry(&chat, EntryKind::Agent, 2, &json!({"text": "**Clean.**", "final": true, "author": "b1"}))
            .unwrap();
        let said = recent(&hub, &bot, 99)["messages"].clone();
        assert_eq!(
            said,
            json!([
                {"from": "User", "text": "check git"},
                {"from": "Voice (relayed)", "text": "Check git"},
                {"from": "Mika", "text": "Clean."},
            ])
        );
        assert_eq!(recent(&hub, &bot, 0)["messages"].as_array().map(Vec::len), Some(1), "at least one");
    }
}
