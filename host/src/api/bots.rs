//! Bot and chat methods: roster, messages, uploads, memory, artifacts, routines and the bots' own MCP calls.

use super::devices::Caller;
use super::str_arg;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{EntryKind, Lane};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
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
        "chatCall" => {
            let (bot, name) = (str_arg(&b, "botId")?, str_arg(&b, "name")?);
            if crate::chat::profile::TOOLS.contains(&name) {
                crate::chat::profile::call(hub, bot, name, &b["arguments"]).await?
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
        "manageCall" => {
            crate::chat::manage::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "history" => {
            let bot = str_arg(&b, "botId")?;
            let before = b["beforeSeq"].as_i64().unwrap_or(i64::MAX);
            let limit = b["limit"].as_i64().unwrap_or(100).clamp(1, 500);
            json!({"entries": hub.store.history(bot, before, limit)?})
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
            let hub = hub.clone();
            json!({"bot": tokio::task::spawn_blocking(move || hub.update_bot(&b)).await??})
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
            // `replyTo`: the message of this chat it answers (`chat::replies`).
            let reply = match b["replyTo"].as_str().filter(|id| !id.is_empty()) {
                Some(id) => Some(crate::chat::replies::quote(&hub.store, bot, id)?),
                None => None,
            };
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
            json!({"entry": crate::chat::messages::post(hub, &row.config, data)?})
        }
        // A voice call is on (its audio flows): the lead speaks first.
        "callStarted" => {
            let call = b["callId"].as_str().map(crate::chat::calls::checked).transpose()?;
            crate::chat::lead::call_started(hub, str_arg(&b, "botId")?, call)?;
            json!({})
        }
        // A finished voice call from an app older than `endCall`, as a line in the chat ("Voice chat · 0:16").
        "logCall" => {
            let bot = str_arg(&b, "botId")?;
            hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let seconds = b["seconds"].as_u64().unwrap_or(0);
            let data = json!({"text": crate::chat::calls::UNTITLED, "callSeconds": seconds});
            let e = hub.add_entry(&Lane::main(bot), EntryKind::Notice, hub.store.max_turn(bot), &data);
            json!({"entry": e})
        }
        // A voice call, owned by the host (`chat::calls`); only the device that starts it hears what its voice should
        // say.
        "startLiveCall" => crate::chat::calls::live::start(hub, &b, caller.device_key()).await?,
        "callLiveEvent" => crate::chat::calls::live::event(hub, &b, caller.device_key()).await?,
        "callLiveUpdate" => crate::chat::calls::live::update(hub, &b, caller.device_key()).await?,
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
            let bot = str_arg(&b, "botId")?;
            match hub.store.bot(bot)? {
                Some(row) if row.config.is_group() => hub.groups.stop(hub, bot),
                _ => hub.send_cmd(bot, Cmd::Stop)?,
            }
            json!({})
        }
        "switchBack" => {
            hub.send_cmd(str_arg(&b, "botId")?, Cmd::SwitchBack)?;
            json!({})
        }
        "newSession" => {
            hub.send_cmd(str_arg(&b, "botId")?, Cmd::NewSession)?;
            json!({})
        }
        "memory" | "memoryFile" => crate::chat::memory::call(hub, method, &b).await?,
        "library" => crate::chat::library::list(hub, str_arg(&b, "botId")?)?,
        "readArtifact" | "renameArtifact" => crate::chat::artifacts::api(hub, method, &b).await?,
        "editHandoff" | "keepConversation" => crate::chat::handoff::call(hub, method, &b)?,
        "react" => {
            let e = hub.react(str_arg(&b, "entryId")?, str_arg(&b, "emoji")?)?;
            json!({"entry": e})
        }
        "respondPermission" => {
            let entry_id = str_arg(&b, "entryId")?;
            let e = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
            let option_id = b["optionId"].as_str().map(str::to_owned);
            // A card in a group belongs to the bot that asked.
            let bot = e.data["author"].as_str().unwrap_or(&e.bot_id);
            hub.send_cmd(bot, Cmd::Permission { entry_id: entry_id.to_owned(), option_id })?;
            json!({})
        }
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}
