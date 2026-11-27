//! The thread tools: what an outside app sees of this computer's threads, and starts, writes to, waits on, reads and
//! steers. A thread is named by its bot and task (`{botId, taskId}`); one the caller may not see is `not_found`.

use super::access::{Admitted, Code, Level, Refusal, Scope, within};
use super::clients::{self, Principal};
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
const MAX_MESSAGE: usize = 32_000;
const PAGE: usize = 20;

/// One admitted call.
pub struct Caller<'a> {
    pub principal: &'a Principal,
    /// Made only by `access::admit`: no handler runs for a call that didn't pass it.
    pub admitted: &'a Admitted,
}

impl Caller<'_> {
    /// The level it acts under.
    pub fn ceiling(&self) -> Level {
        self.admitted.ceiling
    }
}

fn invalid(why: impl Into<String>) -> anyhow::Error {
    Refusal::err(Code::InvalidRequest, why)
}

fn not_found() -> anyhow::Error {
    Refusal::err(Code::NotFound, "no such thread you may see")
}

fn denied(why: impl Into<String>) -> anyhow::Error {
    Refusal::err(Code::CapabilityDenied, why)
}

fn text<'a>(args: &'a Value, field: &str) -> Result<&'a str> {
    args[field].as_str().filter(|t| !t.trim().is_empty()).ok_or_else(|| invalid(format!("`{field}` is required")))
}

/// A bot whose threads `p` may reach: a live bot, or the computer's box of threads without one.
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

/// The bots `p` may reach, by id and name.
fn reachable(hub: &Hub, p: &Principal) -> Vec<BotConfig> {
    let rows = hub.store.bots().unwrap_or_default().into_iter().filter(|r| !r.deleted).map(|r| r.config);
    rows.filter(|c| !c.plain && !c.hidden && !c.is_thread() && !c.is_group() && p.reaches(c)).collect()
}

