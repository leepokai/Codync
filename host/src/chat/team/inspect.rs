//! What a bot asks about another bot: whether it is idle or working and what it is doing, its latest messages, and the
//! two things it may not do alone — answering an approval, and stopping it — which the user decides on a card in
//! the requesting bot's chat.

use super::visible_bot;
use crate::agent::bot::Cmd;
use crate::chat::insight::{self, RECENT_ENTRIES};
use crate::hub::Hub;
use crate::network::Recipient;
use crate::store::{BotConfig, Permission};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// How long a card waits for the user, under Codex's five-minute MCP tool timeout that drops a call without telling
/// the server.
const CONFIRM_TIMEOUT: Duration = crate::chat::manage::APPROVAL_TIMEOUT;
/// Fits between the user's approval window and the team HTTP deadline.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
/// The most of a bot's latest messages `bot_status` may add.
const MESSAGES: i64 = 20;

/// What a bot is told about these tools when its settings let it ask the user.
pub const INSTRUCTIONS: &str = "bot_status (with botId) and recent_messages (with botId) let you look at another bot \
on this computer: whether it is idle or working, what it is doing, the approval it waits for, and the latest messages \
it and the user wrote. Ask the user before acting on what you read there. answer_approval (with botId and the option's \
name) and stop_bot (with botId) are the user's decisions on another bot's work: say what should happen in your reply \
with why, and they choose.";

/// What a bot under Approve automatically is told instead: its own answer and stop need no card, still only when the
/// user asked for them.
pub const AUTO_INSTRUCTIONS: &str = "bot_status (with botId) and recent_messages (with botId) let you look at another \
bot on this computer: whether it is idle or working, what it is doing, the approval it waits for, and the latest \
messages it and the user wrote. This bot's settings approve what it asks automatically, so answer_approval (with botId \
and the option's name) and stop_bot (with botId) act at once: use them only when the user asked for exactly that, and \
tell them what you did.";

