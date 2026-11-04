//! Bot and chat methods: roster, messages, uploads, memory, artifacts, routines and the bots' own MCP calls.

use super::devices::Caller;
use super::str_arg;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotKind, EntryKind, Lane};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    if let Some(answer) = crate::routines::flow::api_call(hub, method, &b)? {
        return Ok(ControlFlow::Break(answer));
    }
    Ok(ControlFlow::Break(match method {
        "routineSchedule" => crate::routines::schedule_preview(&b, crate::store::now_ms())?,
        "routines" => hub.routines.list(str_arg(&b, "botId")?),
        "saveRoutine" => hub.routines.save(hub, str_arg(&b, "botId")?, &b)?,
        "setRoutineEnabled" => hub.routines.set_enabled(
            &hub.store,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            b["enabled"].as_bool().context("enabled is required")?,
        )?,
        "deleteRoutine" => hub.routines.remove(hub, str_arg(&b, "botId")?, str_arg(&b, "id")?)?,
        "runRoutine" => hub.routines.enqueue(
            &hub.store,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            json!({"source":"test"}),
            None,
            true,
        )?,
        "retryRoutineRun" => hub.routines.retry(hub, str_arg(&b, "botId")?, str_arg(&b, "runId")?)?,
        "routineWebhook" => hub.routines.credentials(
            hub,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            b["rotate"].as_bool().unwrap_or(false),
        )?,
        "routineCall" => crate::routines::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"])?,
        "memoryCall" => {
            crate::chat::memory::tool_call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "listBotApps" | "createAppLink" | "revokeAppLink" | "deleteBotApp" => {
            crate::chat::apps::api_call(hub, method, &b)?
        }
        "appRequest" => super::apps_bridge::request(hub, &b).await?,
        "listOpenItems" | "dismissOpenItem" | "listBookmarks" | "saveBookmark" | "deleteBookmark" => {
            crate::chat::open_items::api_call(hub, method, &b)?
        }
        "chatCall" => {
            let (bot, name) = (str_arg(&b, "botId")?, str_arg(&b, "name")?);
            if name == "research_sources" {
                crate::research::sources(hub, bot, str_arg(&b["arguments"], "objective")?, b["arguments"].get("urls"))
                    .await?
            } else if crate::documents::TOOLS.contains(&name) {
                crate::documents::call(hub, bot, name, &b["arguments"]).await?
            } else if crate::chat::profile::TOOLS.contains(&name) {
                crate::chat::profile::call(hub, bot, name, &b["arguments"]).await?
            } else if crate::chat::scratchpad::TOOLS.contains(&name) {
                crate::chat::scratchpad::call(hub, bot, name, &b["arguments"])?
            } else if crate::chat::teach::TOOLS.contains(&name) {
                crate::chat::teach::call(hub, bot, name, &b["arguments"])?
            } else if crate::chat::apps::TOOLS.contains(&name) {
                crate::chat::apps::call(hub, bot, name, &b["arguments"])?
            } else if crate::chat::open_items::TOOLS.contains(&name) {
                crate::chat::open_items::call(hub, bot, name, &b["arguments"])?
            } else if name == crate::chat::skills::TOOL {
                crate::chat::skills::call(hub, bot, &b["arguments"]).await?
            } else if crate::chat::fallbacks::TOOLS.contains(&name) {
                crate::chat::fallbacks::call(hub, bot, name, &b["arguments"]).await?
            } else if crate::chat::artifacts::TOOLS.contains(&name) {
                crate::chat::artifacts::call(hub, bot, name, &b["arguments"]).await?
            } else {
                crate::chat::outbox::call(hub, bot, name, &b["arguments"]).await?
            }
        }
        // An agent on this computer that isn't a bot (`codync-host mcp network`).
        "agentCall" => {
            crate::chat::team::assistant::local_call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"])
                .await?
        }
        "teamCall" => {
            crate::chat::team::call_async(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "messageResult" => {
            let bot = str_arg(&b, "botId")?;
            hub.store.bot(bot)?.filter(|row| !row.deleted).context("Unknown bot.")?;
            let entry = hub
                .store
                .entry(str_arg(&b, "id")?)
                .filter(|entry| {
                    entry.bot_id == bot
                        && entry.kind == EntryKind::User.as_str()
                        && !crate::store::is_internal(&entry.data)
                })
                .context("Unknown user message.")?;
            json!({"id":entry.id,"botId":entry.bot_id,"threadId":entry.thread_id,"status":entry.data["status"],"result":entry.data["turnResult"]})
        }
        "manageCall" => {
            crate::chat::manage::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "history" => {
            let bot = str_arg(&b, "botId")?;
            let before = b["beforeSeq"].as_i64().unwrap_or(i64::MAX);
            let limit = b["limit"].as_i64().unwrap_or(100).clamp(1, 500);
            json!({"entries": hub.store.history(bot, before, limit)?})
        }
        // A side conversation (a document's `doc:<id>`, a Slack thread's `slack-…`); the chat itself has none.
        "thread" => {
            let root = str_arg(&b, "rootId")?;
            if !(root.starts_with("doc:") || root.starts_with("slack-")) {
                bail!("threads are gone: replies are in the chat");
            }
            json!({"entries": hub.store.thread(str_arg(&b, "botId")?, root, 500)?})
        }
        // Notices between a bot and one peer (the newest 200), for the read-only bot conversation sheet.
        "botConversation" => {
            json!({"entries": hub.store.bot_conversation(str_arg(&b, "botId")?, str_arg(&b, "peerId")?, 200)?})
        }
        "createBot" => {
            let mut b = b;
            // Only a bot's `create_bot` records who made one.
            b["createdBy"] = Value::Null;
            let cfg = crate::agent::new_bot::with_defaults(hub, b).await?;
            let hub = hub.clone();
            json!({"bot": tokio::task::spawn_blocking(move || hub.create_bot(cfg)).await??})
        }
        "updateBot" => {
            // Edits through the API are the user's; only the bot's own tool says otherwise.
            let mut b = b;
            if let Some(patch) = b.as_object_mut() {
                patch.remove(crate::chat::instruction_history::AUTHOR);
            }
            let hub = hub.clone();
            json!({"bot": tokio::task::spawn_blocking(move || hub.update_bot(&b)).await??})
        }
        // The versions of a bot's instructions, what changed between two, and going back to one.
        "instructionHistory" => {
            let row = hub.store.bot(str_arg(&b, "botId")?)?.context("unknown bot")?;
            crate::chat::instruction_history::list(&hub.store, &row.config)?
        }
        "instructionDiff" => {
            let version =
                |key: &str| b[key].as_u64().and_then(|v| u32::try_from(v).ok()).context("from and to are versions");
            json!({"diff": crate::chat::instruction_history::diff(&hub.store, str_arg(&b, "botId")?, version("from")?, version("to")?)?})
        }
        "restoreInstructions" => {
            use crate::chat::instruction_history as history;
            let bot = str_arg(&b, "botId")?.to_owned();
            let version = b["version"].as_u64().and_then(|v| u32::try_from(v).ok()).context("version is required")?;
            let patch = json!({"id": bot, "description": history::text(&hub.store, &bot, version)?,
                history::REASON: format!("Restored version {version}")});
            let hub = hub.clone();
            let saved = tokio::task::spawn_blocking(move || {
                let saved = hub.update_bot(&patch)?;
                let notice = json!({"text": format!("Instructions restored to version {version}")});
                hub.add_entry(&Lane::main(&bot), EntryKind::Notice, hub.store.max_turn(&bot), &notice);
                anyhow::Ok(saved)
            })
            .await??;
            json!({"bot": saved})
        }
        // The roster as clients build bot menus from the API (the n8n node): the agents,
        // not the groups; hidden bots stay out, as in the roster.
        "botList" => {
            let bots = hub
                .store
                .bots()?
                .into_iter()
                .filter(|row| !row.deleted && !row.config.hidden && row.config.kind != BotKind::Group)
                .map(|row| {
                    json!({
                        "id": row.config.id,
                        "name": row.config.name,
                        "description": row.config.description,
                        "external": crate::external::is_external(&row.config),
                    })
                })
                .collect::<Vec<_>>();
            json!({"bots": bots})
        }
        // The roster as the user dragged it, top first.
        "reorderBots" => {
            let ids: Vec<String> = serde_json::from_value(b["ids"].clone()).context("ids is required")?;
            let hub = hub.clone();
            tokio::task::spawn_blocking(move || hub.reorder_bots(&ids)).await??;
            json!({})
        }
        "deleteBot" => {
            hub.delete_bot(str_arg(&b, "botId")?)?;
            json!({})
        }
        "markRead" => {
            hub.mark_read(str_arg(&b, "botId")?)?;
            json!({})
        }
        // One chunk of a file for a later `send` (`attachments`); base64 `data` at `offset`.
        "upload" => {
            use base64::Engine as _;
            let row = hub.store.bot(str_arg(&b, "botId")?)?.filter(|r| !r.deleted);
            let root = crate::chat::uploads::root(&row.ok_or_else(|| anyhow!("unknown bot"))?.config);
            let (id, name) = (str_arg(&b, "uploadId")?.to_owned(), str_arg(&b, "name")?.to_owned());
            let data =
                base64::engine::general_purpose::STANDARD.decode(str_arg(&b, "data")?).context("invalid data")?;
            let (offset, done) = (b["offset"].as_u64().unwrap_or(0), b["done"] == true);
            let attachment = tokio::task::spawn_blocking(move || {
                crate::chat::uploads::append(&root, &id, &name, offset, &data, done)
            })
            .await??;
            json!({"attachment": attachment})
        }
        // A sent file, back in chunks (chat previews on other devices).
        "readUpload" => {
            use base64::Engine as _;
            let row = hub.store.bot(str_arg(&b, "botId")?)?.filter(|r| !r.deleted);
            let root = crate::chat::uploads::root(&row.ok_or_else(|| anyhow!("unknown bot"))?.config);
            let id = str_arg(&b, "uploadId")?.to_owned();
            let offset = b["offset"].as_u64().unwrap_or(0);
            let (data, size) =
                tokio::task::spawn_blocking(move || crate::chat::uploads::read(&root, &id, offset, 384 * 1024))
                    .await??;
            json!({"data": base64::engine::general_purpose::STANDARD.encode(data), "size": size})
        }
        "send" => {
            let bot = str_arg(&b, "botId")?;
            let text = b["text"].as_str().unwrap_or_default().trim();
            let uploads: Vec<&str> =
                b["attachments"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if text.is_empty() && uploads.is_empty() {
                bail!("empty message");
            }
            let nonce = b["clientNonce"].as_str().unwrap_or_default();
            if !nonce.is_empty()
                && let Some(e) = hub.store.find_by_nonce(bot, nonce)
            {
                return Ok(ControlFlow::Break(json!({"entry": e})));
            }
            let row = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            if crate::external::is_external(&row.config) {
                bail!("this bot runs in your terminal: reply there, or use its approval cards here");
            }
            // `threadId`: a document's side conversation (`doc:<id>`); the chat itself has no threads.
            let side = match b["threadId"].as_str().filter(|t| !t.is_empty()) {
                Some(root) => {
                    let id = root.strip_prefix("doc:").context("threads are gone: reply to a message instead")?;
                    hub.store.document(id)?.filter(|doc| doc.bot_id == bot).context("unknown document conversation")?;
                    Some(root.to_owned())
                }
                None => None,
            };
            // `replyTo`: the message of this chat it answers (`chat::replies`).
            let reply = match b["replyTo"].as_str().filter(|id| !id.is_empty()) {
                Some(id) => Some(crate::chat::replies::quote(&hub.store, bot, id)?),
                None => None,
            };
            let lane = side.map_or_else(|| Lane::main(bot), |root| Lane::in_thread(bot, &root));
            let mut attachments = Vec::new();
            if !uploads.is_empty() {
                let root = crate::chat::uploads::root(&row.config);
                for id in uploads {
                    attachments.push(crate::chat::uploads::resolve(&root, id)?.1);
                }
            }
            let mut data = json!({"text": text, "clientNonce": nonce, "attachments": attachments});
            // What the user pasted rather than typed: memory never takes it as the user's own words.
            let pasted: Vec<&str> = b["pasted"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|p| !p.is_empty() && text.contains(p))
                .collect();
            if !pasted.is_empty() {
                data["pasted"] = json!(pasted);
            }
            if let Some(reply) = reply {
                data["replyTo"] = reply;
            }
            // Said on a voice call (on-device or speech to text): the apps show it in the call's conversation.
            if let Some(call) = b["call"].as_str() {
                data["call"] = crate::chat::calls::checked(call)?.into();
                crate::chat::calls::heard(hub, bot, call, caller.device_key())?;
            }
            json!({"entry": crate::chat::messages::post_in_lane(hub, &row.config, lane, data)?})
        }
        // A voice call is on (its audio flows): the lead speaks first.
        "callStarted" => {
            let call = b["callId"].as_str().map(crate::chat::calls::checked).transpose()?;
            crate::chat::lead::call_started(hub, str_arg(&b, "botId")?, call)?;
            json!({})
        }
        // A finished voice call, as a line in the chat ("Voice chat · 0:16").
        "logCall" => crate::voice::receipts::log(hub, &b)?,
        // A voice call, owned by the host (`chat::calls`); only the device that starts it hears what its voice should
        // say.
        "startCall" => crate::chat::calls::start(hub, &b, caller.device_key()).await?,
        // `lines`: what was said on the call that the app hadn't sent yet, which the tool may hand on.
        "callTool" => {
            let call = crate::chat::calls::checked(str_arg(&b, "callId")?)?;
            let result = crate::chat::calls::tools::call(hub, call, str_arg(&b, "name")?, &b["arguments"], &b["lines"]);
            json!({"result": result.await})
        }
        "callLines" => {
            crate::chat::calls::add_lines(hub, crate::chat::calls::checked(str_arg(&b, "callId")?)?, &b["lines"])?;
            json!({})
        }
        // What a call carried, for its conversation: the chat an app holds may not reach back that far.
        "callEntries" => {
            let bot = str_arg(&b, "botId")?;
            let call = crate::chat::calls::checked(str_arg(&b, "callId")?)?;
            json!({"entries": hub.store.call_entries(bot, call, crate::chat::calls::CALL_ENTRIES)?})
        }
        "endCall" => {
            let call = crate::chat::calls::checked(str_arg(&b, "callId")?)?;
            let seconds = b["seconds"].as_u64().unwrap_or(0);
            json!({"entry": crate::chat::calls::end(hub, str_arg(&b, "botId")?, call, seconds, &b["lines"])?})
        }
        "stop" => {
            hub.stop_bot(str_arg(&b, "botId")?)?;
            json!({})
        }
        "tokenBudgets" => hub.budget.status(crate::store::now_ms()),
        "setTokenBudget" => {
            let limit = b["limit"].as_u64();
            hub.budget.set_limit(&hub.store, str_arg(&b, "botId")?, limit)?;
            hub.budget.status(crate::store::now_ms())
        }
        "clearFailedRuns" => {
            let cleared = hub.routines.clear_failed(&hub.store, b["botId"].as_str())?;
            json!({"cleared": cleared})
        }
        "exportBot" => {
            let row = hub.store.bot(str_arg(&b, "botId")?)?.context("unknown bot")?;
            crate::bundle::export(&[row.config])?
        }
        "importBots" => crate::bundle::import(hub, &b["bundle"], str_arg(&b, "cwd")?).await?,
        "retireDormantSkills" => {
            let hub = hub.clone();
            let turned_off =
                tokio::task::spawn_blocking(move || crate::market::retire_dormant_skills(&hub, crate::store::now_ms()))
                    .await??;
            json!({"turnedOff": turned_off})
        }
        "doctor" => crate::doctor::report(hub)?,
        "stopAll" => json!({"stopped": hub.stop_all()?}),
        "switchBack" => {
            hub.send_cmd(str_arg(&b, "botId")?, Cmd::SwitchBack)?;
            json!({})
        }
        "newSession" => {
            let bot = str_arg(&b, "botId")?;
            if !hub.store.bot(bot)?.is_some_and(|r| crate::external::is_external(&r.config)) {
                hub.send_cmd(bot, Cmd::NewSession)?;
            }
            json!({})
        }
        "memory" | "memoryFile" => crate::chat::memory::call(hub, method, &b).await?,
        "skillProposals" | "editSkillProposal" | "approveSkillProposal" | "dismissSkillProposal" => {
            crate::chat::skill_proposals::call(hub, method, &b).await?
        }
        "beliefs" | "beliefAction" | "undoMemoryChange" => crate::chat::memory::belief_call(hub, method, &b).await?,
        "importAgentMemory" => {
            crate::chat::memory::import_agent_memory(hub, str_arg(&b, "botId")?, &b["facts"]).await?
        }
        "library" => crate::chat::library::list(hub, str_arg(&b, "botId")?)?,
        "readArtifact" | "renameArtifact" => crate::chat::artifacts::api(hub, method, &b).await?,
        "editHandoff" | "keepConversation" => crate::chat::handoff::call(hub, method, &b)?,
        "react" => {
            let e = hub.react(str_arg(&b, "entryId")?, str_arg(&b, "emoji")?)?;
            json!({"entry": e})
        }
        // The security profiles to choose among: their sentence and what each sets, for a client to show.
        "securityProfiles" => json!({"profiles": crate::agent::security_profile::describe_all()}),
        // What the bot would do with a command: the same decision a real request gets, so a rule editor tests rules
        // without copying their logic. `rules` tests an unsaved list; without it the bot's saved rules.
        "testApprovalRule" => {
            let row =
                hub.store.bot(str_arg(&b, "botId")?)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let rules: Vec<crate::agent::approval_rules::ApprovalRule> = match b.get("rules") {
                Some(list) if !list.is_null() => serde_json::from_value(list.clone())?,
                _ => row.config.approval_rules.clone(),
            };
            crate::agent::approval_rules::validate(&rules)?;
            let command = str_arg(&b, "command")?;
            let kind = b["kind"].as_str().unwrap_or("execute");
            // The quick rules as the editor has them (unsaved), else the bot's saved ones.
            let policies = match b.get("actionRules") {
                Some(map) if map.is_object() => serde_json::from_value(map.clone())?,
                _ => row.config.action_rules.clone(),
            };
            let quick_on = b["actionRulesOn"].as_bool().unwrap_or(row.config.action_rules_on);
            let quick = if quick_on { crate::agent::approval_rules::quick_rules(&policies) } else { Vec::new() };
            let evaluation = crate::agent::approval_rules::evaluate(
                &rules,
                &quick,
                row.config.permission,
                kind,
                Some(command),
                command,
            );
            // The quick-rule classes approvals cannot enforce, and why, so an editor can say so.
            let not_enforced: Vec<Value> = crate::agent::approval_rules::QUICK_UNENFORCED
                .iter()
                .map(|(class, why)| json!({"class": class, "reason": why}))
                .collect();
            json!({"evaluation": evaluation, "quickNotEnforced": not_enforced})
        }
        // A paused turn's card: one tap sends the continuation (bounded, once per card).
        "continueTurn" => {
            let entry_id = str_arg(&b, "entryId")?;
            let e = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
            let used = crate::agent::bot::pause::begin(&e.data)?;
            if hub.runtime(&e.bot_id).status != crate::hub::BotStatus::Idle {
                bail!("The bot is busy; continue when it is idle");
            }
            hub.set_entry(entry_id, &used);
            crate::api::apps_runtime::dispatch_bot_turn(
                hub,
                &e.bot_id,
                crate::agent::bot::pause::CONTINUE_TEXT,
                "continue",
            )?;
            json!({})
        }
        // The rule an "always allow" answer to this card means, to show (and let the person edit) before it is saved.
        "proposeRule" => {
            let e = hub.store.entry(str_arg(&b, "entryId")?).ok_or_else(|| anyhow!("unknown card"))?;
            if e.kind != EntryKind::Permission.as_str() {
                bail!("That isn't an approval card");
            }
            match crate::agent::always_rule::propose(&e.data) {
                Ok(rule) => json!({"rule": rule}),
                Err(refusal) => json!({"rule": null, "refusal": refusal, "reason": refusal.reason()}),
            }
        }
        "respondPermission" => {
            let entry_id = str_arg(&b, "entryId")?;
            let e = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
            if e.kind != EntryKind::Permission.as_str() {
                bail!("That isn't an approval card");
            }
            let option_id = b["optionId"].as_str().map(str::to_owned);
            // The card is the effect's key: the first decision is the one that happens. The same
            // answer again (another device, a retry) gets the card back; a different one is refused.
            match e.data["status"].as_str() {
                Some("pending") => {}
                Some("answered") if e.data["selected"].as_str() == option_id.as_deref() => {
                    return Ok(ControlFlow::Break(json!({"entry": e})));
                }
                Some("answered") => bail!("This approval was already answered with another choice"),
                _ => bail!("This approval is closed: its turn ended or was stopped"),
            }
            // "Always allow" with a rule: check it, and keep it for the bot before the answer goes out.
            if let Some(rule) = b.get("rule").filter(|r| r.is_object()) {
                save_always_rule(hub, &e, option_id.as_deref(), rule)?;
            }
            if e.data["mcpToolCallId"].is_string() {
                crate::market::vault::unlock(hub.clone()).await?;
            }
            if crate::mcp::policy::respond(hub, entry_id, option_id.as_deref())? {
                return Ok(ControlFlow::Break(json!({})));
            }
            // A terminal session's hook is waiting for this, not a bot actor.
            if hub.external.answer(hub, entry_id, option_id.as_deref()) {
                return Ok(ControlFlow::Break(json!({})));
            }
            // A card in a group belongs to the bot that asked.
            let bot = e.data["author"].as_str().unwrap_or(&e.bot_id);
            hub.send_cmd(bot, Cmd::Permission { entry_id: entry_id.to_owned(), option_id })?;
            json!({})
        }
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}

/// Adds the rule that goes with an "always allow" answer to the card's bot. Only `allow_always` carries one, and a rule
/// that chains commands or allows what no bot may run is refused (`agent::always_rule`).
fn save_always_rule(hub: &Hub, card: &crate::store::Entry, option_id: Option<&str>, rule: &Value) -> Result<()> {
    let chosen_always = card.data["options"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|o| o["optionId"].as_str() == option_id && o["kind"] == "allow_always");
    if !chosen_always {
        bail!("A rule is saved only with \"always allow\"");
    }
    let rule: crate::agent::approval_rules::ApprovalRule = serde_json::from_value(rule.clone())?;
    crate::agent::always_rule::check(&rule).map_err(|refusal| anyhow!(refusal.reason()))?;
    let row = hub.store.bot(&card.bot_id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    let rules = crate::agent::always_rule::added(&row.config.approval_rules, rule);
    hub.update_bot(&json!({"id": card.bot_id, "approvalRules": rules}))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LockExt;
    use crate::remote::identity::Identity;
    use crate::store::Store;

    /// A hub on an in-memory store; the bot list reads the store, nothing else.
    fn fixture() -> (Arc<Hub>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-botlist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        // Keep connector defaults independent of the person's credential manager.
        *store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![0; 32]));
        let hub = Hub::new(store, "botlist-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        (hub, dir)
    }

    async fn list(hub: &Arc<Hub>) -> Value {
        match call(hub, &Caller::Local, "botList", json!({})).await.unwrap() {
            ControlFlow::Break(v) => v,
            ControlFlow::Continue(_) => panic!("botList fell through the route"),
        }
    }

    async fn make(hub: &Arc<Hub>, name: &str) -> Value {
        let cfg = crate::agent::new_bot::with_defaults(hub, json!({"name": name})).await.unwrap();
        hub.create_bot(cfg).unwrap()
    }

    #[tokio::test]
    async fn bot_list_lists_the_agents_the_roster_shows() {
        let (hub, dir) = fixture();
        let alpha = make(&hub, "Alpha").await;
        let beta = make(&hub, "Beta").await;
        let alpha_id = alpha["id"].as_str().unwrap().to_owned();
        let beta_id = beta["id"].as_str().unwrap().to_owned();
        // A group is a conversation, not an agent the node may message.
        let mut group_cfg = crate::agent::new_bot::with_defaults(&hub, json!({"name": "Team"})).await.unwrap();
        group_cfg.kind = BotKind::Group;
        group_cfg.members = vec![alpha_id.clone(), beta_id.clone()];
        hub.create_bot(group_cfg).unwrap();

        let out = list(&hub).await;
        let ids: Vec<&str> = out["bots"].as_array().unwrap().iter().map(|b| b["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec![alpha_id.as_str(), beta_id.as_str()], "groups stay out of the list");
        let entry = &out["bots"].as_array().unwrap()[0];
        assert_eq!(entry["name"], "Alpha");
        assert_eq!(entry["external"], false);

        // Hidden bots stay out, as in the roster.
        hub.update_bot(&json!({"id": beta_id, "hidden": true})).unwrap();
        let out = list(&hub).await;
        let ids: Vec<&str> = out["bots"].as_array().unwrap().iter().map(|b| b["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec![alpha_id.as_str()]);

        // Deleted bots stay out too.
        hub.delete_bot(&alpha_id).unwrap();
        assert_eq!(list(&hub).await["bots"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
