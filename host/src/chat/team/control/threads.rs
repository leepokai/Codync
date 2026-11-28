//! The thread tools: what an outside app sees of this computer's threads, and starts, writes to, waits on, reads and
//! steers. A thread is named by its bot and task (`{botId, taskId}`); one the caller may not see is `not_found`.

use super::access::{ACT_DIRECTLY, Code, Level, Refusal, Scope, within};
use super::clients::{self, Principal};
use super::{Caller, calls, denied, invalid, reachable, text, usable};
use crate::agent::bot::Cmd;
use crate::chat::work::{self, external, external::StartedBy, external::Target};
use crate::hub::Hub;
use crate::store::{BotConfig, Entry};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Threads one app may have going at once (waiting or working), on the whole computer.
pub const MAX_LIVE: usize = 8;
/// How long `thread_wait` waits at most: on this computer, and through the cloud, which gives a call 10 seconds.
const WAIT_LOCAL_SECS: u64 = crate::chat::team::mailbox::WAIT_MAX_SECS;
const WAIT_CLOUD_SECS: u64 = 8;
pub const MAX_MESSAGE: usize = 32_000;
const PAGE: usize = 20;
/// Archive pages one `thread_list` call looks through for threads the caller sees.
const SCAN_PAGES: usize = 10;

fn not_found() -> anyhow::Error {
    Refusal::err(Code::NotFound, "no such thread you may see")
}

/// A box of threads: a live bot's, or the computer's box of threads without one (who sees which is [`sees`]).
fn bot_of(hub: &Hub, bot: &str) -> Option<BotConfig> {
    let cfg = hub.store.bot(bot).ok().flatten().filter(|r| !r.deleted)?.config;
    (!cfg.is_thread() && !cfg.is_group()).then_some(cfg)
}

/// Whether `p` sees a thread of `cfg`'s that `client` started (none: the user or the bot did). Its own, while it may
/// still reach where it is; another's only on a bot (never a thread without one) with the all-threads scope.
fn sees(p: &Principal, cfg: &BotConfig, client: Option<&str>) -> bool {
    if cfg.plain {
        return client == Some(p.id.as_str()) && p.plain_threads;
    }
    p.reaches(cfg) && (client == Some(p.id.as_str()) || p.may(Scope::AllThreads))
}

/// The thread `{botId, taskId}` names, if the caller sees it.
fn resolve(hub: &Hub, p: &Principal, args: &Value) -> Result<(BotConfig, String, Target)> {
    let (bot, task) = (text(args, "botId")?, text(args, "taskId")?);
    let cfg = bot_of(hub, bot).ok_or_else(not_found)?;
    let target = external::target(hub, bot, task).ok_or_else(not_found)?;
    if !sees(p, &cfg, target.started_by.as_ref().map(|by| by.client.as_str())) {
        return Err(not_found());
    }
    Ok((cfg, task.to_owned(), target))
}

/// [`resolve`], for a call that acts on the thread: its level must be within the caller's.
fn resolve_to_act(hub: &Hub, me: &Caller, args: &Value) -> Result<(BotConfig, String, Target)> {
    let found = resolve(hub, me.principal, args)?;
    within(found.2.level, me.ceiling(), "That thread")?;
    Ok(found)
}

fn by(me: &Caller, level: Level) -> StartedBy {
    StartedBy { client: me.principal.id.clone(), label: me.principal.label.clone(), level }
}

/// How long `thread_wait` waits for `p` at most.
pub fn wait_secs(p: &Principal) -> u64 {
    if p.cloud { WAIT_CLOUD_SECS } else { WAIT_LOCAL_SECS }
}

/// Refuses another thread to app `client` while [`MAX_LIVE`] of its own are going. Under [`calls::gate`].
fn room(hub: &Hub, client: &str) -> Result<()> {
    if external::live(&hub.store, client) >= MAX_LIVE {
        return Err(Refusal::err(
            Code::RateLimited,
            format!("{MAX_LIVE} of this app's threads are still going: wait for one to end, or stop one"),
        ));
    }
    Ok(())
}

