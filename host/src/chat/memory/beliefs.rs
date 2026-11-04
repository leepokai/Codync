//! Memory as beliefs ("What I know about you"): each note says what kind of thing it holds, how sure the bot is and when the
//! person last confirmed it (`codync_mem::files::belief`). The person confirms, forgets or brings back a note from the
//! apps; the bot does the first two and a correction with its own tools. Nothing is ever lost for good: forgetting is a
//! new version (a deletion) and the history keeps what was there, so a restore or an undo brings it back.
//!
//! When the bot changes what it believes, the chat says so in one line with an Undo (`announce`), and a question about a
//! contradiction is asked at most once in 24 hours (idea from Mercury Agent's `learning/announce.ts`, MIT).

use super::files::{self, Writer};
use crate::hub::Hub;
use crate::store::{BotConfig, EntryKind, Lane};
use anyhow::{Result, anyhow, bail};
use codync_mem::files::belief::{self, Belief, Confidence, Kind};
use codync_mem::files::{self as mf, Author, Cause, MAIN};
use serde_json::{Value, json};
use std::sync::Arc;

/// A contradiction is asked about at most this often, per bot.
const QUESTION_EVERY_MS: i64 = 24 * 60 * 60 * 1000;
const VERSIONS_SHOWN: usize = 20;

pub async fn call(hub: &Arc<Hub>, method: &str, b: &Value) -> Result<Value> {
    let (hub, method, b) = (hub.clone(), method.to_owned(), b.clone());
    tokio::task::spawn_blocking(move || dispatch(&hub, &method, &b)).await?
}

fn bot_of(hub: &Hub, b: &Value) -> Result<BotConfig> {
    let id = b["botId"].as_str().ok_or_else(|| anyhow!("botId required"))?;
    Ok(hub.store.bot(id)?.filter(|r| !r.deleted && !r.config.is_group()).ok_or_else(|| anyhow!("unknown bot"))?.config)
}

fn dispatch(hub: &Hub, method: &str, b: &Value) -> Result<Value> {
    let cfg = bot_of(hub, b)?;
    match method {
        "beliefs" => list(hub, &cfg, b),
        "beliefAction" => act(hub, &cfg, b),
        "undoMemoryChange" => undo(hub, &cfg, b),
        _ => bail!("unknown method: {method}"),
    }
}

fn layer_of(cfg: &BotConfig, b: &Value) -> Result<Option<String>> {
    if b["shared"] != true {
        return Ok(Some(cfg.id.clone()));
    }
    if !super::shared(cfg) {
        bail!("This bot doesn't use the shared memory.");
    }
    Ok(None)
}

/// Every note of the bot's layers with its belief header, filtered by `kind` and `confidence` when asked. With
/// `forgotten`, the notes that were forgotten instead (their last content gives the header), so they can be brought back.
fn list(hub: &Hub, cfg: &BotConfig, b: &Value) -> Result<Value> {
    files::scan(hub, cfg);
    let want_kind = b["kind"].as_str().and_then(Kind::parse);
    let want_confidence = b["confidence"].as_str().and_then(Confidence::parse);
    let forgotten = b["forgotten"] == true;
    let mut beliefs = Vec::new();
    for layer in files::layers(cfg) {
        let notes = hub.store.memory_read(|c| mf::layer(c, layer.as_deref()))?;
        for note in notes.into_iter().filter(|n| n.name != MAIN && n.deleted == forgotten) {
            let versions = hub.store.memory_read(|c| mf::versions(c, layer.as_deref(), &note.name, VERSIONS_SHOWN))?;
            let content = if forgotten {
                versions.iter().find_map(|v| v.content.clone()).unwrap_or_default()
            } else {
                files::read(layer.as_deref(), &note.name)?.unwrap_or_default()
            };
            let belief = belief::read(&content);
            if want_kind.is_some_and(|k| k != belief.kind) || want_confidence.is_some_and(|c| c != belief.confidence) {
                continue;
            }
            beliefs.push(json!({
                "file": note.name, "shared": layer.is_none(), "description": note.description,
                "kind": belief.kind, "confidence": belief.confidence, "confirmed": belief.confirmed,
                "updatedAt": note.updated_at, "tokens": note.tokens, "versions": versions,
            }));
        }
    }
    Ok(json!({"beliefs": beliefs}))
}

fn today() -> String {
    super::ymd(crate::store::now_ms())
}

