//! Proposed skills: when a turn or a routine run ends well after using three or more different kinds of tool, the bot's own
//! agent drafts a skill from what it did (name, when to use it, steps) and the draft waits in an inbox. A proposal is **never**
//! a skill until the person approves it; approving saves it like any skill the bot writes (`chat::skills`), editing changes
//! the draft, dismissing remembers its fingerprint so the same procedure is not proposed again.
//!
//! Cost is held down: no draft while the bot's token budget has reached its warning level, at most one draft per bot per
//! hour, and none for a procedure already waiting or dismissed. Idea from Mercury Agent's `bots/skill-synthesis.ts` (MIT).

use crate::hub::Hub;
use crate::store::{BotConfig, Store};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Different kinds of tool a procedure needs before it is worth a skill.
const MIN_TOOLS: usize = 3;
const MAX_PENDING: usize = 20;
const MAX_DISMISSED: usize = 200;
/// One draft per bot per this long.
const DRAFT_EVERY_MS: i64 = 60 * 60 * 1000;
const MAX_NAME: usize = 64;
const MAX_DESCRIPTION: usize = 1024;
const MAX_INSTRUCTIONS: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: String,
    pub bot_id: String,
    pub name: String,
    /// When to use it.
    pub description: String,
    /// The skill in Markdown.
    pub instructions: String,
    pub fingerprint: String,
    /// The kinds of tool it used.
    pub tools: Vec<String>,
    pub created_at: i64,
}

// MARK: the procedure

/// The kinds of tool a turn used: the ACP kind, and for commands the program (`execute:cargo`), so reading three files is one
/// kind of tool and `cargo`, `git` and an edit are three.
pub fn tool_keys(rows: &[(String, String)]) -> BTreeSet<String> {
    rows.iter()
        .map(|(kind, title)| {
            let kind = kind.trim().to_ascii_lowercase();
            if kind == "execute" {
                let program = title.trim().trim_start_matches("Run ").split_whitespace().next().unwrap_or_default();
                format!("execute:{}", program.to_ascii_lowercase())
            } else {
                kind
            }
        })
        .filter(|k| !k.is_empty() && k != "think" && k != "other")
        .collect()
}

pub fn worth_a_skill(tools: &BTreeSet<String>) -> bool {
    tools.len() >= MIN_TOOLS
}

/// The same procedure for the same kind of request gives the same fingerprint, whatever the order of the steps.
pub fn fingerprint(tools: &BTreeSet<String>, request: &str) -> String {
    let words: Vec<String> = request.split_whitespace().take(8).map(str::to_lowercase).collect();
    let mut digest = Sha256::new();
    digest.update(tools.iter().cloned().collect::<Vec<_>>().join(",").as_bytes());
    digest.update(b"|");
    digest.update(words.join(" ").as_bytes());
    crate::chat::files::digest_hex(&digest.finalize()).chars().take(16).collect()
}

// MARK: storage (a small list per bot in the key-value store)

fn pending_key(bot: &str) -> String {
    format!("skill-proposals.pending.{bot}")
}

fn dismissed_key(bot: &str) -> String {
    format!("skill-proposals.dismissed.{bot}")
}