/// A thread row as an app reads it.
fn shown(cfg: &BotConfig, row: &Value) -> Value {
    json!({
        "botId": cfg.id, "bot": if cfg.plain { Value::Null } else { cfg.name.clone().into() },
        "taskId": row["id"], "title": row["title"], "status": row["status"], "activity": row["activity"],
        "wait": row["wait"], "startedAt": row["startedAt"], "endedAt": row["endedAt"], "result": row["result"],
        "agent": row["runsOn"], "folder": row["folder"], "startedBy": row["startedBy"]["label"],
    })
}

pub async fn list(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let p = me.principal.clone();
    let (hub, args) = (hub.clone(), args.clone());
    tokio::task::spawn_blocking(move || list_now(&hub, &p, &args)).await?
}

fn list_now(hub: &Arc<Hub>, p: &Principal, args: &Value) -> Result<Value> {
    let limit = usize::try_from(args["limit"].as_u64().unwrap_or(PAGE as u64)).unwrap_or(PAGE).clamp(1, 50);
    let status = args["status"].as_str();
    let only = args["botId"].as_str();
    let mut boxes: Vec<BotConfig> = if let Some(bot) = only {
        bot_of(hub, bot).into_iter().collect()
    } else {
        let mut all = reachable(hub, p);
        all.extend(hub.store.bots()?.into_iter().filter(|r| !r.deleted && r.config.plain).map(|r| r.config));
        all
    };
    boxes.retain(|cfg| if cfg.plain { p.plain_threads } else { p.reaches(cfg) });
    let seen = |row: &Value| {
        let cfg = boxes.iter().find(|cfg| row["botId"] == cfg.id.as_str())?;
        let wanted = sees(p, cfg, external::client_of(row)) && status.is_none_or(|s| row["status"] == s);
        wanted.then(|| shown(cfg, row))
    };
    let (rows, next) = match args["cursor"].as_str() {
        None => {
            let rows = boxes.iter().flat_map(|cfg| {
                external::rows(hub, cfg).into_iter().map(|mut row| {
                    row["botId"] = cfg.id.clone().into();
                    row
                })
            });
            first_page(rows.filter_map(|row| seen(&row)).collect(), limit)
        }
        Some(cursor) => {
            let before = cursor.parse::<i64>().map_err(|_| invalid("cursor is next from thread_list"))?;
            older_page(hub, only, before, limit, seen)?
        }
    };
    Ok(json!({"threads": rows, "next": next.map(|n| n.to_string())}))
}

/// Keeps the first `keep` of `rows` (newest end first) and every other that ended with the last of them, so a page
/// never splits the ones that ended at once. Where the next page starts, when some went.
fn cut(rows: &mut Vec<Value>, keep: usize) -> Option<i64> {
    if rows.len() <= keep {
        return None;
    }
    let edge = rows.get(keep.checked_sub(1)?)?["endedAt"].as_i64()?;
    rows.retain(|r| r["endedAt"].as_i64() >= Some(edge));
    Some(edge)
}

/// The first page: every live thread, then the latest ended ones up to `limit`. It always leads on, to the ones it
/// left out and the archive.
fn first_page(rows: Vec<Value>, limit: usize) -> (Vec<Value>, Option<i64>) {
    let (mut live, mut ended): (Vec<Value>, Vec<Value>) = rows.into_iter().partition(|r| r["endedAt"].is_null());
    live.sort_by_key(|r| std::cmp::Reverse(r["startedAt"].as_i64()));
    ended.sort_by_key(|r| std::cmp::Reverse(r["endedAt"].as_i64()));
    let keep = limit.saturating_sub(live.len());
    if keep == 0 {
        ended.clear();
    }
    let next = cut(&mut ended, keep)
        .or_else(|| ended.last().and_then(|r| r["endedAt"].as_i64()))
        .unwrap_or_else(crate::store::now_ms);
    live.extend(ended);
    (live, Some(next))
}

/// Ended threads before `before` the caller sees (`seen`), from the archive's pages, until `limit` of them or
/// [`SCAN_PAGES`] pages; where the next page starts, while more may come.
fn older_page(
    hub: &Hub,
    only: Option<&str>,
    mut before: i64,
    limit: usize,
    seen: impl Fn(&Value) -> Option<Value>,
) -> Result<(Vec<Value>, Option<i64>)> {
    let mut rows = Vec::new();
    for _ in 0..SCAN_PAGES {
        let (page, more) = work::archive::older_page(hub, only, before)?;
        rows.extend(page.iter().filter_map(&seen));
        let edge = page.last().and_then(|r| r["endedAt"].as_i64());
        match edge {
            Some(edge) if more => before = edge,
            _ => {
                let next = cut(&mut rows, limit);
                return Ok((rows, next));
            }
        }
        if rows.len() >= limit {
            break;
        }
    }
    // More may come past what was looked at: from there, or from the last row shown.
    let next = cut(&mut rows, limit).unwrap_or(before);
    Ok((rows, Some(next)))
}

