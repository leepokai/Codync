//! Durable bot routines, shared by API clients and the built-in MCP server.
mod check;
pub use check::{Checked, Folder};
mod editor;
pub use editor::{ScheduleDraft, preview as schedule_preview};
mod follow_up;
pub use follow_up::{Cancelled, FollowUp};
pub mod hooks;
use hooks::Refused;
mod kept;
mod mcp;
pub use mcp::{BRIEF, INSTRUCTIONS, call, tools};
mod progress;
mod retry;
mod runner;
mod schedule;
pub use runner::{Ran, STOPPED_BY_USER, TIMED_OUT, could_not_start, did_not_finish, start};
pub use schedule::{Trigger, local_zone};

use crate::{
    LockExt,
    hub::Hub,
    store::{EntryKind, Lane, Store, now_ms},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

const STATE_KEY: &str = "routines.v1";
const MAX_FINISHED_RUNS: usize = 1000;
fn default_timeout() -> u64 {
    3600
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: String,
    pub bot_id: String,
    pub name: String,
    pub instruction: String,
    pub triggers: Vec<Trigger>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub next_runs: Vec<Option<i64>>,
    pub webhook_key: String,
    pub deleted: bool,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Where its runs stand, for the next one (`progress.rs`).
    #[serde(default)]
    pub progress: Option<String>,
    /// The folder its runs work in, an absolute path; none: the bot's. A run with one always works in a thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Pending,
    Starting,
    Running,
    Recovering,
    Succeeded,
    Failed,
    Interrupted,
    Cancelled,
}

impl Status {
    fn active(self) -> bool {
        matches!(self, Self::Pending | Self::Starting | Self::Running | Self::Recovering)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub routine_id: String,
    pub bot_id: String,
    pub status: Status,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub detail: Option<String>,
    pub event: Value,
    pub delivery_id: Option<String>,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    report_pending: bool,
    #[serde(default)]
    result: Option<String>,
    /// How many earlier tries of the same work failed before acting (`retry.rs`).
    #[serde(default)]
    pub attempt: u32,
    /// A queued retry waits until then.
    #[serde(default)]
    pub not_before: Option<i64>,
    /// This failed run is tried again automatically then.
    #[serde(default)]
    pub retry_at: Option<i64>,
    /// This failed run can be retried by the user, so it keeps its event until then.
    #[serde(default)]
    pub retryable: bool,
    /// It is a task in its bot's box (`chat::work`), which says where it stands, not a turn in the chat.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub boxed: bool,
}

impl Run {
    /// A retry still waiting for its time: it holds nothing up and nothing takes it yet.
    fn waiting(&self, now: i64) -> bool {
        self.status == Status::Pending && self.not_before.is_some_and(|at| at > now)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct State {
    routines: Vec<Routine>,
    runs: Vec<Run>,
    #[serde(default)]
    follow_ups: Vec<FollowUp>,
}

pub struct Routines {
    state: Mutex<State>,
    active: Mutex<HashSet<String>>,
    scheduler: Mutex<Option<tokio::task::JoinHandle<()>>>,
    stopping: std::sync::atomic::AtomicBool,
    /// Bumped whenever the webhook set the relay registers (`hooks()`) changes.
    hooks_rev: tokio::sync::watch::Sender<u64>,
    following: follow_up::Following,
}

impl Default for Routines {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            active: Mutex::default(),
            scheduler: Mutex::default(),
            stopping: std::sync::atomic::AtomicBool::default(),
            hooks_rev: tokio::sync::watch::channel(0).0,
            following: follow_up::Following::default(),
        }
    }
}

/// One routine's public webhook as the cloud stores it.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Hook {
    pub id: String,
    pub key: String,
    pub enabled: bool,
}

fn hook_set(state: &State) -> Vec<Hook> {
    state
        .routines
        .iter()
        .filter(|r| !r.deleted && r.triggers.iter().any(|t| matches!(t, Trigger::Webhook | Trigger::Event { .. })))
        .map(|r| Hook { id: r.id.clone(), key: r.webhook_key.clone(), enabled: r.enabled })
        .collect()
}

fn webhook_key() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// The stored routines.
fn stored(store: &Store) -> Result<State> {
    let Some(raw) = store.kv_read(STATE_KEY)? else { return Ok(State::default()) };
    serde_json::from_str(&raw).context("reading routines")
}

/// The stored routines of `bots`, deleted ones too (`hub::projects::migrate`'s backup).
pub fn of_bots(store: &Store, bots: &[&str]) -> Result<Vec<Routine>> {
    Ok(stored(store)?.routines.into_iter().filter(|r| bots.contains(&r.bot_id.as_str())).collect())
}

/// Before `Routines::restore` reads them (`hub::projects::migrate`): `bot`'s routines that name no folder are stamped
/// with `folder` in the stored state.
pub fn stamp_folder(store: &Store, bot: &str, folder: &str) -> Result<()> {
    let mut state = stored(store)?;
    let unstamped = |r: &&mut Routine| r.bot_id == bot && !r.deleted && r.folder.is_none();
    let mut stamped = false;
    for routine in state.routines.iter_mut().filter(unstamped) {
        routine.folder = Some(folder.to_owned());
        stamped = true;
    }
    if stamped {
        store.kv_set(STATE_KEY, &serde_json::to_string(&state)?)?;
    }
    Ok(())
}

impl Routines {
    pub fn restore(&self, store: &Store) -> Result<()> {
        let mut state = stored(store)?;
        let mut active = self.active.locked();
        active.clear();
        for run in &mut state.runs {
            match run.status {
                Status::Starting => run.status = Status::Pending,
                // A run in the box is its thread's to resume (`chat::work::restore`).
                Status::Running | Status::Recovering if !run.boxed => {
                    run.status = Status::Recovering;
                    run.finished_at = None;
                    active.insert(run.bot_id.clone());
                }
                _ => {}
            }
        }
        store.kv_set(STATE_KEY, &serde_json::to_string(&state)?)?;
        *self.state.locked() = state;
        self.hooks_rev.send_modify(|v| *v += 1);
        Ok(())
    }

    /// The webhook routines the relay registers with the cloud.
    pub fn hooks(&self) -> Vec<Hook> {
        hook_set(&self.state.locked())
    }

    pub fn subscribe_hooks(&self) -> tokio::sync::watch::Receiver<u64> {
        self.hooks_rev.subscribe()
    }

    fn change<T>(&self, store: &Store, update: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut guard = self.state.locked();
        let mut next = guard.clone();
        let result = update(&mut next)?;
        let mut kept = 0;
        next.runs.reverse();
        next.runs.retain(|run| {
            // A failure the user can still retry keeps its event (at most one per routine).
            if run.status.active() || run.report_pending || run.retryable {
                return true;
            }
            kept += 1;
            kept <= MAX_FINISHED_RUNS
        });
        next.runs.reverse();
        next.routines
            .retain(|r| !r.deleted || next.runs.iter().any(|run| run.routine_id == r.id && run.status.active()));
        store.kv_set(STATE_KEY, &serde_json::to_string(&next)?)?;
        let hooks_changed = hook_set(&guard) != hook_set(&next);
        *guard = next;
        drop(guard);
        if hooks_changed {
            self.hooks_rev.send_modify(|v| *v += 1);
        }
        Ok(result)
    }

    pub fn list(&self, bot: &str) -> Value {
        let state = self.state.locked();
        // The next one to happen first, as every list of them shows.
        let mut follow_ups: Vec<_> = state.follow_ups.iter().filter(|f| f.bot_id == bot).collect();
        follow_ups.sort_by_key(|f| f.at);
        json!({"routines": state.routines.iter().filter(|r| r.bot_id == bot && !r.deleted).map(public).collect::<Vec<_>>(),
            "runs": state.runs.iter().rev().filter(|r| r.bot_id == bot).take(200).collect::<Vec<_>>(),
            "followUps": follow_ups})
    }

    /// All active runs (`Pending` / `Starting` / `Running` / `Recovering`) for `bot`, without `list`'s 200-run cap:
    /// a run still owed its turn is never the newest of a busy bot's last 200 finished ones.
    pub fn active_runs(&self, bot: &str) -> Value {
        let state = self.state.locked();
        let runs: Vec<_> = state.runs.iter().rev().filter(|r| r.bot_id == bot && r.status.active()).collect();
        let routines: Vec<_> = state
            .routines
            .iter()
            .filter(|r| r.bot_id == bot && (!r.deleted || runs.iter().any(|run| run.routine_id == r.id)))
            .map(public)
            .collect();
        json!({"routines": routines, "runs": runs})
    }

    pub fn owns(&self, bot: &str, id: &str) -> bool {
        self.state.locked().routines.iter().any(|r| r.is(bot, id))
    }

    pub fn save(&self, hub: &Hub, bot: &str, body: &Value) -> Result<Value> {
        let row = agent_bot(hub, bot)?;
        let Checked { name, instruction, triggers, timeout, folder } = Checked::parse(body)?;
        let now = now_ms();
        let id = body["id"].as_str().map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned);
        let updated = body["id"].is_string();
        let routine = self.change(&hub.store, |s| {
            let old = check::replaced(s, bot, body, &triggers, now)?;
            if !updated && s.routines.iter().filter(|r| !r.deleted).count() >= 1000 {
                bail!("routine limit reached");
            }
            let r = Routine {
                id: id.clone(),
                bot_id: row.config.id.clone(),
                name: name.clone(),
                instruction,
                enabled: body["enabled"].as_bool().unwrap_or_else(|| old.as_ref().is_none_or(|r| r.enabled)),
                created_at: old.as_ref().map_or(now, |r| r.created_at),
                updated_at: now,
                next_runs: if let Some(old) = &old
                    && old.triggers == triggers
                    && (body["enabled"] != true || old.enabled)
                {
                    old.next_runs.clone()
                } else {
                    triggers.iter().map(|t| t.next(now)).collect::<Result<_>>()?
                },
                triggers,
                webhook_key: old.as_ref().map_or_else(webhook_key, |r| r.webhook_key.clone()),
                deleted: false,
                timeout_seconds: timeout
                    .unwrap_or_else(|| old.as_ref().map_or_else(default_timeout, |r| r.timeout_seconds)),
                last_error: None,
                progress: old.as_ref().and_then(|r| r.progress.clone()),
                folder: folder.applied(old.as_ref().and_then(|r| r.folder.clone()))?,
            };
            if !(1..=86_400).contains(&r.timeout_seconds) {
                bail!("timeoutSeconds must be between 1 and 86400");
            }
            if !r.enabled {
                cancel_pending(s, &id);
            }
            s.routines.retain(|r| r.id != id);
            s.routines.push(r.clone());
            Ok(r)
        })?;
        hub.add_entry(&Lane::main(bot), EntryKind::Notice, 0, &json!({"text":format!("{} routine: {name}", if updated {"Updated"} else {"Created"}), "routineId": id, "style":"info"}));
        Ok(json!({"routine":public(&routine)}))
    }

    pub fn set_enabled(&self, store: &Store, bot: &str, id: &str, enabled: bool) -> Result<Value> {
        self.change(store, |s| {
            let r = own_mut(&mut s.routines, bot, id)?;
            if enabled && !r.enabled {
                r.next_runs = r.triggers.iter().map(|t| t.next(now_ms())).collect::<Result<_>>()?;
            }
            r.enabled = enabled;
            r.updated_at = now_ms();
            if !enabled {
                for run in &mut s.runs {
                    if run.routine_id == id && run.status == Status::Pending {
                        cancel_run(run);
                    }
                }
            }
            Ok(json!({"routine":public(r)}))
        })
    }

    pub fn remove(&self, hub: &Hub, bot: &str, id: &str) -> Result<Value> {
        let withdrawn = self.change(&hub.store, |s| {
            let r = own_mut(&mut s.routines, bot, id)?;
            r.deleted = true;
            r.enabled = false;
            r.webhook_key.clear();
            cancel_pending(s, id);
            Ok(retry::forget(s, |run| run.routine_id == id))
        })?;
        retry::withdraw(hub, &withdrawn);
        Ok(json!({"ok":true}))
    }

    pub fn enqueue(
        &self,
        store: &Store,
        bot: &str,
        id: &str,
        event: Value,
        delivery: Option<String>,
        test: bool,
    ) -> Result<Value> {
        self.change(store, |s| {
            let r = own(&s.routines, bot, id)?;
            if !r.enabled && !test {
                bail!(Refused::Paused);
            }
            if let Some(key) = &delivery
                && let Some(run) =
                    s.runs.iter().find(|run| run.routine_id == id && run.delivery_id.as_ref() == Some(key))
            {
                return Ok(json!({"run":run}));
            }
            // Asked to run now while a retry waits: that retry runs now.
            if test
                && let Some(waiting) = s
                    .runs
                    .iter_mut()
                    .find(|run| run.routine_id == id && run.status == Status::Pending && run.not_before.is_some())
            {
                waiting.not_before = None;
                return Ok(json!({"run":waiting}));
            }
            if busy(s, id) {
                bail!(Refused::Busy);
            }
            let run = new_run(r, event, delivery);
            let result = json!({"run":run});
            s.runs.push(run);
            Ok(result)
        })
    }

    /// The routine's webhook: the public URL through the Codync cloud (when it's on; a private
    /// relay doesn't deliver them), the local one, and the key. `rotate` replaces the key first,
    /// cutting off the old one.
    pub fn credentials(&self, hub: &Hub, bot: &str, id: &str, rotate: bool) -> Result<Value> {
        if rotate {
            self.change(&hub.store, |s| {
                let r = own_mut(&mut s.routines, bot, id)?;
                r.webhook_key = webhook_key();
                r.updated_at = now_ms();
                Ok(())
            })?;
        }
        let key = own(&self.state.locked().routines, bot, id)?.webhook_key.clone();
        let cloud = hub.cloud.status();
        // Public deliveries go through the relay, for a computer in an account (docs/plans/optional-sign-in.md).
        let url = crate::remote::cloud::codync_url(hub)
            .ok()
            .filter(|_| crate::remote::owner::owner(&hub.store).is_some())
            .map(|base| format!("{}/v1/hooks/{}/{id}", base.trim_end_matches('/'), hub.identity.computer_id()));
        Ok(json!({
            "url": url,
            "needsAccount": crate::remote::owner::can_join(hub),
            "localUrl": format!("http://127.0.0.1:{}/hooks/routines/{id}", hub.port),
            "key": key,
            "connected": url.is_some() && cloud.connected,
        }))
    }

    /// One delivery from the local endpoint or the cloud: checks the key or signature, then
    /// queues a run when the event matches. Retries of a delivery id return its first run.
    pub fn receive(&self, hub: &Hub, id: &str, headers: &hooks::Headers, body: &[u8]) -> Result<Value, Refused> {
        let r = self.state.locked().routines.iter().find(|r| r.id == id && !r.deleted).cloned();
        let Some(r) = r.filter(|r| hooks::authorized(&r.webhook_key, headers, body)) else {
            return Err(Refused::Unauthorized);
        };
        let delivery = hooks::delivery_id(headers)?;
        let Some(event) = hooks::event(headers, body)? else {
            return Ok(json!({"ping": true}));
        };
        if !r.enabled {
            return Err(Refused::Paused);
        }
        if !r.triggers.iter().any(|t| matches!(t, Trigger::Webhook) || t.matches(&event)) {
            return Ok(json!({"ignored": "event does not match this routine"}));
        }
        self.enqueue(&hub.store, &r.bot_id, id, event, delivery, false).map_err(|e| match e.downcast::<Refused>() {
            Ok(refused) => refused,
            Err(e) => Refused::Invalid(format!("{e:#}")),
        })
    }
}