/// The tools' declarations, added to the team server's.
pub fn declarations() -> Value {
    /// A tool that reads a bot's chat, with the one argument beyond `botId` it adds: its name, and its type and what
    /// it means.
    fn reading(name: &str, description: &str, field: (&str, &str, &str)) -> Value {
        json!({
            "name": name,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "botId": {"type": "string", "description": "The bot's id from list_bots."},
                    field.0: {"type": field.1, "description": field.2},
                },
                "required": ["botId"],
            },
            "annotations": {"readOnlyHint": true},
        })
    }
    json!([
        reading(
            "bot_status",
            "Whether another Codync bot on this computer is idle or working, what it is doing, and the approval it is waiting for. Its chat is the user's: you read it, you never write in it.",
            ("messages", "integer", "0-20 (default 0). Also its latest messages, oldest first."),
        ),
        reading(
            "recent_messages",
            "The latest messages in another bot's chat with the user, oldest first: who wrote each and its text.",
            ("count", "integer", "1-20 (default 6)"),
        ),
        json!({
            "name": "answer_approval",
            "description": "Answer the approval another bot is waiting for with one of its options, after the user told you which. Under Ask, a card in your chat waits up to four and a half minutes; under Auto it answers at once. A card that stopped waiting is reported as stale.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "botId": {"type": "string", "description": "The bot's id from list_bots."},
                    "option": {"type": "string", "description": "The option's name, as bot_status lists it."},
                },
                "required": ["botId", "option"],
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "stop_bot",
            "description": "Request cancellation of another bot's current turn after the user asked for it, preserving queued messages. Under Ask a card in your chat decides; under Auto it requests cancellation at once. stopRequested means the target accepted the request; check bot_status for completion. If its turn changed, nothing is stopped and the result is stale.",
            "inputSchema": {
                "type": "object",
                "properties": {"botId": {"type": "string", "description": "The bot's id from list_bots."}},
                "required": ["botId"],
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
    ])
}

/// One of these tools, from `source` (`actor`: its running turn or task), among this computer's bots.
pub async fn call(hub: &Arc<Hub>, source: &BotConfig, actor: &str, name: &str, args: &Value) -> Result<Value> {
    let target = resolve_target(hub, source, name, args)?;
    match name {
        "bot_status" => status(hub, &target, args["messages"].as_i64().unwrap_or(0)),
        "recent_messages" => insight::recent(hub, &target, args["count"].as_i64().unwrap_or(6)),
        "answer_approval" => answer(hub, source, actor, &target, args["option"].as_str().unwrap_or_default()).await,
        "stop_bot" => stop(hub, source, actor, &target).await,
        _ => bail!("unknown team tool: {name}"),
    }
}

fn resolve_target(hub: &Hub, source: &BotConfig, name: &str, args: &Value) -> Result<BotConfig> {
    match target(hub, args) {
        Ok(target) if target.id != source.id => Ok(target),
        Ok(_) => {
            record_asking(hub, name, "invalid", None, Some("bad_target"));
            bail!("that is you: {name} is about another bot");
        }
        Err(error) => {
            record_asking(hub, name, "invalid", None, Some("bad_target"));
            Err(error)
        }
    }
}

/// The bot these tools are about, on this computer: its chat and its approvals are here.
fn target(hub: &Hub, args: &Value) -> Result<BotConfig> {
    let id = args["botId"].as_str().map(str::trim).filter(|id| !id.is_empty());
    let id = id.ok_or_else(|| anyhow!("botId is required: the bot's id from list_bots"))?;
    match hub.network.recipient(id)? {
        Recipient::Here(_) => visible_bot(hub, id),
        Recipient::There(at) => {
            bail!(
                "{} is on another computer: its chat, approval and work are that computer's, so ask it with message_bot",
                at.address
            )
        }
    }
}

/// The target's latest entries, so an unreadable history says so instead of reading as "nothing waiting".
fn recent(hub: &Hub, cfg: &BotConfig) -> Result<Vec<crate::store::Entry>> {
    hub.store
        .history(&cfg.id, i64::MAX, RECENT_ENTRIES)
        .with_context(|| format!("couldn't read {}'s history", cfg.name))
}

/// `bot_status`: its runtime, the approval it waits for, and the latest messages it and the user wrote.
fn status(hub: &Hub, cfg: &BotConfig, messages: i64) -> Result<Value> {
    let mut out = insight::status(hub, cfg, &recent(hub, cfg)?);
    out["id"] = cfg.id.clone().into();
    out["name"] = cfg.name.clone().into();
    if messages > 0 {
        out["recentMessages"] = insight::recent(hub, cfg, messages.min(MESSAGES))?["messages"].clone();
    }
    Ok(out)
}

/// `answer_approval`: one option of the approval the target waits for, once the user says so.
async fn answer(hub: &Arc<Hub>, source: &BotConfig, actor: &str, target: &BotConfig, option: &str) -> Result<Value> {
    let tool = "answer_approval";
    let recent = recent(hub, target)?;
    let Some(waiting) = insight::waiting(&recent) else {
        record_outcome(hub, tool, "invalid", None, Some("not_waiting"));
        return Ok(json!({"error": format!("{} isn't waiting for an approval.", target.name)}));
    };
    let options = waiting.card.data["options"].as_array().cloned().unwrap_or_default();
    let Some(pick) = pick(&options, option) else {
        record_outcome(hub, tool, "invalid", None, Some("bad_option"));
        return Ok(json!({"error": "No such option.", "options": waiting.options}));
    };
    let Some(option_id) = pick["optionId"].as_str().map(str::to_owned) else {
        record_outcome(hub, tool, "failed", None, Some("missing_option_id"));
        bail!("the approval option has no id");
    };
    let picked = pick["name"].as_str().unwrap_or_default().to_owned();
    let detail = format!("{}: {}\nThe option: {picked}", target.name, waiting.request);
    let Some(how) = confirm(hub, "Answer an approval", source, actor, &detail, tool).await? else {
        record_outcome(hub, tool, "declined", None, None);
        return Ok(declined());
    };
    let owner = crate::chat::tasks::card_actor(waiting.card).to_owned();
    let cmd = |reply| Cmd::AnswerPermission { entry_id: waiting.card.id.clone(), option_id, reply };
    let applied = send_confirmed(
        hub,
        tool,
        &owner,
        cmd,
        "the target did not confirm the answer; check its status before trying again",
    )
    .await?;
    if !applied {
        return Ok(
            json!({"done": false, "stale": true, "note": "That approval is no longer waiting; no answer was applied."}),
        );
    }
    asked(hub, tool, how);
    Ok(answered(json!({"answered": picked}), how))
}

/// `stop_bot`: the target's current work ends, once the user asks for it.
async fn stop(hub: &Arc<Hub>, source: &BotConfig, actor: &str, target: &BotConfig) -> Result<Value> {
    let tool = "stop_bot";
    let rt = insight::runtime(hub, target);
    let Some(key) = rt.turn_key else {
        record_outcome(hub, tool, "invalid", None, Some("not_running"));
        return Ok(json!({"done": false, "note": "There is no running turn to stop."}));
    };
    let doing = match rt.activity.trim() {
        "" => "its work".to_owned(),
        doing => doing.to_owned(),
    };
    let detail = format!("{} is {doing}", target.name);
    let Some(how) = confirm(hub, &format!("Stop {}", target.name), source, actor, &detail, tool).await? else {
        record_outcome(hub, tool, "declined", None, None);
        return Ok(declined());
    };
    let accepted = send_confirmed(
        hub,
        tool,
        &target.id,
        |reply| Cmd::StopTurn { key, reply },
        "the target did not confirm the stop request; check its status before trying again",
    )
    .await?;
    if !accepted {
        return Ok(
            json!({"done": false, "stale": true, "note": "That turn is no longer running; nothing else was stopped."}),
        );
    }
    asked(hub, tool, how);
    Ok(answered(json!({"stopRequested": true}), how))
}

async fn send_confirmed(
    hub: &Hub,
    tool: &str,
    actor: &str,
    build: impl FnOnce(tokio::sync::oneshot::Sender<bool>) -> Cmd,
    timeout_msg: &str,
) -> Result<bool> {
    let (reply, ack) = tokio::sync::oneshot::channel();
    if let Err(error) = hub.send_cmd(actor, build(reply)) {
        record_outcome(hub, tool, "failed", None, Some("send_failed"));
        return Err(error);
    }
    match tokio::time::timeout(ANSWER_TIMEOUT, ack).await {
        Ok(Ok(true)) => Ok(true),
        Ok(Ok(false) | Err(_)) => {
            record_outcome(hub, tool, "stale", None, None);
            Ok(false)
        }
        Err(_) => {
            record_outcome(hub, tool, "failed", None, Some("ack_timeout"));
            bail!("{timeout_msg}");
        }
    }
}

/// What the user decided about another bot, as `bot_inspected`: which tool asked, and who decided.
fn asked(hub: &Arc<Hub>, tool: &str, how: Confirmation) {
    let by = confirmation_by(how);
    let properties =
        [("tool".to_owned(), json!(tool)), ("by".to_owned(), json!(by)), ("client".to_owned(), json!("local"))];
    crate::analytics::capture(hub, crate::analytics::Event::BotInspected, properties.into_iter().collect());
    record_outcome(hub, tool, "done", Some(how), None);
}

fn confirmation_by(how: Confirmation) -> &'static str {
    match how {
        Confirmation::User => "user",
        Confirmation::Auto => "auto",
    }
}

