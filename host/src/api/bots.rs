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
    if let Some(answer) = crate::routines::flow::api_call(hub, method, &b)? {
        return Ok(ControlFlow::Break(answer));
    }
    Ok(ControlFlow::Break(match method {
        "hermesRuntime" | "hermesRuntimeStatus" | "setHermesRuntime" | "hermesEnvironmentHealth" => {
            crate::agent::hermes::api(hub, method, &b).await?
        }
        "routineSchedule" => crate::routines::schedule_preview(&b, crate::store::now_ms())?,
        "browserProxy" => {
            let bot = str_arg(&b, "botId")?;
            crate::market::vault::unlock(hub.clone()).await?;
            let proxy = crate::browser::proxy::load(&hub.store, bot)?;
            json!({"proxy": proxy.as_ref().map(crate::browser::proxy::Proxy::masked), "authenticated": proxy.is_some_and(|p| p.username.is_some())})
        }
        "setBrowserProxy" => {
            let bot = str_arg(&b, "botId")?;
            let proxy = crate::browser::proxy::parse(str_arg(&b, "address")?)?;
            crate::market::vault::unlock(hub.clone()).await?;
            crate::browser::proxy::save(&hub.store, bot, &proxy)?;
            json!({"proxy": proxy.masked(), "authenticated": proxy.username.is_some()})
        }
        "clearBrowserProxy" => {
            let bot = str_arg(&b, "botId")?;
            crate::market::vault::unlock(hub.clone()).await?;
            crate::browser::proxy::clear(&hub.store, bot)?;
            json!({"proxy": null, "authenticated": false})
        }
        "routines" => hub.routines.list(str_arg(&b, "botId")?, |_| false),
        "routineTemplates" => crate::routines::blueprints::list(),
        "routineTemplateDraft" => crate::routines::blueprints::draft(&b)?,
        "routineCalendar" => hub.routines.calendar(
            str_arg(&b, "botId")?,
            b["from"].as_i64().context("from is required")?,
            b["to"].as_i64().context("to is required")?,
        )?,
        "saveRoutine" => hub.routines.save(hub, str_arg(&b, "botId")?, &b)?,
        "saveRoutineWithOutputs" => {
            if b.get("outputs").is_none() && b.get("outputText").is_none() {
                bail!("outputs or outputText is required");
            }
            hub.routines.save(hub, str_arg(&b, "botId")?, &b)?
        }
        "setRoutineEnabled" => hub.routines.set_enabled(
            &hub.store,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            b["enabled"].as_bool().context("enabled is required")?,
        )?,
        "deleteEntry" => json!({"deleted": hub.delete_entry(str_arg(&b, "entryId")?)?}),
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
            } else if crate::chat::skills::TOOLS.contains(&name) {
                crate::chat::skills::dispatch(hub, bot, name, &b["arguments"]).await?
            } else if crate::chat::fallbacks::TOOLS.contains(&name) {
                crate::chat::fallbacks::call(hub, bot, name, &b["arguments"]).await?
            } else if crate::chat::artifacts::TOOLS.contains(&name) {
                crate::chat::artifacts::call(hub, bot, name, &b["arguments"]).await?
            } else if crate::dev_servers::TOOLS.contains(&name) {
                crate::dev_servers::call(hub, bot, name, &b["arguments"]).await?
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
            let mut entries = vec![];
            for e in hub.store.history(bot, before, limit)? {
                let mut entry = serde_json::to_value(&e)?;
                hub.store.decorate_thread(&mut entry)?;
                entries.push(entry);
            }
            json!({"entries": entries})
        }
        // A side conversation (one opened from the chat, a document's `doc:<id>`, a routine's thread, a Slack
        // thread's `slack-…`); the chat itself has none.
        "thread" => {
            let root = str_arg(&b, "rootId")?;
            let bot = str_arg(&b, "botId")?;
            let entries = hub.store.thread(bot, root, 500)?;
            let thread = hub.store.thread_by_id(root)?;
            if entries.is_empty() && thread.is_none() {
                bail!("unknown thread");
            }
            // What the client hasn't seen yet, read before opening the thread clears it.
            let unread = hub.store.thread_unread(root);
            hub.mark_thread_read(bot, root)?;
            json!({"entries": entries, "thread": thread, "unread": unread})
        }
        // A chat's threads, the last one written first: never the lines, only where each ends.
        "threads" => {
            let bot = str_arg(&b, "botId")?;
            hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            json!({"threads": hub.store.threads(bot, b["archived"].as_bool().unwrap_or(false))?})
        }
        // A thread opened from the chat, optionally branched from one of its messages.
        "createThread" => {
            let bot = str_arg(&b, "botId")?;
            hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let from = b["fromEntryId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .filter(|id| hub.store.entry(id).is_some_and(|e| e.bot_id == bot && e.thread_id.is_none()));
            json!({"threadId": hub.store.create_thread(bot, b["title"].as_str().unwrap_or(""), from)?})
        }
        "renameThread" => {
            hub.store.rename_thread(str_arg(&b, "threadId")?, str_arg(&b, "title")?)?;
            json!({})
        }
        "archiveThread" => {
            hub.store.archive_thread(str_arg(&b, "threadId")?, b["archived"].as_bool().unwrap_or(true))?;
            json!({})
        }
        "deleteThread" => {
            let id = str_arg(&b, "threadId")?;
            let bot = hub.store.thread_by_id(id)?.map(|t| t.bot_id).unwrap_or_default();
            if hub.store.delete_thread(id)?.is_some() && !bot.is_empty() {
                hub.thread_deleted(id, &bot);
            }
            json!({})
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
        // Bots from another app's export file on this computer (`import`).
        "previewImport" => crate::import::preview(&b).await?,
        "importBots" => crate::import::apply(hub, &b).await?,
        "importSkill" => json!({"name": "codync-export", "text": crate::import::EXPORT_SKILL}),
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
        // The roster as the user dragged it, top first.
        "reorderBots" => {
            let ids: Vec<String> = serde_json::from_value(b["ids"].clone()).context("ids is required")?;
            let hub = hub.clone();
            tokio::task::spawn_blocking(move || hub.reorder_bots(&ids)).await??;
            json!({})
        }
        // Every bot in section `from` moves to `to`; `null` or an empty name removes the section.
        "renameSection" => {
            const TO: &str = "`to` must be a section name or null";
            let from = str_arg(&b, "from")?.to_owned();
            // A missing `to` is refused, not read as null: a client bug must not strip every section.
            let to: Option<String> = serde_json::from_value(b.get("to").cloned().context(TO)?).context(TO)?;
            let hub = hub.clone();
            tokio::task::spawn_blocking(move || hub.rename_section(&from, to.as_deref())).await??;
            json!({})
        }
        "duplicateBot" => {
            let hub = hub.clone();
            let id = str_arg(&b, "botId")?.to_owned();
            json!({"bot": tokio::task::spawn_blocking(move || hub.duplicate_bot(&id)).await??})
        }
        "botUsage" => hub.budget.bot_usage(str_arg(&b, "botId")?, crate::store::now_ms()),
        // What this bot waits on: open approvals, routine runs and threads with unread replies.
        "botTasks" => {
            let bot = str_arg(&b, "botId")?;
            let approvals: Vec<Value> = hub
                .store
                .pending_approvals(bot)?
                .into_iter()
                .map(|entry| {
                    json!({
                        "id": entry.id,
                        "title": entry.data["title"].as_str().unwrap_or("Approval"),
                        "detail": entry.data["detail"].as_str().unwrap_or_default(),
                        "since": entry.created_at,
                    })
                })
                .collect();
            let runs: Vec<Value> = hub
                .routines
                .runs_for(bot)
                .into_iter()
                .map(|run| {
                    json!({
                        "id": run.id,
                        "routineId": run.routine_id,
                        "status": run.status,
                        "startedAt": run.started_at,
                        "finishedAt": run.finished_at,
                        "detail": run.detail,
                        "threadRoot": run.thread_root,
                    })
                })
                .collect();
            let threads: Vec<Value> = hub
                .store
                .thread_roots(bot)?
                .into_iter()
                .filter_map(|id| {
                    let unread = hub.store.thread_unread(&id);
                    (unread > 0).then(|| json!({"id": id, "unread": unread}))
                })
                .collect();
            json!({"approvals": approvals, "runs": runs, "threads": threads})
        }
        // The apps name a dev server, and never send a command.
        "devServerStop" => {
            crate::dev_servers::stop(hub, str_arg(&b, "botId")?, str_arg(&b, "id")?, "user").await?;
            json!({})
        }
        "devServerRestart" => {
            crate::dev_servers::restart(hub, str_arg(&b, "botId")?, str_arg(&b, "id")?).await?;
            json!({})
        }
        "devServerRemove" => {
            crate::dev_servers::remove(hub, str_arg(&b, "botId")?, str_arg(&b, "id")?)?;
            json!({})
        }
        "devServerLogs" => {
            json!({"text": crate::dev_servers::shown_logs(hub, str_arg(&b, "botId")?, str_arg(&b, "id")?).await?})
        }
        "deleteBot" => {
            hub.delete_bot(str_arg(&b, "botId")?)?;
            json!({})
        }
        // The user makes this bot the lead, in place of the one before.
        "setLead" => {
            crate::chat::lead::set(hub, str_arg(&b, "botId")?)?;
            json!({})
        }
        "markRead" => {
            hub.mark_read(str_arg(&b, "botId")?)?;
            json!({})
        }
        // A message and everything after it read unread again; the chat stays unread until opened.
        "markUnread" => json!({"unread": hub.mark_unread(str_arg(&b, "botId")?, str_arg(&b, "entryId")?)?}),
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
            // A reply to a pending `ask_member` card answers it instead of starting a turn.
            if let Some(card) = b["replyTo"].as_str().filter(|id| !id.is_empty())
                && !text.is_empty()
                && hub.members.waiting(card)
                && hub.members.answer(card, "You", text)
            {
                return Ok(ControlFlow::Break(json!({"answered": true})));
            }
            if !nonce.is_empty()
                && let Some(e) = hub.store.find_by_nonce(bot, nonce)
            {
                return Ok(ControlFlow::Break(json!({"entry": e})));
            }
            let row = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            if crate::external::is_external(&row.config) {
                bail!("this bot runs in your terminal: reply there, or use its approval cards here");
            }
            // `threadId`: a side conversation (a document's `doc:<id>`, a routine's thread); the chat itself has none.
            let side = match b["threadId"].as_str().filter(|t| !t.is_empty()) {
                Some(root) if root.starts_with("doc:") => {
                    let id = root.strip_prefix("doc:").context("unknown document conversation")?;
                    hub.store.document(id)?.filter(|doc| doc.bot_id == bot).context("unknown document conversation")?;
                    Some(root.to_owned())
                }
                Some(root) => Some(root.to_owned()),
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
        // A waiting message rewritten before it goes out.
        "editQueued" => {
            let (reply, answer) = tokio::sync::oneshot::channel();
            let cmd = Cmd::EditQueued {
                entry_id: str_arg(&b, "entryId")?.to_owned(),
                text: b["text"].as_str().unwrap_or_default().to_owned(),
                reply,
            };
            hub.send_cmd(str_arg(&b, "botId")?, cmd)?;
            answer.await??;
            json!({})
        }
        // A message sent on to another chat (the message menu's "Send to a channel").
        "forward" => {
            json!({"entry": crate::chat::forward::forward(hub, str_arg(&b, "entryId")?, str_arg(&b, "botId")?).await?})
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
        "retireDormantSkills" => {
            let hub = hub.clone();
            let turned_off =
                tokio::task::spawn_blocking(move || crate::market::retire_dormant_skills(&hub, crate::store::now_ms()))
                    .await??;
            json!({"turnedOff": turned_off})
        }
        "doctor" => crate::doctor::report(hub)?,
        "stopAll" => json!({"stopped": hub.stop_all()?}),
        // A message of the user's waiting for its turn: handled now (`steer`) or taken back (`unqueue`).
        "steer" | "unqueue" | "deleteQueued" => {
            let (bot, entry_id) = (str_arg(&b, "botId")?, str_arg(&b, "entryId")?.to_owned());
            let queued = hub
                .store
                .entry(&entry_id)
                .filter(|e| e.bot_id == bot && e.kind == EntryKind::User.as_str() && e.data["status"] == "queued");
            if queued.is_none() {
                bail!("that message isn't waiting any more");
            }
            let cmd = if method == "steer" { Cmd::Steer { entry_id } } else { Cmd::Unqueue { entry_id } };
            hub.send_cmd(bot, cmd)?;
            json!({})
        }
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
        // The security profiles to choose among: their sentence and what each sets, for a client to show.
        "securityProfiles" => json!({"profiles": crate::agent::security_profile::describe_all()}),
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
        // Cancel task: ends a stopped turn (its card) or a failed routine run (its notice); the
        // messages they answered stay.
        "cancelTask" => {
            let entry_id = str_arg(&b, "entryId")?;
            let e = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
            if e.data["pausedTurn"].is_object() {
                let closed = crate::agent::bot::pause::close(&e.data)?;
                hub.set_entry(entry_id, &closed);
            } else if e.data["runId"].is_string() && e.data["source"] == "scheduled_task" {
                let run_id = str_arg(&e.data, "runId")?;
                hub.routines.cancel(hub, &e.bot_id, run_id)?;
            } else {
                bail!("That isn't a stopped or failed task");
            }
            json!({})
        }
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