/// What `folder` resolves to for a launch on `cfg` (`None`: a thread without a bot), if the caller may work there:
/// one of the bot's projects or its own folder, a project of a bot it may reach, or a folder its grant names.
fn folder(hub: &Hub, p: &Principal, cfg: Option<&BotConfig>, folder: &str) -> Result<String> {
    let real = clients::allowed_folder(folder).map_err(|e| invalid(format!("{e:#}")))?;
    let real_path = Path::new(&real);
    let canonical = |f: &String| std::fs::canonicalize(f).ok();
    let mut allowed: Vec<PathBuf> = p.folders.clone();
    if let Some(cfg) = cfg {
        allowed.extend(cfg.projects.iter().chain([&cfg.cwd]).filter_map(canonical));
    }
    for bot in reachable(hub, p) {
        allowed.extend(bot.projects.iter().filter_map(canonical));
    }
    if !allowed.iter().any(|a| real_path.starts_with(a)) {
        return Err(denied(format!(
            "{folder} isn't a folder this app may work in: one of the bot's projects, or a folder the user gave it in \
             Codync's settings"
        )));
    }
    Ok(real)
}

/// The call's `message`, within [`MAX_MESSAGE`].
fn message(args: &Value) -> Result<String> {
    let message = text(args, "message")?;
    if message.len() > MAX_MESSAGE {
        return Err(invalid(format!("a message of at most {MAX_MESSAGE} bytes")));
    }
    Ok(message.to_owned())
}

/// What a launch asks for, before where it works: the level it runs at, and its setup (no folder yet), refused when
/// it asks for more than a caller under `ceiling` may.
fn setup_of(ceiling: Level, args: &Value) -> Result<(Level, work::Setup)> {
    let approval = match args["approval"].as_str() {
        None => Level::Ask,
        Some(word) => match Level::parse(word) {
            Some(level) if level >= Level::Ask => level,
            _ => return Err(invalid("approval: ask, auto or full")),
        },
    };
    within(approval, ceiling, "A thread with that approval")?;
    let mode = args["mode"].as_str().filter(|m| !m.is_empty()).map(str::to_owned);
    let options: std::collections::BTreeMap<String, String> = match &args["modelOptions"] {
        Value::Null => std::collections::BTreeMap::default(),
        v => serde_json::from_value(v.clone()).map_err(|_| invalid("modelOptions: names and values, as text"))?,
    };
    // The agent's own mode, and model settings that may set it, are what only Full may pick.
    if (mode.is_some() || !options.is_empty()) && approval < Level::Full {
        return Err(Refusal::err(
            Code::ModeEscalationDenied,
            format!(
                "mode and modelOptions can set how freely the agent works: only approval full may pick them. \
                 {ACT_DIRECTLY}"
            ),
        ));
    }
    let place = match args["place"].as_str() {
        None => None,
        Some(word) => Some(serde_json::from_value(json!(word)).map_err(|_| invalid("place: here or worktree"))?),
    };
    let model = args["model"].as_str().map(|id| work::Model { id: Some(id.to_owned()), options: options.clone() });
    let model = model.or_else(|| (!options.is_empty()).then_some(work::Model { id: None, options }));
    let setup = work::Setup {
        backend: args["agent"].as_str().map(str::to_owned),
        login_profile: args["loginProfile"].as_str().map(str::to_owned),
        cwd: None,
        model,
        approval: Some(work::Approval { permission: approval.permission(), mode }),
        workflow: None,
        place,
    };
    Ok((approval, setup))
}