fn record_asking(hub: &Hub, tool: &str, outcome: &str, how: Option<Confirmation>, error: Option<&str>) {
    if super::ASKING.contains(&tool) {
        record_outcome(hub, tool, outcome, how, error);
    }
}

fn record_outcome(hub: &Hub, tool: &str, outcome: &str, how: Option<Confirmation>, error: Option<&str>) {
    let mut properties = serde_json::Map::from_iter([
        ("tool".into(), json!(tool)),
        ("outcome".into(), json!(outcome)),
        ("client".into(), json!("local")),
    ]);
    if let Some(how) = how {
        properties.insert("by".into(), json!(confirmation_by(how)));
    }
    if let Some(error) = error {
        properties.insert("error".into(), json!(error));
    }
    crate::analytics::capture(hub, crate::analytics::Event::BotInspectOutcome, properties);
}

/// What the user did.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirmation {
    /// On the card.
    User,
    /// By the source's own Approve automatically, which needs no card.
    Auto,
}

/// The answer to a call that went ahead.
fn answered(mut out: Value, how: Confirmation) -> Value {
    if how == Confirmation::Auto {
        out["approved"] = "automatically: this bot's settings approve what it asks without a card".into();
    }
    out
}

/// The answer to a call the user declined.
fn declined() -> Value {
    json!({"done": false, "note": "The user declined: nothing changed."})
}