impl Routine {
    /// Whether this is `bot`'s routine `id`.
    fn is(&self, bot: &str, id: &str) -> bool {
        self.id == id && self.bot_id == bot && !self.deleted
    }
}

/// `bot`'s own routine `id`. They take the routines alone, so a caller can change the runs while it holds one.
fn own<'a>(routines: &'a [Routine], bot: &str, id: &str) -> Result<&'a Routine> {
    routines.iter().find(|r| r.is(bot, id)).ok_or_else(|| kept::Refused::NotFound.into())
}

fn own_mut<'a>(routines: &'a mut [Routine], bot: &str, id: &str) -> Result<&'a mut Routine> {
    routines.iter_mut().find(|r| r.is(bot, id)).ok_or_else(|| kept::Refused::NotFound.into())
}

/// Routines and follow-ups belong to an agent bot: a group has no agent to take their turn.
fn agent_bot(hub: &Hub, bot: &str) -> Result<crate::store::BotRow> {
    hub.store
        .bot(bot)?
        .filter(|b| !b.deleted && !b.config.is_group())
        .context("routines and follow-ups need an existing agent bot")
}

fn required<'a>(body: &'a Value, key: &str) -> Result<&'a str> {
    body[key].as_str().with_context(|| format!("{key} is required"))
}
fn public(r: &Routine) -> Value {
    json!({"id":r.id,"botId":r.bot_id,"name":r.name,"instruction":r.instruction,"triggers":r.triggers,"enabled":r.enabled,"createdAt":r.created_at,"updatedAt":r.updated_at,
        "timeoutSeconds":r.timeout_seconds,"lastError":r.last_error,"folder":r.folder,
        "triggerDescriptions":r.triggers.iter().map(Trigger::description).collect::<Vec<_>>(),"nextRunAt": if r.enabled {r.next_runs.iter().flatten().min().copied()} else {None}})
}
fn new_run(r: &Routine, event: Value, delivery_id: Option<String>) -> Run {
    Run {
        id: uuid::Uuid::new_v4().to_string(),
        routine_id: r.id.clone(),
        bot_id: r.bot_id.clone(),
        status: Status::Pending,
        created_at: now_ms(),
        finished_at: None,
        detail: None,
        event,
        delivery_id,
        started_at: None,
        report_pending: false,
        result: None,
        attempt: 0,
        not_before: None,
        retry_at: None,
        retryable: false,
        boxed: false,
    }
}