/// Writes `content` (or deletes) over a note that is `expected` as the person or the bot, and says what version it made.
fn write(hub: &Hub, cfg: &BotConfig, layer: Option<&str>, name: &str, content: Option<&str>, by: By, cause: Cause) -> Result<mf::Version> {
    let before = files::read(layer, name)?;
    let expected = before.as_deref().map(mf::hash);
    let message = hub.runtime(&cfg.id).message;
    let writer = match by {
        By::Person => Writer { author: Author::User, bot: None, cause, message: None },
        By::Bot => Writer { author: Author::Bot, bot: Some(cfg), cause, message: message.as_deref() },
    };
    files::write(hub, layer, name, content, expected.as_deref(), writer)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum By {
    Person,
    Bot,
}

/// The note as confirmed now: high confidence, today's date, the rest of its header kept.
pub fn confirmed(content: &str) -> String {
    let current = belief::read(content);
    belief::stamp(content, &Belief { confidence: Confidence::High, confirmed: Some(today()), ..current })
}

fn act(hub: &Hub, cfg: &BotConfig, b: &Value) -> Result<Value> {
    let layer = layer_of(cfg, b)?;
    let name = b["file"].as_str().ok_or_else(|| anyhow!("file required"))?;
    files::scan(hub, cfg);
    let version = match b["action"].as_str() {
        Some("confirm") => {
            let content = files::read(layer.as_deref(), name)?.ok_or_else(|| anyhow!("There's no note {name}."))?;
            write(hub, cfg, layer.as_deref(), name, Some(&confirmed(&content)), By::Person, Cause::Edit)?
        }
        Some("forget") => {
            files::read(layer.as_deref(), name)?.ok_or_else(|| anyhow!("There's no note {name}."))?;
            write(hub, cfg, layer.as_deref(), name, None, By::Person, Cause::Delete)?
        }
        Some("restore") => {
            let id = b["versionId"].as_i64().ok_or_else(|| anyhow!("versionId required"))?;
            let old = hub.store.memory_read(|c| mf::version(c, id))?.ok_or_else(|| anyhow!("unknown version"))?;
            if old.name != name || old.bot.as_deref() != layer.as_deref() {
                bail!("That version belongs to another note.");
            }
            let content = old.content.ok_or_else(|| anyhow!("That version is the note being forgotten; pick an earlier one."))?;
            write(hub, cfg, layer.as_deref(), name, Some(&content), By::Person, Cause::Restore)?
        }
        _ => bail!("action must be confirm, forget or restore"),
    };
    Ok(json!({"version": version.id}))
}

/// What the bot's tools do: confirm a note, correct it, or forget it. Each is a change of the person's memory that the
/// bot makes only in a turn the person started (the rule every bot write passes).
pub fn tool(hub: &Hub, cfg: &BotConfig, name: &str, args: &Value) -> Result<Value> {
    let layer = layer_of(cfg, args)?;
    let file = args["file"].as_str().ok_or_else(|| anyhow!("file required"))?;
    files::scan(hub, cfg);
    let before = files::read(layer.as_deref(), file)?;
    let earlier = hub.store.memory_read(|c| Ok(mf::get(c, layer.as_deref(), file)?.map(|f| f.version)))?;
    match name {
        "confirm_memory" => {
            let content = before.ok_or_else(|| anyhow!("There's no note {file}."))?;
            write(hub, cfg, layer.as_deref(), file, Some(&confirmed(&content)), By::Bot, Cause::Edit)?;
            Ok(json!({"status": "confirmed", "file": file}))
        }
        "correct_memory" => {
            let content = args["content"].as_str().ok_or_else(|| anyhow!("content (the whole corrected note) required"))?;
            let kept = before.as_deref().map(belief::read).unwrap_or_default();
            let fixed = belief::stamp(content, &Belief { confidence: Confidence::High, confirmed: Some(today()), ..kept });
            let version = write(hub, cfg, layer.as_deref(), file, Some(&fixed), By::Bot, Cause::Edit)?;
            let summary = args["summary"].as_str().unwrap_or("something about you changed");
            announce(hub, cfg, summary, file, layer.is_none(), earlier, version.id);
            let question = args["question"].as_str().map(|q| ask(hub, cfg, q)).transpose()?;
            Ok(json!({"status": "corrected", "file": file, "question": question.map(|asked| if asked { "asked" } else { "skipped: a question was asked in the last 24 hours" })}))
        }
        "forget_memory" => {
            before.ok_or_else(|| anyhow!("There's no note {file}."))?;
            let version = write(hub, cfg, layer.as_deref(), file, None, By::Bot, Cause::Delete)?;
            let summary = args["summary"].as_str().unwrap_or("you asked me to forget something");
            announce(hub, cfg, summary, file, layer.is_none(), earlier, version.id);
            Ok(json!({"status": "forgotten", "file": file}))
        }
        _ => bail!("unknown memory tool: {name}"),
    }
}

/// "I learned …" in the chat, with what Undo needs: the version the note had before this change (none: it was new).
fn announce(hub: &Hub, cfg: &BotConfig, summary: &str, file: &str, shared: bool, before: Option<i64>, change: i64) {
    let text = format!("I learned: {}", crate::agent::acp::truncate(summary.trim(), 200));
    let data = json!({"text": text, "style": "info",
        "memoryUndo": {"file": file, "shared": shared, "before": before, "change": change, "undone": false}});
    hub.add_entry(&Lane::main(&cfg.id), EntryKind::Notice, hub.store.max_turn(&cfg.id), &data);
}

/// Asks the person about a contradiction, unless one was asked in the last 24 hours. False when it was held back.
fn ask(hub: &Hub, cfg: &BotConfig, question: &str) -> Result<bool> {
    let key = format!("memory.contradiction.{}", cfg.id);
    let now = crate::store::now_ms();
    if !may_ask(hub.store.kv_read(&key)?.and_then(|s| s.parse().ok()), now) {
        return Ok(false);
    }
    hub.store.kv_set(&key, &now.to_string())?;
    let data = json!({"text": crate::agent::acp::truncate(question.trim(), 400), "style": "info"});
    hub.add_entry(&Lane::main(&cfg.id), EntryKind::Notice, hub.store.max_turn(&cfg.id), &data);
    Ok(true)
}

/// Whether a question may be asked now, given when the last one was.
pub fn may_ask(last: Option<i64>, now: i64) -> bool {
    last.is_none_or(|at| now - at >= QUESTION_EVERY_MS)
}

/// Takes back the change a notice told about: the version before it comes back, or a note that was new goes.
fn undo(hub: &Hub, cfg: &BotConfig, b: &Value) -> Result<Value> {
    let id = b["entryId"].as_str().ok_or_else(|| anyhow!("entryId required"))?;
    let entry = hub.store.entry(id).filter(|e| e.bot_id == cfg.id).ok_or_else(|| anyhow!("unknown message"))?;
    let undo = entry.data["memoryUndo"].clone();
    if !undo.is_object() {
        bail!("That message has nothing to undo.");
    }
    if undo["undone"] == true {
        return Ok(json!({"entry": entry}));
    }
    let file = undo["file"].as_str().ok_or_else(|| anyhow!("bad undo"))?;
    let layer = layer_of(cfg, &json!({"shared": undo["shared"]}))?;
    files::scan(hub, cfg);
    match undo["before"].as_i64() {
        Some(version) => {
            let old = hub.store.memory_read(|c| mf::version(c, version))?.ok_or_else(|| anyhow!("The earlier version is gone."))?;
            let content = old.content.ok_or_else(|| anyhow!("The earlier version was a deletion."))?;
            write(hub, cfg, layer.as_deref(), file, Some(&content), By::Person, Cause::Undo)?;
        }
        None => {
            if files::read(layer.as_deref(), file)?.is_some() {
                write(hub, cfg, layer.as_deref(), file, None, By::Person, Cause::Undo)?;
            }
        }
    }
    let mut data = entry.data.clone();
    data["memoryUndo"]["undone"] = true.into();
    let entry = hub.set_entry(id, &data).ok_or_else(|| anyhow!("could not update the message"))?;
    Ok(json!({"entry": entry}))
}

pub fn definitions() -> Vec<Value> {
    let file = json!({"type": "string", "description": "A note's name like people.md or health/allergies.md."});
    let shared = json!({"type": "boolean", "description": "True for the memory all the user's bots share."});
    vec![
        json!({
            "name": "confirm_memory",
            "description": "The user just said a note of yours still holds: record that you are sure of it (high confidence, confirmed today).",
            "inputSchema": {"type": "object", "properties": {"file": file, "shared": shared}, "required": ["file"]},
        }),
        json!({
            "name": "correct_memory",
            "description": "The user corrected something you believed. Give the whole corrected note (frontmatter first, with its description:). The change is shown to the user as one line starting \"I learned\" with an Undo; summary is that line. If the correction contradicts something else you believe, give question: one short question to settle it (asked at most once a day). Works only in a turn the user started in your own chat.",
            "inputSchema": {"type": "object", "properties": {
                "file": file, "shared": shared,
                "content": {"type": "string", "description": "The whole corrected note."},
                "summary": {"type": "string", "description": "What you learned, in a few words, like \"you now live in Porto\"."},
                "question": {"type": "string", "description": "Optional: a question about a contradiction."},
            }, "required": ["file", "content"]},
        }),
        json!({
            "name": "forget_memory",
            "description": "The user asked you to forget a note. It is taken out of your memory, not destroyed: its history keeps it and the user can bring it back. Shown to the user as one line starting \"I learned\" with an Undo. Works only in a turn the user started in your own chat.",
            "inputSchema": {"type": "object", "properties": {"file": file, "shared": shared, "summary": {"type": "string"}}, "required": ["file"]},
        }),
    ]
}

#[cfg(test)]
mod tests;
