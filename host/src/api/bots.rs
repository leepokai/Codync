//! Bot and chat methods: roster, messages, uploads, memory, artifacts, routines and the bots' own MCP calls.

use super::devices::Caller;
use super::str_arg;
use crate::agent::bot::Cmd;
use crate::chat::work::{self, Steer};
use crate::hub::Hub;
use crate::store::{EntryKind, Lane};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

/// The methods a bot's MCP servers call the host's tools through.
const BOT_TOOL_CALLS: [&str; 5] = ["routineCall", "memoryCall", "chatCall", "teamCall", "manageCall"];

/// Where a message's files go and are read back from: `bot`'s chat, or with `task` the thread of that task of the
/// bot's, which the thread's agent reads them from.
fn uploads_root(hub: &Hub, bot: &str, task: Option<&str>) -> Result<std::path::PathBuf> {
    let id = match task {
        Some(task) => work::thread_of(hub, bot, task)?,
        None => bot.to_owned(),
    };
    // A thread that ended over a week ago is parked: its files are still read back.
    let cfg = match hub.store.bot(&id)?.filter(|r| !r.deleted) {
        Some(row) => row.config,
        None => hub.store.parked_thread(&id)?.ok_or_else(|| anyhow!("unknown bot"))?,
    };
    Ok(crate::chat::uploads::root(&cfg))
}

/// A message's `attachments`: upload ids.
/// The app's terminal tab `tab` names (`terminal:<id>`), when given: short, of letters, digits, `:`, `-` and `_`.
fn tab_arg(b: &Value) -> Result<Option<&str>> {
    let tab = b["tab"].as_str();
    if tab.is_some_and(|t| {
        t.is_empty() || t.len() > 64 || !t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_'))
    }) {
        bail!("not a terminal tab");
    }
    Ok(tab)
}

fn attachment_ids(b: &Value) -> Vec<&str> {
    b["attachments"].as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}

/// What the user does to a task of a bot's box.
async fn steer_task(hub: &Arc<Hub>, steer: Steer, b: &Value) -> Result<Value> {
    let (hub, bot) = (hub.clone(), str_arg(b, "botId")?.to_owned());
    let task = b["taskId"].as_str().map(str::to_owned);
    let instead_of = b["insteadOf"].as_str().map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        let named = || task.as_deref().ok_or_else(|| anyhow!("taskId required"));
        match steer {
            Steer::First => work::first(&hub, &bot, named()?),
            Steer::Now => work::cut_in(&hub, &bot, named()?, instead_of.as_deref()),
            Steer::Drop => work::drop_task(&hub, &bot, named()?),
            Steer::Retry => work::retry(&hub, &bot, named()?),
            // One task, or every one that is working or waiting.
            Steer::Stop => work::stop(&hub, &bot, task.as_deref()),
            // No method asks for it.
            Steer::Replace => bail!("only the bot replaces a brief"),
        }
    })
    .await??;
    Ok(json!({}))
}