/// A paused routine still takes test runs.
fn may_run(routine: &Routine, event: &Value) -> bool {
    routine.enabled || event["source"] == "test"
}

/// Most runs one routine holds queued and not yet started (webhook deliveries waiting for a busy bot, retries).
// ponytail: fixed ceiling per routine, whatever the payload sizes; make it a setting if a sender bursts more.
pub const MAX_QUEUED: usize = 16;

/// Whether `routine` takes no new run now: one of its runs has started, or `MAX_QUEUED` wait to. Queued runs
/// alone don't block: each runs once, in the order it came, when its bot is free (issue #502).
fn busy(s: &State, routine: &str) -> bool {
    started(s, routine)
        || s.runs.iter().filter(|r| r.routine_id == routine && r.status == Status::Pending).count() >= MAX_QUEUED
}

/// Whether a run of `routine` has started and not ended: its other runs wait for it (one at a time, in order).
fn started(s: &State, routine: &str) -> bool {
    s.runs.iter().any(|r| r.routine_id == routine && r.status.active() && r.status != Status::Pending)
}

fn cancel_run(run: &mut Run) {
    run.status = Status::Cancelled;
    run.finished_at = Some(now_ms());
    run.event = Value::Null;
}
fn cancel_pending(state: &mut State, id: &str) {
    for run in &mut state.runs {
        if run.routine_id == id && run.status == Status::Pending {
            cancel_run(run);
        }
    }
}

/// The name of a run's own agent session, which it resumes after a restart.
pub fn session_slot(run_id: &str) -> String {
    format!("routine-{run_id}")
}

/// The notice a run posts in the chat when it begins; the run's turn is the notice's.
pub fn notice_id(run_id: &str) -> String {
    format!("routine-{run_id}")
}

/// The reply of a run that worked in the box, which the host posts for it (`chat::work`).
pub fn reply_id(run_id: &str) -> String {
    format!("routine-reply-{run_id}")
}

/// The chat notice that reports a run's outcome.
fn result_notice_id(run_id: &str) -> String {
    format!("routine-result-{run_id}")
}

#[cfg(test)]
mod tests;
