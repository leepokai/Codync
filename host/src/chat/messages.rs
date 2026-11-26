//! The user's messages in a chat: typed (`send`) or relayed from a voice call (`chat::calls`).

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, EntryKind, Lane};
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::sync::Arc;

/// Posts `data` (the message's text and what it carries) as the user's newest message in `cfg`'s chat and hands it
/// on: to the bot's agent, or to a group's members.
pub fn post(hub: &Arc<Hub>, cfg: &BotConfig, data: Value) -> Result<Entry> {
    let e = save(hub, cfg, data)?;
    hand_on(hub, cfg, &e)?;
    Ok(e)
}

/// [`post`]'s first half, `data` saved as the user's newest message in `cfg`'s chat: for a caller to whom a message
/// that was never saved differs from one that was saved and not taken.
pub fn save(hub: &Hub, cfg: &BotConfig, mut data: Value) -> Result<Entry> {
    // A group has no agent of its own to queue behind: its message is simply sent.
    data["status"] = (if cfg.is_group() { "sent" } else { "queued" }).into();
    let turn = hub.store.max_turn(&cfg.id) + 1;
    let e = hub
        .add_entry(&Lane::main(&cfg.id), EntryKind::User, turn, &data)
        .ok_or_else(|| anyhow!("couldn't save the message"))?;
    // Before the turn starts, so its reply lands unread.
    if let Err(error) = hub.mark_read(&cfg.id) {
        tracing::warn!(%error, bot = %cfg.id, "couldn't mark bot read after send");
    }
    Ok(e)
}

/// [`post`]'s second half: the saved message `e` goes to the bot's agent, or to a group's members.
pub fn hand_on(hub: &Arc<Hub>, cfg: &BotConfig, e: &Entry) -> Result<()> {
    let lane = Lane::main(&cfg.id);
    if cfg.is_group() {
        // Each member reads it in its room prompt (`chat::group`).
        crate::chat::group::start(hub, cfg, lane);
    } else {
        // `/skill …` reaches the agent as "read this skill first"; the chat keeps what was typed.
        let prompt = crate::chat::prompt::of_entry(hub, cfg, e);
        hub.send_cmd(&cfg.id, Cmd::Send { lane, entry_id: e.id.clone(), text: prompt })?;
    }
    Ok(())
}

/// A message's new text (`updateScheduled`): what the user pasted stays marked only while it is still part of
/// the message.
pub fn set_text(data: &mut Value, text: &str) {
    data["text"] = text.into();
    let pasted: Vec<&str> =
        data["pasted"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let kept: Vec<Value> = pasted.into_iter().filter(|p| text.contains(*p)).map(Into::into).collect();
    if kept.is_empty() {
        if let Some(o) = data.as_object_mut() {
            o.remove("pasted");
        }
    } else {
        data["pasted"] = kept.into();
    }
}

/// A scheduled send (`scheduleSend`): the message is saved now, shown in the chat as scheduled, and delivered
/// later by the sweep (`chat::schedule`) through the ordinary [`post`] path. A group's sends are immediate, so
/// they can't be scheduled; the time must be at least half a minute out.
pub fn save_scheduled(hub: &Hub, cfg: &BotConfig, mut data: Value, not_before_ms: i64) -> Result<Entry> {
    if cfg.is_group() {
        return Err(anyhow!("a group's messages send immediately; there is nothing to schedule"));
    }
    if not_before_ms <= crate::store::now_ms() + SOONEST {
        return Err(anyhow!("the scheduled time must be at least {} seconds out", SOONEST / 1_000));
    }
    data["status"] = "scheduled".into();
    data["notBefore"] = not_before_ms.into();
    let turn = hub.store.max_turn(&cfg.id) + 1;
    let e = hub
        .add_entry(&Lane::main(&cfg.id), EntryKind::User, turn, &data)
        .ok_or_else(|| anyhow!("couldn't save the message"))?;
    if let Err(error) = hub.mark_read(&cfg.id) {
        tracing::warn!(%error, bot = %cfg.id, "couldn't mark bot read after scheduling a send");
    }
    Ok(e)
}

/// How far out a fire time must at least be: anything closer is a send, not a schedule.
pub const SOONEST: i64 = 30_000;

/// A scheduled message's time has come (`chat::schedule`): it takes its delivery-time place in the chat and
/// enters the ordinary queue exactly as a message typed now would. Returns the queued entry, or `None` when
/// something else (an edit, a cancel) moved it first.
pub fn fire_scheduled(hub: &Arc<Hub>, e: &Entry) -> Result<Option<Entry>> {
    let row = hub.store.bot(&e.bot_id)?.ok_or_else(|| anyhow!("unknown bot"))?;
    let mut data = e.data.clone();
    data["status"] = "queued".into();
    // The durable outbox mark: if this host stops between the flip and `hand_on`, the next start hands this
    // message on exactly once from `dispatch_pending` instead of expiring it with ordinary live-queue sends.
    data["scheduledFired"] = true.into();
    // Remove the scheduling fields: the message is an ordinary queued send from here on.
    if let Some(o) = data.as_object_mut() {
        o.remove("notBefore");
        o.remove("scheduledAt");
    }
    let turn = hub.store.max_turn(&e.bot_id) + 1;
    // One-way at the rev we read: a cancel, an edit — even one that kept it scheduled — that landed since
    // our snapshot moved the rev, so this refuses and the sweep re-reads and fires what was actually said.
    let Some(turned) = hub.flip_scheduled(&e.id, Some(turn), &data, e.rev)? else {
        return Ok(None);
    };
    if let Err(error) = hand_on(hub, &row.config, &turned) {
        // Back on its time rather than lost — the sweep tries again — but only while the queued row is
        // still the one we flipped: any write that landed in the failure window — an edit through
        // `Cmd::Hold`, or an unrelated one such as a reaction — moved the rev, so what it wrote stays
        // (a rev-blind rollback would stomp it) and the entry waits where that write left it until a
        // host restart expires it. A rollback that fails outright is said, not swallowed.
        if let Err(rollback) = hub.edit_entry(&e.id, |cur| {
            Ok(cur.rev == turned.rev && cur.data["status"] == "queued" && {
                cur.data = e.data.clone();
                true
            })
        }) {
            tracing::warn!(error = format!("{rollback:#}"), entry = %e.id, "couldn't put a fired message back on its time");
        }
        return Err(error);
    }
    Ok(Some(turned))
}