pub fn pending(store: &Store, bot: &str) -> Vec<Proposal> {
    store.kv_read(&pending_key(bot)).ok().flatten().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_pending(store: &Store, bot: &str, list: &[Proposal]) -> Result<()> {
    store.kv_set(&pending_key(bot), &serde_json::to_string(list)?)
}

fn dismissed(store: &Store, bot: &str) -> Vec<String> {
    store.kv_read(&dismissed_key(bot)).ok().flatten().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn remember_dismissed(store: &Store, bot: &str, fingerprint: &str) -> Result<()> {
    let mut list = dismissed(store, bot);
    if !list.iter().any(|f| f == fingerprint) {
        list.push(fingerprint.to_owned());
    }
    if list.len() > MAX_DISMISSED {
        list.drain(..list.len() - MAX_DISMISSED);
    }
    store.kv_set(&dismissed_key(bot), &serde_json::to_string(&list)?)
}

// MARK: drafting

const SYSTEM: &str = "You turn what an assistant just did into a reusable skill. You are given the person's request and \
the kinds of steps the assistant took. Reply with one JSON object and nothing else: {\"name\": a short name, \"when\": one \
or two sentences saying when to use the skill, \"steps\": a list of short imperative steps}. Write steps anyone could follow \
again; never include secrets, keys, personal data or the specifics of one file.";

/// A draft from the model's reply: the first JSON object in it, its fields cut to the limits. `None` when it is not one.
pub fn parse_draft(reply: &str) -> Option<(String, String, String)> {
    let (start, end) = (reply.find('{')?, reply.rfind('}')?);
    let value: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let name = value["name"].as_str()?.trim().chars().take(MAX_NAME).collect::<String>();
    let when = value["when"].as_str()?.trim().chars().take(MAX_DESCRIPTION).collect::<String>();
    let steps: Vec<String> = value["steps"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .take(30)
        .collect();
    if name.is_empty() || when.is_empty() || steps.is_empty() {
        return None;
    }
    let numbered: Vec<String> = steps.iter().enumerate().map(|(i, s)| format!("{}. {s}", i + 1)).collect();
    let instructions =
        format!("Use this when: {when}\n\n{}\n", numbered.join("\n")).chars().take(MAX_INSTRUCTIONS).collect();
    Some((name, when, instructions))
}

/// After a turn that ended well: drafts a proposal if its procedure is worth one and nothing says not to. Failures are
/// logged and change nothing; this never runs in the way of the turn.
pub async fn consider(hub: Arc<Hub>, cfg: BotConfig, turn: i64, request: String) {
    if let Err(error) = try_consider(&hub, &cfg, turn, &request).await {
        tracing::warn!(error = format!("{error:#}"), bot = cfg.id, "a skill could not be proposed");
    }
}

async fn try_consider(hub: &Arc<Hub>, cfg: &BotConfig, turn: i64, request: &str) -> Result<()> {
    let now = crate::store::now_ms();
    let rows = hub.store.tools_in_turn(&cfg.id, turn)?;
    let tools = tool_keys(&rows);
    if !worth_a_skill(&tools) || cfg.is_group() || crate::external::is_external(cfg) {
        return Ok(());
    }
    // A bot short of budget spends nothing on drafts.
    if hub.budget.saving(&cfg.id, now) {
        return Ok(());
    }
    let print = fingerprint(&tools, request);
    let waiting = pending(&hub.store, &cfg.id);
    if waiting.len() >= MAX_PENDING
        || waiting.iter().any(|p| p.fingerprint == print)
        || dismissed(&hub.store, &cfg.id).contains(&print)
    {
        return Ok(());
    }
    let last_key = format!("skill-proposals.last.{}", cfg.id);
    if hub.store.kv_read(&last_key)?.and_then(|s| s.parse::<i64>().ok()).is_some_and(|at| now - at < DRAFT_EVERY_MS) {
        return Ok(());
    }
    hub.store.kv_set(&last_key, &now.to_string())?;
    let steps: Vec<String> =
        rows.iter().map(|(kind, title)| format!("- {kind}: {}", crate::agent::acp::truncate(title, 120))).collect();
    let user = format!("Request: {}\n\nSteps taken:\n{}", crate::agent::acp::truncate(request, 600), steps.join("\n"));
    let reply = super::oneshot::one_shot(hub, cfg, SYSTEM, &user).await?;
    let Some((name, description, instructions)) = parse_draft(&reply) else { return Ok(()) };
    let mut list = pending(&hub.store, &cfg.id);
    list.push(Proposal {
        id: uuid::Uuid::new_v4().to_string(),
        bot_id: cfg.id.clone(),
        name,
        description,
        instructions,
        fingerprint: print,
        tools: tools.into_iter().collect(),
        created_at: now,
    });
    save_pending(&hub.store, &cfg.id, &list)
}

// MARK: the inbox's API

pub async fn call(hub: &Arc<Hub>, method: &str, b: &Value) -> Result<Value> {
    let bot = b["botId"].as_str().ok_or_else(|| anyhow!("botId required"))?.to_owned();
    hub.store.bot(&bot)?.filter(|r| !r.deleted && !r.config.is_group()).ok_or_else(|| anyhow!("unknown bot"))?;
    let id = b["id"].as_str().unwrap_or_default().to_owned();
    match method {
        "skillProposals" => {}
        "editSkillProposal" => edit(&hub.store, &bot, &id, b)?,
        "dismissSkillProposal" => dismiss(&hub.store, &bot, &id)?,
        "approveSkillProposal" => approve(hub, &bot, &id).await?,
        _ => bail!("unknown method: {method}"),
    }
    Ok(json!({"proposals": pending(&hub.store, &bot)}))
}

fn take(list: &mut Vec<Proposal>, id: &str) -> Result<Proposal> {
    let at = list.iter().position(|p| p.id == id).ok_or_else(|| anyhow!("That proposal is gone."))?;
    Ok(list.remove(at))
}

fn edit(store: &Store, bot: &str, id: &str, b: &Value) -> Result<()> {
    let mut list = pending(store, bot);
    let proposal = list.iter_mut().find(|p| p.id == id).ok_or_else(|| anyhow!("That proposal is gone."))?;
    let text = |key: &str, max: usize| {
        b[key].as_str().map(|s| s.trim().chars().take(max).collect::<String>()).filter(|s| !s.is_empty())
    };
    if let Some(name) = text("name", MAX_NAME) {
        proposal.name = name;
    }
    if let Some(description) = text("description", MAX_DESCRIPTION) {
        proposal.description = description;
    }
    if let Some(instructions) = text("instructions", MAX_INSTRUCTIONS) {
        proposal.instructions = instructions;
    }
    save_pending(store, bot, &list)
}

fn dismiss(store: &Store, bot: &str, id: &str) -> Result<()> {
    let mut list = pending(store, bot);
    let gone = take(&mut list, id)?;
    remember_dismissed(store, bot, &gone.fingerprint)?;
    save_pending(store, bot, &list)
}

/// Saves the proposal as a skill of the bot (on for that bot only, with a notice in its chat) and takes it from the inbox.
async fn approve(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    let mut list = pending(&hub.store, bot);
    let proposal = take(&mut list, id)?;
    super::skills::call(
        hub,
        bot,
        &json!({"name": proposal.name, "description": proposal.description, "instructions": proposal.instructions}),
    )
    .await?;
    save_pending(&hub.store, bot, &list)
}

#[cfg(test)]
mod tests;