pub async fn launch(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let p = me.principal;
    let message = message(args)?;
    let (approval, mut setup) = setup_of(me.ceiling(), args)?;
    let cfg = match args["botId"].as_str() {
        Some(bot) => Some(usable(hub, p, bot).ok_or_else(not_found)?),
        None if !p.plain_threads => return Err(denied("this app may only start threads on bots")),
        None if setup.backend.is_none() => return Err(invalid("a thread without a bot names its agent")),
        None => None,
    };
    setup.cwd = args["folder"].as_str().map(|f| folder(hub, p, cfg.as_ref(), f)).transpose()?;
    let bot = match &cfg {
        Some(cfg) => cfg.id.clone(),
        None => work::plain::owner(hub).await?,
    };
    let (started, owner) = (by(me, approval), bot.clone());
    let (launched, replayed) = calls::once_blocking(hub, me, "thread_launch", args, move |hub| {
        room(hub, &started.client)?;
        let task = external::launch(hub, &owner, &message, setup, &started)?;
        Ok(json!({"botId": owner, "taskId": task}))
    })
    .await?;
    let properties = json!({"agent": args["agent"].as_str().or(cfg.as_ref().map(|c| c.backend.as_str())),
        "plain": cfg.is_none(), "folder": args["folder"].is_string(), "approval": approval, "replayed": replayed,
        "cloud": p.cloud});
    crate::analytics::capture(
        hub,
        crate::analytics::Event::McpThreadLaunched,
        properties.as_object().cloned().unwrap_or_default(),
    );
    let status = launched["taskId"].as_str().and_then(|t| external::target(hub, &bot, t)).map(|t| t.status);
    Ok(json!({"thread": launched, "status": status, "replayed": replayed,
        "note": "It runs on its own: follow it with thread_wait, then thread_read. Approvals are the user's, in Codync."}))
}

pub async fn send(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let (cfg, task, _) = resolve_to_act(hub, me, args)?;
    let message = message(args)?;
    let steer = match args["mode"].as_str() {
        None | Some("queue" | "auto") => false,
        Some("steer") => true,
        Some(_) => return Err(invalid("mode: queue or steer")),
    };
    // A retry with the same `clientRequestId` is the same message (the thread's own nonce), never a second one.
    let key = args["clientRequestId"].as_str().filter(|k| !k.is_empty());
    if key.is_some_and(|k| k.len() > 200) {
        return Err(invalid("clientRequestId is at most 200 characters"));
    }
    let nonce = key.map(|k| calls::request_key(me.principal, "thread_send", k)).unwrap_or_default();
    let (hub2, started, ceiling, bot) = (hub.clone(), by(me, me.ceiling()), me.ceiling(), cfg.id.clone());
    let sent = tokio::task::spawn_blocking(move || {
        let _g = calls::gate();
        // Again under the box's lock: the thread's level can't change between the check and the message, and one it
        // reopens counts among the app's.
        external::message(&hub2, &bot, &task, &message, &nonce, &started, |t| {
            within(t.level, ceiling, "That thread")?;
            if t.status.ended() && t.started_by.as_ref().is_some_and(|b| b.client == started.client) {
                room(&hub2, &started.client)?;
            }
            Ok(())
        })
    })
    .await??;
    let replayed = key.is_some() && sent["reopened"].is_null();
    if steer && !replayed && sent["queued"] == true {
        let thread = external::target(hub, &cfg.id, args["taskId"].as_str().unwrap_or_default()).and_then(|t| t.thread);
        if let (Some(thread), Some(entry)) = (thread, sent["entry"]["id"].as_str()) {
            hub.send_cmd(&thread, Cmd::Steer { entry_id: entry.to_owned() })?;
        }
    }
    Ok(json!({"entryId": sent["entry"]["id"], "queued": sent["queued"], "reopened": sent["reopened"],
        "replayed": replayed}))
}

/// What waits on the user in `thread`: its open approval and choice cards, as an app may read them.
fn needs(hub: &Hub, bot: &str, thread: Option<&str>) -> Vec<Value> {
    let Some(thread) = thread else { return Vec::new() };
    let cards = hub.store.open_cards(bot, thread).unwrap_or_default();
    cards.iter().map(card).collect()
}

fn card(e: &Entry) -> Value {
    if crate::chat::decision::is_card(e) {
        json!({"requestId": e.id, "kind": "decision", "question": e.data["question"],
            "options": crate::chat::decision::choices(e), "at": e.created_at})
    } else {
        json!({"requestId": e.id, "kind": "permission", "summary": e.data["title"].as_str().or(e.data["text"].as_str()),
            "at": e.created_at,
            "note": "An approval request: the user answers it on their devices. It can't be answered through MCP."})
    }
}

