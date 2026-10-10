//! Bot and chat methods: roster, messages, uploads, memory, routines and the bots' own MCP calls.

use super::marketplace::connector_ids;
use super::str_arg;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::market;
use crate::store::{BotConfig, EntryKind, Lane, ReadScope};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
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
        "deleteRoutine" => hub.routines.remove(&hub.store, str_arg(&b, "botId")?, str_arg(&b, "id")?)?,
        "runRoutine" => hub.routines.enqueue(
            &hub.store,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            json!({"source":"test"}),
            None,
            true,
        )?,
        "routineWebhook" => hub.routines.credentials(
            hub,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            b["rotate"].as_bool().unwrap_or(false),
        )?,
        "routineCall" => crate::routines::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"])?,
        "memoryCall" => {
            let bot = str_arg(&b, "botId")?;
            let name = str_arg(&b, "name")?;
            let args = if name.starts_with("mem_") {
                crate::chat::memory::lifecycle::bound_args(hub, bot, b["memoryLane"].as_str(), name, &b["arguments"])
                    .await?
            } else {
                b["arguments"].clone()
            };
            crate::chat::memory::call(hub, bot, name, &args).await?
        }
        "memoryTools" => crate::chat::memory::available_tools(hub, str_arg(&b, "botId")?).await?,
        "chatCall" => {
            crate::chat::outbox::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "teamCall" => {
            crate::chat::team::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "history" => {
            let bot = str_arg(&b, "botId")?;
            let before = b["beforeSeq"].as_i64().unwrap_or(i64::MAX);
            let limit = b["limit"].as_i64().unwrap_or(100).clamp(1, 500);
            json!({"entries": hub.store.history(bot, before, limit)?})
        }
        // A thread's replies (its newest 500); the root is in the main chat.
        "thread" => json!({"entries": hub.store.thread(str_arg(&b, "botId")?, str_arg(&b, "rootId")?, 500)?}),
        // Notices between a bot and one peer (the newest 200), for the read-only bot conversation sheet.
        "botConversation" => {
            json!({"entries": hub.store.bot_conversation(str_arg(&b, "botId")?, str_arg(&b, "peerId")?, 200)?})
        }
        "createBot" => {
            let mut b = b;
            b["id"] = "".into();
            // Connectors are on for a new bot unless the client picked them.
            // Without a credential store (headless Linux) no connector can run, so the bot starts with none.
            if b["connectors"].is_null() && b["kind"] != "group" {
                b["connectors"] = match market::vault::unlock(hub.clone()).await {
                    Ok(()) => json!(connector_ids(&hub.store)?),
                    Err(e) => {
                        tracing::warn!(
                            error = format!("{e:#}"),
                            "credential store unavailable; new bot starts without connectors"
                        );
                        json!([])
                    }
                };
            }
            let cfg: BotConfig = serde_json::from_value(b).context("invalid bot")?;
            let hub = hub.clone();
            json!({"bot": tokio::task::spawn_blocking(move || hub.create_bot(cfg)).await??})
        }
        "updateBot" => {
            let hub = hub.clone();
            json!({"bot": tokio::task::spawn_blocking(move || hub.update_bot(&b)).await??})
        }
        "deleteBot" => {
            hub.delete_bot(str_arg(&b, "botId")?)?;
            json!({})
        }
        // The main chat, one thread (`threadId`), or everything (`all`, the roster's "Mark as read").
        "markRead" => {
            let scope = match b["threadId"].as_str().filter(|t| !t.is_empty()) {
                Some(root) => ReadScope::Thread(root),
                None if b["all"] == true => ReadScope::All,
                None => ReadScope::Chat,
            };
            hub.mark_read(str_arg(&b, "botId")?, scope)?;
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
            // `threadId`: reply in the thread on that message of the main chat (threads are flat).
            let thread = match b["threadId"].as_str().filter(|t| !t.is_empty()) {
                Some(root) => {
                    let r = hub.store.entry(root).ok_or_else(|| anyhow!("unknown message"))?;
                    if r.bot_id != bot || r.thread_id.is_some() {
                        bail!("threads start from a message in the main chat");
                    }
                    Some(root.to_owned())
                }
                None => None,
            };
            let lane = Lane { chat: bot.to_owned(), thread };
            let (mut paths, mut attachments) = (Vec::new(), Vec::new());
            if !uploads.is_empty() {
                let root = crate::chat::uploads::root(&row.config);
                for id in uploads {
                    let (path, meta) = crate::chat::uploads::resolve(&root, id)?;
                    paths.push(path);
                    attachments.push(meta);
                }
            }
            let prompt = if paths.is_empty() {
                text.to_owned()
            } else {
                format!("{text}{}", crate::chat::uploads::prompt_suffix(&paths))
            };
            let turn = hub.store.max_turn(bot) + 1;
            // A group has no agent of its own to queue behind: its message is simply sent.
            let status = if row.config.is_group() { "sent" } else { "queued" };
            let e = hub
                .add_entry(
                    &lane,
                    EntryKind::User,
                    turn,
                    &json!({"text": text, "clientNonce": nonce, "status": status, "attachments": attachments}),
                )
                .ok_or_else(|| anyhow!("couldn't save the message"))?;
            // Before the turn starts, so its reply lands unread.
            let scope = lane.thread.as_deref().map_or(ReadScope::Chat, ReadScope::Thread);
            if let Err(error) = hub.mark_read(bot, scope) {
                tracing::warn!(%error, bot, "couldn't mark bot read after send");
            }
            if row.config.is_group() {
                crate::chat::group::start(hub, &row.config, lane);
            } else {
                hub.send_cmd(bot, Cmd::Send { lane, entry_id: e.id.clone(), text: prompt })?;
            }
            json!({"entry": e})
        }
        // A finished voice call, as a line in the chat ("Voice chat · 0:16").
        "logCall" => {
            let bot = str_arg(&b, "botId")?;
            hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let seconds = b["seconds"].as_u64().unwrap_or(0);
            let data = json!({"text": "Voice chat", "callSeconds": seconds});
            let e = hub.add_entry(&Lane::main(bot), EntryKind::Notice, hub.store.max_turn(bot), &data);
            json!({"entry": e})
        }
        "stop" => {
            let bot = str_arg(&b, "botId")?;
            match hub.store.bot(bot)? {
                Some(row) if row.config.is_group() => hub.groups.stop(hub, bot),
                _ => hub.send_cmd(bot, Cmd::Stop)?,
            }
            json!({})
        }
        "newSession" => {
            hub.send_cmd(str_arg(&b, "botId")?, Cmd::NewSession)?;
            json!({})
        }
        "memory" | "saveMemory" | "memoryDetail" | "pinMemory" | "reviewMemory" | "forgetMemory" | "clearMemory"
        | "exportMemory" | "importMemory" => crate::chat::memory::manage::call(hub, method, &b).await?,
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