pub async fn capabilities(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let p = me.principal;
    let mut agents: Vec<Value> = codync_agent::backends::list()
        .into_iter()
        .filter(|a| a["installed"] == true)
        .map(|a| json!({"id": a["id"], "name": a["name"], "installed": a["installed"], "signedIn": a["signedIn"]}))
        .collect();
    crate::agent::login_profiles::annotate(&hub.store, &mut agents);
    for agent in &mut agents {
        // Only what picking a login needs.
        let profiles: Vec<Value> = agent["loginProfiles"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| json!({"id": p["id"], "name": p["name"]}))
            .collect();
        *agent = json!({"id": agent["id"], "name": agent["name"], "installed": agent["installed"],
            "signedIn": agent["signedIn"], "loginProfiles": profiles});
    }
    let bots: Vec<Value> = reachable(hub, p)
        .iter()
        .map(|c| json!({"botId": c.id, "name": c.name, "agent": c.backend, "folder": c.cwd, "projects": c.projects}))
        .collect();
    let mut out = json!({
        "you": {
            "name": p.label, "level": me.ceiling(), "approved": p.approved,
            "computerMaximum": clients::max_access(&hub.store), "scopes": p.scopes,
            "threadsWithoutABot": p.plain_threads, "folders": p.folders,
        },
        "bots": bots,
        "agents": agents,
        "approvals": {
            "ask": "every request is an approval card on the user's devices (needs level ask)",
            "auto": "the agent's own auto mode; what it asks for is a card (needs level auto)",
            "full": "requests approved by themselves, in the agent mode you pick (needs level full)",
        },
        "approvalsAnsweredHere": false,
        "limits": {
            "callsPerMinute": clients::CALLS_PER_MINUTE, "liveThreads": MAX_LIVE, "messageBytes": MAX_MESSAGE,
            "waitSecs": if p.cloud { WAIT_CLOUD_SECS } else { WAIT_LOCAL_SECS },
        },
    });
    if let Some(agent) = args["agent"].as_str().filter(|a| !a.is_empty()) {
        if p.cloud {
            out["models"] = Value::Null;
            out["note"] = "Model lists aren't fetched over the cloud: leave model out for the agent's default.".into();
        } else {
            out["models"] = crate::agent::auth::models(&hub.store, agent, None).await?["models"].clone();
        }
    }
    Ok(out)
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
    let mut boxes: Vec<BotConfig> = if let Some(bot) = args["botId"].as_str() {
        bot_of(hub, bot).into_iter().collect()
    } else {
        let mut all = reachable(hub, p);
        all.extend(hub.store.bots()?.into_iter().filter(|r| !r.deleted && r.config.plain).map(|r| r.config));
        all
    };
    boxes.retain(|cfg| if cfg.plain { p.plain_threads } else { p.reaches(cfg) });
    let wanted = |cfg: &BotConfig, row: &Value| {
        sees(p, cfg, external::client_of(row)) && status.is_none_or(|s| row["status"] == s)
    };
    let mut rows: Vec<Value> = Vec::new();
    match args["cursor"].as_str() {
        // Older ended ones, from the archive's pages.
        Some(cursor) => {
            let before = cursor.parse::<i64>().map_err(|_| invalid("cursor is next from thread_list"))?;
            for cfg in &boxes {
                let older = work::archive::older(hub, Some(&cfg.id), Some(before))?;
                let page = older["threads"].as_array().cloned().unwrap_or_default();
                rows.extend(page.iter().filter(|row| wanted(cfg, row)).map(|row| shown(cfg, row)));
            }
            rows.sort_by_key(|r| std::cmp::Reverse(r["endedAt"].as_i64()));
        }
        None => {
            for cfg in &boxes {
                rows.extend(external::rows(hub, cfg).iter().filter(|row| wanted(cfg, row)).map(|row| shown(cfg, row)));
            }
        }
    }
    let more = rows.len() > limit;
    rows.truncate(limit);
    // The next page starts before the oldest end shown; the first page also leads to the ones it didn't carry.
    let oldest = rows.iter().filter_map(|r| r["endedAt"].as_i64()).min();
    let next = match (args["cursor"].is_null(), more) {
        (_, true) => oldest,
        (true, false) => Some(oldest.unwrap_or_else(crate::store::now_ms)),
        (false, false) => None,
    };
    Ok(json!({"threads": rows, "next": next.map(|n| n.to_string())}))
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

pub async fn launch(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let p = me.principal;
    let message = text(args, "message")?.to_owned();
    if message.len() > MAX_MESSAGE {
        return Err(invalid(format!("a message of at most {MAX_MESSAGE} bytes")));
    }
    let approval = match args["approval"].as_str() {
        None => Level::Ask,
        Some(word) => match Level::parse(word) {
            Some(level) if level >= Level::Ask => level,
            _ => return Err(invalid("approval: ask, auto or full")),
        },
    };
    within(approval, me.ceiling(), "A thread with that approval")?;
    let mode = args["mode"].as_str().filter(|m| !m.is_empty()).map(str::to_owned);
    let options: std::collections::BTreeMap<String, String> = match &args["modelOptions"] {
        Value::Null => std::collections::BTreeMap::default(),
        v => serde_json::from_value(v.clone()).map_err(|_| invalid("modelOptions: names and values, as text"))?,
    };
    // The agent's own mode, and model settings that may set it, are what only Full may pick.
    if (mode.is_some() || !options.is_empty()) && (approval < Level::Full || me.ceiling() < Level::Full) {
        return Err(Refusal::err(
            Code::ModeEscalationDenied,
            format!(
                "mode and modelOptions can set how freely the agent works: only approval full may pick them. {}",
                super::access::ACT_DIRECTLY
            ),
        ));
    }
    let cfg = match args["botId"].as_str() {
        Some(bot) => {
            let cfg = bot_of(hub, bot).filter(|c| !c.plain && p.reaches(c)).ok_or_else(not_found)?;
            Some(cfg)
        }
        None if !p.plain_threads => return Err(denied("this app may only start threads on bots")),
        None if args["agent"].as_str().is_none() => return Err(invalid("a thread without a bot names its agent")),
        None => None,
    };
    let cwd = args["folder"].as_str().map(|f| folder(hub, p, cfg.as_ref(), f)).transpose()?;
    let place = match args["place"].as_str() {
        None => None,
        Some(word) => Some(serde_json::from_value(json!(word)).map_err(|_| invalid("place: here or worktree"))?),
    };
    let model = args["model"].as_str().map(|id| work::Model { id: Some(id.to_owned()), options: options.clone() });
    let model = model.or_else(|| (!options.is_empty()).then_some(work::Model { id: None, options }));
    let setup = work::Setup {
        backend: args["agent"].as_str().map(str::to_owned),
        login_profile: args["loginProfile"].as_str().map(str::to_owned),
        cwd,
        model,
        approval: Some(work::Approval { permission: approval.permission(), mode }),
        workflow: None,
        place,
    };
    let bot = match &cfg {
        Some(cfg) => cfg.id.clone(),
        None => work::plain::owner(hub).await?,
    };
    let (hub2, client, key) = (hub.clone(), p.id.clone(), args["clientRequestId"].as_str().map(str::to_owned));
    let (started, owner) = (by(me, approval), bot.clone());
    let (launched, replayed) = tokio::task::spawn_blocking(move || {
        clients::once(&hub2.store, &client, key.as_deref(), || {
            if external::live(&hub2.store, &client) >= MAX_LIVE {
                return Err(Refusal::err(
                    Code::RateLimited,
                    format!("{MAX_LIVE} of this app's threads are still going: wait for one to end, or stop one"),
                ));
            }
            let task = external::launch(&hub2, &owner, &message, setup, &started)?;
            Ok(json!({"botId": owner, "taskId": task}))
        })
    })
    .await??;
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
    let message = text(args, "message")?.to_owned();
    if message.len() > MAX_MESSAGE {
        return Err(invalid(format!("a message of at most {MAX_MESSAGE} bytes")));
    }
    let steer = match args["mode"].as_str() {
        None | Some("queue" | "auto") => false,
        Some("steer") => true,
        Some(_) => return Err(invalid("mode: queue or steer")),
    };
    let (hub2, started, ceiling) = (hub.clone(), by(me, me.ceiling()), me.ceiling());
    let (client, key) = (me.principal.id.clone(), args["clientRequestId"].as_str().map(str::to_owned));
    let bot = cfg.id.clone();
    let (sent, replayed) = tokio::task::spawn_blocking(move || {
        clients::once(&hub2.store, &client, key.as_deref(), || {
            // Again under the box's lock: the thread's level can't change between the check and the message.
            external::message(&hub2, &bot, &task, &message, &started, |t| within(t.level, ceiling, "That thread"))
        })
    })
    .await??;
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

/// Where `task` of `bot` stands now, with a cursor that changes whenever any of it does.
fn state(hub: &Hub, bot: &str, task: &str) -> Option<(Target, String, Vec<Value>, String)> {
    let target = external::target(hub, bot, task)?;
    let activity = target.thread.as_deref().map(|t| hub.runtime(t).activity).unwrap_or_default();
    let needs = needs(hub, bot, target.thread.as_deref());
    let last = target.thread.as_deref().and_then(|t| hub.store.history(t, i64::MAX, 1).ok()?.pop()).map(|e| e.rev);
    let cursor = format!("{}:{}:{}:{}", json!(target.status), last.unwrap_or(0), needs.len(), activity.len());
    Some((target, activity, needs, cursor))
}

pub async fn wait(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    let (cfg, task, _) = resolve(hub, me.principal, args)?;
    let most = if me.principal.cloud { WAIT_CLOUD_SECS } else { WAIT_LOCAL_SECS };
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
        .map(|e| json!({"seq": e.seq, "kind": e.kind, "at": e.created_at, "text": clip(&entry_text(e))}))
        .collect();
    Ok(json!({"thread": summary, "steps": steps, "next": next}))
}

pub async fn steer(hub: &Arc<Hub>, me: &Caller<'_>, args: &Value) -> Result<Value> {
    // ponytail: the level is checked just before the box's lock, not under it; a thread's setup changing in between
    // only matters for a write (thread_send checks under the lock).
    let (cfg, task, _) = resolve_to_act(hub, me, args)?;
    let how = text(args, "how")?.to_owned();
    let (hub2, bot) = (hub.clone(), cfg.id.clone());
    tokio::task::spawn_blocking(move || match how.as_str() {
        "first" => work::first(&hub2, &bot, &task),
        "now" => work::cut_in(&hub2, &bot, &task, None),
        // One task only: never every one of the box.
        "stop" => work::stop(&hub2, &bot, Some(&task)),
        "retry" => work::retry(&hub2, &bot, &task),
        "drop" => work::drop_task(&hub2, &bot, &task),
        _ => Err(invalid("how: first, now, stop, retry or drop")),
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
                    "that is an approval request: the user answers it on their devices. {}",
                    super::access::ACT_DIRECTLY
                )));
            }
            let option = text(args, "option")?;
            crate::chat::decision::respond(hub, &found.id, option).await
        }
        _ => Err(invalid("action: list, read or answer")),
    }
}

#[cfg(test)]
mod tests;