/// The thread's newest entries a cursor covers: any of them changing (a tool's progress, a card answered) changes it.
const CURSOR_ENTRIES: i64 = 500;

/// Where `task` of `bot` stands now, with a cursor that changes whenever any of it does.
fn state(hub: &Hub, bot: &str, task: &str) -> Option<(Target, String, Vec<Value>, String)> {
    let target = external::target(hub, bot, task)?;
    let activity = target.thread.as_deref().map(|t| hub.runtime(t).activity).unwrap_or_default();
    let needs = needs(hub, bot, target.thread.as_deref());
    let entries =
        target.thread.as_deref().and_then(|t| hub.store.history(t, i64::MAX, CURSOR_ENTRIES).ok()).unwrap_or_default();
    let cursor = cursor_of(target.status, &entries, &needs, &activity);
    Some((target, activity, needs, cursor))
}

/// A digest of every part of a thread's state an app waits on: its status, each entry's revision, what waits on the
/// user and the whole activity line.
fn cursor_of(status: work::Status, entries: &[Entry], needs: &[Value], activity: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(json!(status).to_string());
    for e in entries {
        h.update(format!("|{}:{}", e.id, e.rev));
    }
    for n in needs {
        h.update(format!("|{}", n["requestId"]));
    }
    h.update(format!("|{activity}"));
    crate::chat::files::digest_hex(&h.finalize()[..12])
}

pub async fn wait(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let (cfg, task, _) = resolve(hub, me.principal, args)?;
    let most = wait_secs(me.principal);
    let wait = Duration::from_secs(args["timeoutSecs"].as_u64().unwrap_or(most).min(most));
    let deadline = tokio::time::Instant::now() + wait;
    let since = args["cursor"].as_str().map(str::to_owned);
    // Subscribed before the first look, so no change between the look and the wait is missed.
    let mut events = hub.events.subscribe();
    loop {
        let (target, activity, needs, cursor) = state(hub, &cfg.id, &task).ok_or_else(not_found)?;
        let settled = !matches!(target.status, work::Status::Waiting | work::Status::Working);
        let changed = since.as_deref().is_some_and(|c| c != cursor);
        let timed_out = tokio::time::Instant::now() >= deadline;
        if changed || (since.is_none() && settled) || timed_out {
            let mut out = json!({"status": target.status, "activity": activity, "needsYou": needs, "cursor": cursor,
                "timedOut": timed_out && !changed && !(since.is_none() && settled)});
            if target.status.ended() {
                out["result"] = target.result.map(|r| codync_agent::acp::truncate(&r, 4000)).into();
            }
            if out["timedOut"] == true {
                out["next"] = "Still going: call thread_wait again with this cursor.".into();
            }
            return Ok(out);
        }
        // ponytail: wakes on every event the host sends; filter by bot if many apps wait at once.
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        let _ = tokio::time::timeout(left.min(Duration::from_secs(2)), events.recv()).await;
    }
}

fn entry_text(e: &Entry) -> String {
    let d = &e.data;
    let words = d["text"].as_str().or(d["title"].as_str()).or(d["question"].as_str()).unwrap_or_default();
    words.to_owned()
}

