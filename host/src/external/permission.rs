//! A terminal agent asking to use a tool: an approval card on every client, and the hook held open for the answer.

use super::event::Event;
use super::session::{bot_for, turn_of};
use super::{DECISION_WAIT, Decision, External, reply, tool};
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::remote::push::{self, AlertKind};
use crate::store::{BotConfig, EntryKind, Lane};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::oneshot;

const ALLOW: &str = "allow";
const DENY: &str = "deny";

fn card(ev: &Event, bot: &str, cwd: &str) -> Value {
    let title = tool::title(&ev.tool, &ev.input);
    let command = if tool::kind(&ev.tool) == "execute" {
        ev.input.get("command").or_else(|| ev.input.get("CommandLine")).and_then(Value::as_str).map(str::to_owned)
    } else {
        None
    };
    json!({
        "title": title,
        "toolKind": tool::kind(&ev.tool),
        "command": command,
        "detail": "",
        "diffs": ev.diffs(),
        "cwd": cwd,
        "options": [
            {"optionId": ALLOW, "name": "Allow", "kind": "allow_once"},
            {"optionId": DENY, "name": "Deny", "kind": "reject_once"},
        ],
        "status": "pending",
        "selected": Value::Null,
        "author": bot,
    })
}

/// Closes the card when the hook goes away unanswered: the person answered in the terminal, the agent gave up, or
/// the wait ran out. Whatever the card still says is no longer true.
struct Waiting {
    hub: Arc<Hub>,
    entry: String,
    bot: String,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        if self.hub.external.state.locked().waiting.remove(&self.entry).is_none() {
            return; // answered: the answer already settled the card
        }
        if let Some(mut entry) = self.hub.store.entry(&self.entry) {
            entry.data["status"] = "expired".into();
            self.hub.set_entry(&entry.id, &entry.data);
        }
        self.hub.set_runtime(&self.bot, |r| {
            r.status = BotStatus::Working;
            r.activity = "Asking in the terminal…".into();
        });
    }
}

/// The person's decision, or `None` when nobody decides in time and the terminal should ask as usual.
pub(super) async fn request(hub: &Arc<Hub>, ev: &Event) -> Option<Decision> {
    // A question isn't an approval; the terminal shows it. Neither is a request from an agent that can't be answered.
    if ev.tool == "AskUserQuestion" || !reply::takes_decisions(&ev.agent) {
        return None;
    }
    let (bot, config) = target(hub, ev)?;
    let data = card(ev, &bot, &config.cwd);
    // Forbidden for every bot: the terminal is told to refuse, and no card asks.
    if data["command"].as_str().is_some_and(|command| crate::agent::blocklist::blocked(command).is_some()) {
        return Some(Decision::Deny);
    }
    let title = data["title"].as_str().unwrap_or("Use a tool").to_owned();
    wait_card(hub, ev, &bot, &config, &data, &title).await
}

/// The bot standing for the event's session and its settings.
pub(super) fn target(hub: &Arc<Hub>, ev: &Event) -> Option<(String, BotConfig)> {
    let bot = bot_for(hub, ev)?;
    let config = hub.store.bot(&bot).ok().flatten()?.config;
    Some((bot, config))
}

/// Shows `data` as a card, tells the person and holds until it is answered; `None` when nobody answers in time.
pub(super) async fn wait_card(
    hub: &Arc<Hub>,
    ev: &Event,
    bot: &str,
    config: &BotConfig,
    data: &Value,
    title: &str,
) -> Option<Decision> {
    let entry = hub.add_entry(&Lane::main(bot), EntryKind::Permission, turn_of(hub, &ev.session), data)?;

    let (tx, rx) = oneshot::channel();
    hub.external.state.locked().waiting.insert(entry.id.clone(), tx);
    let waiting = Waiting { hub: hub.clone(), entry: entry.id, bot: bot.to_owned() };
    hub.set_runtime(bot, |r| {
        r.status = BotStatus::NeedsInput;
        r.activity = format!("Needs approval: {title}");
        r.lane = Some(Lane::main(bot));
    });
    push::notify(hub, config, Some(bot), &format!("{} needs you", config.name), title, AlertKind::NeedsInput);

    let answer = tokio::time::timeout(DECISION_WAIT, rx).await;
    drop(waiting);
    answer.ok()?.ok()?
}

/// See [`External::answer`].
pub(super) fn answer(external: &External, hub: &Hub, entry_id: &str, option_id: Option<&str>) -> bool {
    let Some(mut entry) = hub.store.entry(entry_id) else { return is_ours(hub, entry_id) };
    // An answer must be one of the options the card showed; anything else leaves the card waiting.
    let known = |id: &str| entry.data["options"].as_array().is_some_and(|o| o.iter().any(|o| o["optionId"] == id));
    if option_id.is_some_and(|id| !known(id)) {
        return external.state.locked().waiting.contains_key(entry_id) || is_ours(hub, entry_id);
    }
    let Some(waiting) = external.state.locked().waiting.remove(entry_id) else {
        return is_ours(hub, entry_id);
    };
    let decision = match option_id {
        Some(ALLOW) => Some(Decision::Allow),
        Some(DENY) => Some(Decision::Deny),
        Some(index) => index.parse().ok().map(Decision::Pick),
        None => None,
    };
    entry.data["status"] = if decision.is_some() { "answered" } else { "cancelled" }.into();
    entry.data["selected"] = option_id.into();
    hub.set_entry(entry_id, &entry.data);
    let bot = entry.data["author"].as_str().unwrap_or(&entry.bot_id).to_owned();
    hub.set_runtime(&bot, |r| {
        r.status = BotStatus::Working;
        r.activity = "Continuing…".into();
    });
    // The hook may already be gone (answered in the terminal): nobody is left to tell.
    let _ = waiting.send(decision);
    true
}

/// A card of a terminal session that is no longer waiting (answered, expired, or the hook went away): a late tap
/// is a no-op, as it is for a bot's own card.
fn is_ours(hub: &Hub, entry_id: &str) -> bool {
    hub.store
        .entry(entry_id)
        .and_then(|e| hub.store.bot(e.data["author"].as_str().unwrap_or(&e.bot_id)).ok().flatten())
        .is_some_and(|row| super::is_external(&row.config))
}