/// Match the exact name first, then only an unambiguous partial name.
fn pick<'a>(options: &'a [Value], wanted: &str) -> Option<&'a Value> {
    let lower = wanted.trim().to_lowercase();
    let name = |o: &Value| insight::option_name(o).unwrap_or_default().to_lowercase();
    options.iter().find(|o| name(o) == lower).or_else(|| {
        let near = |o: &&Value| {
            let option = name(o);
            option.len() > 1 && !lower.is_empty() && (option.contains(&lower) || lower.contains(&option))
        };
        let mut matches = options.iter().filter(near);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

/// Asks the user (`title`, `detail`) on a card in the source's chat, hosted by `actor`.
/// `None` when the user declined or the card expired;
/// an error when no answer came in [`CONFIRM_TIMEOUT`].
async fn confirm(
    hub: &Arc<Hub>,
    title: &str,
    source: &BotConfig,
    actor: &str,
    detail: &str,
    tool: &str,
) -> Result<Option<Confirmation>> {
    if source.permission == Permission::Auto {
        return Ok(Some(Confirmation::Auto));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Err(error) =
        hub.send_cmd(actor, Cmd::Approve { title: title.to_owned(), detail: detail.to_owned(), reply: tx })
    {
        record_outcome(hub, tool, "failed", None, Some("confirm_failed"));
        return Err(anyhow!("the card couldn't wait for the user: {error}"));
    }
    match tokio::time::timeout(CONFIRM_TIMEOUT, rx).await {
        Ok(Ok(true)) => Ok(Some(Confirmation::User)),
        // Declined, or the card expired (the turn stopped): either way, nothing happens.
        Ok(Ok(false) | Err(_)) => Ok(None),
        Err(_) => {
            record_outcome(hub, tool, "failed", None, Some("confirm_timeout"));
            bail!("the user didn't answer within four and a half minutes: nothing changed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Entry, EntryKind, Lane, Store};
    use tokio::sync::mpsc;

    fn options() -> Vec<Value> {
        ["Allow", "Always", "Reject"]
            .iter()
            .map(|name| json!({"optionId": name.to_lowercase(), "name": name}))
            .collect()
    }

    fn temp_hub() -> (Arc<Hub>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-inspect-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        (Hub::new(store, "h".into(), identity, "t".into(), 0), dir)
    }

    fn bot(id: &str, permission: Permission) -> BotConfig {
        let mut cfg: BotConfig =
            serde_json::from_value(json!({"id": id, "name": id, "cwd": "/tmp", "backend": "claude"})).unwrap();
        cfg.permission = permission;
        cfg
    }

    fn register_bot(hub: &Hub, cfg: &BotConfig) -> mpsc::UnboundedReceiver<Cmd> {
        hub.store.save_bots(std::slice::from_ref(cfg)).unwrap();
        hub.attach_bot_for_test(&cfg.id)
    }

    fn add_pending_card(hub: &Hub, bot_id: &str, task_id: Option<&str>) -> Entry {
        let mut data = json!({
            "title": "Secret task action",
            "status": "pending",
            "options": [{"optionId": "allow", "name": "Allow", "kind": "allow_once"}],
        });
        if let Some(task_id) = task_id {
            data["task"] = json!({"id": task_id, "title": "Secret work"});
        }
        hub.add_entry(&Lane::main(bot_id), EntryKind::Permission, 1, &data).unwrap()
    }

    fn drain_names(hub: &Hub) -> Vec<(String, Value)> {
        hub.analytics
            .drain_test_events()
            .into_iter()
            .map(|event| (event["event"].as_str().unwrap().to_owned(), event["properties"].clone()))
            .collect()
    }

    #[test]
    fn an_option_is_found_by_name_case_or_part_of_either() {
        let options = options();
        let picked = |wanted: &str| pick(&options, wanted).map(|o| o["name"].as_str().unwrap().to_owned());
        assert_eq!(picked("ALLOW").as_deref(), Some("Allow"), "any case");
        assert_eq!(picked("Allow once").as_deref(), Some("Allow"), "a name within what was said");
        assert_eq!(picked("rej").as_deref(), Some("Reject"), "part of a name");
        assert_eq!(picked("Maybe"), None);
        assert_eq!(picked(""), None, "nothing said picks nothing");
    }

    #[test]
    fn ambiguous_approval_names_choose_nothing() {
        let options = vec![json!({"name": "Allow once"}), json!({"name": "Always allow"})];
        assert!(pick(&options, "allow").is_none());
        assert_eq!(pick(&options, "Allow once").unwrap()["name"], "Allow once");
    }

    #[test]
    fn the_tools_say_what_the_users_own_setting_changes() {
        assert!(INSTRUCTIONS.contains("they choose") && INSTRUCTIONS.contains("answer_approval"));
        assert!(AUTO_INSTRUCTIONS.contains("at once") && !AUTO_INSTRUCTIONS.contains("they choose"));
        let names: Vec<String> =
            declarations().as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
        assert_eq!(names, ["bot_status", "recent_messages", "answer_approval", "stop_bot"]);
    }

    #[tokio::test]
    async fn asking_actions_record_outcomes_and_route_task_and_actor_commands() {
        let (hub, dir) = temp_hub();
        let lead = bot("lead-secret", Permission::Ask);
        let worker = bot("worker-secret", Permission::Ask);
        let _lead_rx = register_bot(&hub, &lead);
        let _worker_rx = register_bot(&hub, &worker);
        let mut routine_rx = hub.attach_bot_for_test("routine-secret");
        let mut task_rx = hub.attach_bot_for_test("task-secret");

        // Invalid: target not waiting
        let res =
            call(&hub, &lead, "routine-secret", "answer_approval", &json!({"botId": worker.id, "option": "Allow"}))
                .await
                .unwrap();
        assert!(res["error"].as_str().unwrap().contains("isn't waiting"));
        let events = drain_names(&hub);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "bot_inspect_outcome");
        assert_eq!(events[0].1["tool"], "answer_approval");
        assert_eq!(events[0].1["outcome"], "invalid");
        assert_eq!(events[0].1["error"], "not_waiting");
        assert!(!events[0].1.to_string().contains("secret"));

        // Applied via calling actor (`routine-secret`) and card task actor (`task-secret`)
        let card = add_pending_card(&hub, &worker.id, Some("task-secret"));
        let hub_call = Arc::clone(&hub);
        let lead_clone = lead.clone();
        let worker_id = worker.id.clone();
        let handle = tokio::spawn(async move {
            call(
                &hub_call,
                &lead_clone,
                "routine-secret",
                "answer_approval",
                &json!({"botId": worker_id, "option": "Allow"}),
            )
            .await
        });
        let approve = routine_rx.recv().await.unwrap();
        let Cmd::Approve { reply, .. } = approve else { panic!("expected Cmd::Approve on routine actor") };
        let _ = reply.send(true);
        let perm = task_rx.recv().await.unwrap();
        let Cmd::AnswerPermission { entry_id, option_id, reply } = perm else {
            panic!("expected Cmd::AnswerPermission on task actor")
        };
        assert_eq!(entry_id, card.id);
        assert_eq!(option_id, "allow");
        let _ = reply.send(true);

        let outcome = handle.await.unwrap().unwrap();
        assert_eq!(outcome, json!({"answered": "Allow"}));
        let events = drain_names(&hub);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "bot_inspected");
        assert_eq!(events[1].0, "bot_inspect_outcome");
        assert_eq!(events[1].1["outcome"], "done");
        assert_eq!(events[1].1["by"], "user");

        // Send failed on confirmation actor (`missing-actor`)
        let err =
            call(&hub, &lead, "missing-actor", "answer_approval", &json!({"botId": worker.id, "option": "Allow"}))
                .await
                .unwrap_err();
        assert!(err.to_string().contains("couldn't wait"));
        let events = drain_names(&hub);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "bot_inspect_outcome");
        assert_eq!(events[0].1["outcome"], "failed");
        assert_eq!(events[0].1["error"], "confirm_failed");

        // Stale when stop_bot has no running turn -> invalid/not_running, no BotInspected
        let stop_res = call(&hub, &lead, &lead.id, "stop_bot", &json!({"botId": worker.id})).await.unwrap();
        assert_eq!(stop_res["done"], false);
        let events = drain_names(&hub);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "bot_inspect_outcome");
        assert_eq!(events[0].1["tool"], "stop_bot");
        assert_eq!(events[0].1["outcome"], "invalid");
        assert_eq!(events[0].1["error"], "not_running");

        let _ = std::fs::remove_dir_all(dir);
    }
}