pub fn read(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let (cfg, _, target) = resolve(hub, me.principal, args)?;
    let most = usize::try_from(args["maxCharsPerItem"].as_u64().unwrap_or(2000).clamp(100, 4000)).unwrap_or(2000);
    let clip = |s: &str| codync_agent::acp::truncate(s, most);
    let thread = target.thread.clone();
    let summary = json!({"title": target.title, "status": target.status, "startedAt": target.started_at,
        "endedAt": target.ended_at, "result": target.result.as_deref().map(clip),
        "startedBy": target.started_by.as_ref().map(|b| b.label.clone()),
        "needsYou": needs(hub, &cfg.id, thread.as_deref())});
    let Some(thread) = thread else {
        return Ok(json!({"thread": summary, "steps": [], "note": "It hasn't started yet."}));
    };
    if args["view"].as_str().unwrap_or("summary") == "summary" {
        let recent = hub.store.history(&thread, i64::MAX, 50)?;
        let reply = recent.iter().rev().find(|e| e.kind == "agent" && e.data["final"] == true).map(entry_text);
        return Ok(json!({"thread": summary, "lastReply": reply.as_deref().map(clip)}));
    }
    let limit = args["limit"].as_i64().unwrap_or(50).clamp(1, 100);
    let before = args["before"].as_i64().unwrap_or(i64::MAX);
    let steps = hub.store.history(&thread, before, limit)?;
    let next = (i64::try_from(steps.len()).unwrap_or(0) == limit).then(|| steps.first().map(|e| e.seq)).flatten();
    let steps: Vec<Value> = steps
        .iter()
        .map(|e| {
            let mut step = json!({"seq": e.seq, "kind": e.kind, "at": e.created_at, "text": clip(&entry_text(e))});
            // A tool's result (a failed command's error, say) and how it ended, held to the same size.
            if e.kind == "tool" {
                step["status"] = e.data["status"].clone();
                step["output"] = clip(e.data["output"].as_str().unwrap_or_default()).into();
            }
            step
        })
        .collect();
    Ok(json!({"thread": summary, "steps": steps, "next": next}))
}

pub async fn steer(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    // ponytail: the level is checked just before the box's lock, not under it; a thread's setup changing in between
    // only matters for a write (thread_send checks under the lock).
    let (cfg, task, target) = resolve_to_act(hub, me, args)?;
    let how = text(args, "how")?.to_owned();
    // A routine's run or another bot's request is the owner's work: ending it or running it again is theirs to do.
    if matches!(how.as_str(), "stop" | "retry" | "drop") && !target.own {
        return Err(denied(format!("that thread is a routine's run or another bot's request. {ACT_DIRECTLY}")));
    }
    let mine = target.started_by.filter(|b| b.client == me.principal.id).map(|b| b.client);
    let (hub2, bot) = (hub.clone(), cfg.id.clone());
    tokio::task::spawn_blocking(move || match how.as_str() {
        // To the front only: an app never makes other work give way.
        "first" => work::first(&hub2, &bot, &task),
        // One task only: never every one of the box.
        "stop" => work::stop(&hub2, &bot, Some(&task)),
        "retry" => {
            let _g = calls::gate();
            mine.as_deref().map_or(Ok(()), |client| room(&hub2, client))?;
            work::retry(&hub2, &bot, &task)
        }
        "drop" => work::drop_task(&hub2, &bot, &task),
        _ => Err(invalid("how: first, stop, retry or drop")),
    })
    .await??;
    let status = external::target(hub, &cfg.id, args["taskId"].as_str().unwrap_or_default()).map(|t| t.status);
    Ok(json!({"status": status}))
}

pub fn interrupt(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let (_, _, target) = resolve_to_act(hub, me, args)?;
    let thread = target.thread.context("it hasn't started: nothing runs")?;
    if hub.has_actor(&thread) {
        hub.send_cmd(&thread, Cmd::Stop)?;
    }
    Ok(json!({}))
}

pub async fn pending(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let (cfg, _, target) = resolve(hub, me.principal, args)?;
    let cards = target.thread.as_deref().map(|t| hub.store.open_cards(&cfg.id, t).unwrap_or_default());
    let cards = cards.unwrap_or_default();
    let named = || {
        let id = text(args, "requestId")?;
        cards.iter().find(|c| c.id == id).ok_or_else(|| Refusal::err(Code::NotFound, "nothing waits under that id"))
    };
    match args["action"].as_str().unwrap_or("list") {
        "list" => Ok(json!({"pending": cards.iter().map(card).collect::<Vec<_>>()})),
        "read" => Ok(card(named()?)),
        "answer" => {
            within(target.level, me.ceiling(), "That thread")?;
            let found = named()?;
            if !crate::chat::decision::is_card(found) {
                return Err(denied(format!(
                    "that is an approval request: the user answers it on their devices. {ACT_DIRECTLY}"
                )));
            }
            let option = text(args, "option")?;
            // Its choice is marked as its own, as its messages are: never taken for the user's words.
            crate::chat::decision::respond(hub, &found.id, option, Some(by(me, target.level))).await
        }
        _ => Err(invalid("action: list, read or answer")),
    }
}

#[cfg(test)]
mod tests;