pub(super) async fn call(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    if let Some(steer) = Steer::of_method(method) {
        return Ok(ControlFlow::Break(steer_task(hub, steer, &b).await?));
    }
    // A turn a network rule holds to a ceiling calls only the host's tools that read or answer the user. `botId`
    // is whoever's turn calls, a thread included. The computer and connector servers are the agent's own tools, which
    // its mode holds.
    if BOT_TOOL_CALLS.contains(&method) {
        hub.ceilings.check_tool(str_arg(&b, "botId")?, str_arg(&b, "name")?)?;
    }
    Ok(ControlFlow::Break(match method {
        "routineSchedule" => crate::routines::schedule_preview(&b, crate::store::now_ms())?,
        "routines" => hub.routines.list(str_arg(&b, "botId")?),
        "saveRoutine" => {
            let saved = hub.routines.save(hub, str_arg(&b, "botId")?, &b)?;
            work::routines_changed(hub, str_arg(&b, "botId")?);
            saved
        }
        "setRoutineEnabled" | "deleteRoutine" => {
            let (bot, id) = (str_arg(&b, "botId")?, str_arg(&b, "id")?);
            let answer = if method == "deleteRoutine" {
                hub.routines.remove(hub, bot, id)?
            } else {
                hub.routines.set_enabled(&hub.store, bot, id, b["enabled"].as_bool().context("enabled is required")?)?
            };
            work::routines_changed(hub, bot);
            answer
        }
        "runRoutine" => hub.routines.enqueue(
            &hub.store,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            json!({"source":"test"}),
            None,
            true,
        )?,
        "continueChain" => crate::chat::team::limit::continue_chain(hub, str_arg(&b, "entryId")?)?,
        "retryRoutineRun" => hub.routines.retry(hub, str_arg(&b, "botId")?, str_arg(&b, "runId")?)?,
        "routineWebhook" => hub.routines.credentials(
            hub,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            b["rotate"].as_bool().unwrap_or(false),
        )?,
        "cancelFollowUp" => hub.routines.cancel_follow_up(
            hub,
            str_arg(&b, "botId")?,
            str_arg(&b, "id")?,
            crate::routines::Cancelled::User,
        )?,
        "routineCall" => {
            let (caller, name) = (str_arg(&b, "botId")?, str_arg(&b, "name")?);
            let bot = work::acting_as(hub, caller, name)?;
            let answer = crate::routines::call(hub, &bot, caller, name, &b["arguments"])?;
            if matches!(name, "save_routine" | "set_routine_enabled" | "delete_routine") {
                work::routines_changed(hub, &bot);
            }
            answer
        }
        "memoryCall" => {
            crate::chat::memory::tool_call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "chatCall" => {
            let (bot, name) = (str_arg(&b, "botId")?, str_arg(&b, "name")?);
            work::refuse_chat_only(hub, bot, name)?;
            if work::TOOLS.contains(&name) {
                work::call(hub, bot, name, &b["arguments"]).await?
            } else if name == work::ask_chat::TOOL {
                work::ask_chat::ask(hub, bot, &b["arguments"]).await?
            } else if crate::chat::profile::TOOLS.contains(&name) {
                crate::chat::profile::call(hub, bot, name, &b["arguments"]).await?
            } else if name == crate::chat::decision::TOOL {
                crate::chat::decision::ask(hub, bot, &b["arguments"]).await?
            } else if name == crate::chat::skills::TOOL {
                crate::chat::skills::call(hub, bot, &b["arguments"]).await?
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
        // A coding agent's session on this computer that isn't a bot (`codync-host mcp network`).
        "agentCall" => {
            let caller: crate::network::sessions::Caller =
                serde_json::from_value(b["agent"].clone()).context("agent is {kind, session, process, cwd}")?;
            crate::chat::team::assistant::local_call(hub, &caller, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        // The turn tier: a coding agent's hook asks whether letters wait (`codync-host network hook`).
        "networkHook" => {
            use crate::network::sessions;
            let kind: codync_network::AgentKind = serde_json::from_value(b["kind"].clone()).context("kind")?;
            let ask = crate::chat::team::mailbox::HookAsk {
                told: b["told"].as_str(),
                failed: b["failed"].as_str(),
                tier: if b["push"] == true { sessions::Tier::Push } else { sessions::Tier::Turn },
                wait: std::time::Duration::from_secs(b["waitSecs"].as_u64().unwrap_or(0)),
            };
            crate::chat::team::mailbox::hook(hub, kind, str_arg(&b, "session")?, ask).await?
        }
        "teamCall" => {
            let (caller, name) = (str_arg(&b, "botId")?, str_arg(&b, "name")?);
            let bot = work::acting_as(hub, caller, name)?;
            crate::chat::team::call_async(hub, &bot, caller, name, &b["arguments"]).await?
        }
        "manageCall" => {
            let (bot, name) = (str_arg(&b, "botId")?, str_arg(&b, "name")?);
            work::refuse_in_thread(hub, bot, name)?;
            crate::chat::manage::call(hub, bot, name, &b["arguments"]).await?
        }
        // A thread the user starts themselves: its first message, as a task of the bot's box, or with no `botId`
        // of the computer's box of threads without a bot, on the agent its setup names.
        "startThread" => {
            let text = str_arg(&b, "text")?.to_owned();
            // One that can't be read is refused: a thread never starts on other approvals than the user chose.
            let setup: work::Setup = match &b["setup"] {
                Value::Null => work::Setup::default(),
                setup => serde_json::from_value(setup.clone()).context("a thread's setup")?,
            };
            let bot = match b["botId"].as_str() {
                Some(bot) => bot.to_owned(),
                None if setup.backend.is_some() => work::plain::owner(hub).await?,
                None => bail!("a thread without a bot names its agent"),
            };
            // Its files, uploaded to that bot (or the box) first.
            let uploads: Vec<String> = attachment_ids(&b).into_iter().map(str::to_owned).collect();
            let (hub, owner) = (hub.clone(), bot.clone());
            let id =
                tokio::task::spawn_blocking(move || work::add_thread(&hub, &owner, &text, &uploads, setup)).await??;
            // The box a thread without a bot went to is named, so the app can show it.
            json!({"botId": bot, "taskId": id})
        }
        // Ended threads older than the bots' `work` carries (of `botId`, else of every bot), newest first, a page at a
        // time: `before` the `endedAt` of the oldest the app has.
        "olderThreads" => {
            let (before, only) = (b["before"].as_i64(), b["botId"].as_str().map(str::to_owned));
            let hub = hub.clone();
            tokio::task::spawn_blocking(move || work::archive::older(&hub, only.as_deref(), before)).await??
        }
        // A thread's transcript, for its details: no chat an app holds has it.
        "threadEntries" => work::entries(hub, str_arg(&b, "botId")?, str_arg(&b, "threadId")?, 500)?,
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
        // Bots from another app's export file on this computer (`import`).
        "previewImport" => crate::import::preview(&b).await?,
        "importBots" => crate::import::apply(hub, &b).await?,
        "importSkill" => json!({"name": "codync-export", "text": crate::import::EXPORT_SKILL}),
        "updateBot" => {
            let hub = hub.clone();
            let updated = tokio::task::spawn_blocking(move || {
                let bot = hub.update_bot(&b)?;
                // Its `maxThreads` may have made room.
                work::dispatch(&hub, b["id"].as_str().unwrap_or_default());
                anyhow::Ok(bot)
            });
            json!({"bot": updated.await??})
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
        // A terminal where the bot's code work is (its default project, else its own folder): the user's own shell there (`termInput` / `termResize` / `termClose` and
        // `/term/<id>` as for a setup terminal), one per app tab (`tab`, optional). Opening it again while it runs returns the same one.
        "botTerminal" => {
            let bot_id = str_arg(&b, "botId")?;
            let cfg = hub.store.bot(bot_id)?.filter(|row| !row.deleted).map(|row| row.config).context("unknown bot")?;
            let (cols, rows) = super::host::term_size(&b);
            let folder = std::path::Path::new(crate::agent::workspace::work_folder(&cfg));
            json!({"term": hub.terms.shell(bot_id, tab_arg(&b)?, folder, cols, rows)?})
        }
        // The app closed terminal tab `tab`: its shells end, on every bot it was shown for.
        "botTerminalClose" => {
            hub.terms.close_shells(tab_arg(&b)?.context("`tab` is required")?);
            json!({})
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
        // One chunk of a file for a later `send` (`attachments`); base64 `data` at `offset`.
        "upload" => {
            use base64::Engine as _;
            // Without a bot: a file a thread without one starts with, in the computer's box of them.
            let bot = match b["botId"].as_str() {
                Some(bot) => bot.to_owned(),
                None => work::plain::owner(hub).await?,
            };
            // A file for an archived thread's message: the thread comes back first, as the message brings it.
            if let Some(task) = b["taskId"].as_str() {
                work::archive::bring_back(hub, &bot, task)?;
            }
            let root = uploads_root(hub, &bot, b["taskId"].as_str())?;
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
            let root = uploads_root(hub, str_arg(&b, "botId")?, b["taskId"].as_str())?;
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
            let uploads = attachment_ids(&b);
            if text.is_empty() && uploads.is_empty() {
                bail!("empty message");
            }
            let nonce = b["clientNonce"].as_str().unwrap_or_default();
            // `taskId`: to the thread of one of the bot's own tasks, where a running turn makes it wait as here.
            if let Some(task) = b["taskId"].as_str() {
                if !b["replyTo"].is_null() || !b["call"].is_null() {
                    return Err(work::ThreadRefused::error("unsupported", "a thread takes no quotes or calls".into()));
                }
                let hub = hub.clone();
                let (bot, task, text, nonce) = (bot.to_owned(), task.to_owned(), text.to_owned(), nonce.to_owned());
                let uploads: Vec<String> = uploads.into_iter().map(str::to_owned).collect();
                let sent =
                    tokio::task::spawn_blocking(move || work::message(&hub, &bot, &task, &text, &uploads, &nonce));
                return Ok(ControlFlow::Break(sent.await??));
            }
            if !nonce.is_empty()
                && let Some(e) = hub.store.find_by_nonce(bot, nonce)
            {
                return Ok(ControlFlow::Break(json!({"entry": e})));
            }
            // A thread is written to through its bot and task (`taskId`), never by its own id.
            let row = hub.store.bot(bot)?.filter(|r| !r.deleted && !r.config.is_thread());
            let row = row.ok_or_else(|| anyhow!("unknown bot"))?;
            // `replyTo`: the message of this chat it answers, and `replyPart` the text selected in it (`chat::replies`).
            let reply = match b["replyTo"].as_str().filter(|id| !id.is_empty()) {
                Some(id) => Some(crate::chat::replies::quote(&hub.store, bot, id, b["replyPart"].as_str())?),
                None => None,
            };
            let attachments = crate::chat::uploads::resolve_all(&crate::chat::uploads::root(&row.config), &uploads)?;
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
                Some(row) if row.config.is_group() => crate::chat::group::stop(hub, bot),
                _ => hub.send_cmd(bot, Cmd::Stop)?,
            }
            json!({})
        }
        // A message of the user's waiting for its turn: handled now (`steer`) or taken back (`unqueue`).
        "steer" | "unqueue" => {
            let (bot, entry_id) = (str_arg(&b, "botId")?, str_arg(&b, "entryId")?.to_owned());
            // `taskId`: the message waits in the thread of one of the bot's own tasks. The caller names the task,
            // never its thread.
            let thread = b["taskId"].as_str().map(|task| work::thread_of(hub, bot, task)).transpose()?;
            if thread.is_none() && crate::store::is_thread_id(bot) {
                bail!("unknown bot");
            }
            let bot = thread.as_deref().unwrap_or(bot);
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
        // A press on a decision card (`request_decision`): the choice is the bot's answer and the user's message.
        "respondDecision" => {
            crate::chat::decision::respond(hub, str_arg(&b, "entryId")?, str_arg(&b, "option")?).await?
        }
        "respondPermission" => {
            let entry_id = str_arg(&b, "entryId")?;
            let e = hub.store.entry(entry_id).ok_or_else(|| anyhow!("unknown card"))?;
            let option_id = b["optionId"].as_str().map(str::to_owned);
            hub.send_cmd(work::card_actor(&e), Cmd::Permission { entry_id: entry_id.to_owned(), option_id })?;
            json!({})
        }
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}
