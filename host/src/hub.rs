//! Shared host state: store, event fan-out, bot actors, push, usage.

use crate::LockExt;
use crate::agent::bot::{self, BotHandle, Cmd};
use crate::api::devices::Pairing;
use crate::remote::identity::Identity;
use crate::remote::push;
use crate::screen::Screen;
use crate::service;
use crate::store::{BotConfig, Entry, EntryKind, Lane, Store};
use crate::usage::Usage;
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, watch};

mod bots;
mod groups;
mod lead;
mod order;
mod patch;
pub(crate) mod projects;
mod reactions;
mod sections;
mod sync;
mod validate;
pub(crate) use patch::patched;
pub(crate) use validate::validate;

/// What a bot is doing right now (wire values: `idle` / `working` / `needsInput` / `error`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BotStatus {
    #[default]
    Idle,
    Working,
    NeedsInput,
    Error,
}

#[derive(Clone, Default, Debug)]
pub struct Runtime {
    pub status: BotStatus,
    pub activity: String,
    pub started_at: Option<i64>,
    /// Where the running turn talks: its own chat or a group.
    pub lane: Option<Lane>,
    /// The user's message that started the running turn, typed in the bot's own chat; `None` for a turn a routine,
    /// a room, another bot or the host started. Memory cites it as where a fact was learned, and only such a turn
    /// may change or take out what memory says.
    pub message: Option<String>,
    /// The ACP session the running turn prompts. A memory write made there is one that session already knows.
    pub session: Option<String>,
    pub fast_mode: codync_agent::config::FastModeSupport,
    /// The fallback the bot is on; `None` on its own agent (see `agent::fallback`).
    pub agent_step: Option<AgentStep>,
    /// What a thread's agent runs apart from its turn, oldest first (`agent::bot::background_tasks`): its task waits
    /// on it.
    pub background: Vec<BackgroundTask>,
}

/// A piece of work a thread's agent reports running in the background: a command, a workflow, a monitor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    /// The agent's id for it.
    pub id: String,
    pub kind: BackgroundKind,
    /// What the agent calls it (a workflow's name, a command's description).
    pub name: String,
    /// What it does now, when the agent says (a workflow's phase and agent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    pub started_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundKind {
    Shell,
    Workflow,
    Monitor,
    Other,
}

/// The bot runs on one of its fallbacks: which, since when, and why it left the agent before.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStep {
    pub step: usize,
    pub agent: String,
    /// The login profile the step runs on; none for the default one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_profile: Option<String>,
    pub model: Option<String>,
    pub since: i64,
    pub reason: Option<crate::agent::fallback::Reason>,
}

impl AgentStep {
    pub fn new(cfg: &BotConfig, step: usize, reason: Option<crate::agent::fallback::Reason>) -> Self {
        Self {
            step,
            agent: cfg.backend.clone(),
            login_profile: cfg.login_profile.clone(),
            model: cfg.model.clone(),
            since: crate::store::now_ms(),
            reason,
        }
    }
}

pub struct Hub {
    pub analytics: crate::analytics::Analytics,
    pub routines: crate::routines::Routines,
    pub workflows: crate::workflows::Workflows,
    pub team: crate::chat::team::Requests,
    pub ceilings: crate::chat::team::Ceilings,
    pub work: crate::chat::work::Work,
    pub groups: crate::chat::group::Rooms,
    /// The user's other computers (codync-network), once this one is in a network.
    pub network: crate::network::Network,
    /// Requests this computer's bots sent to bots on the other computers, until they're answered.
    pub waiting: crate::chat::team::remote::Waiting,
    /// The user's cloud assistant: its requests to this computer's bots and their replies.
    pub assistant: crate::chat::team::assistant::Replies,
    pub store: Store,
    pub host_id: String,
    pub identity: Identity,
    /// Loopback-only bearer token for this computer's own apps and helpers.
    pub token: String,
    pub port: u16,
    pub events: broadcast::Sender<Value>,
    /// Held across "mutate store + broadcast" so events leave in rev order. A bot's config is also settled and
    /// saved under it: `login_profiles::remove` checks a profile's bots and marks it as being removed under it.
    pub(crate) emit_lock: Mutex<()>,
    /// Bots mid-deletion. Checked by `make_lead` under `emit_lock`, so a lead pick can't land in the
    /// unlocked cleanup window and leave a lead in the store whose runtime is already torn down.
    deleting: Mutex<BTreeSet<String>>,
    bots: Mutex<HashMap<String, BotHandle>>,
    runtime: Mutex<HashMap<String, Runtime>>,
    /// Bot → the status its Live Activities last heard (`outside_changed`).
    outside: Mutex<HashMap<String, BotStatus>>,
    pub phone_clients: AtomicUsize,
    pub usage: Mutex<Usage>,
    /// An agent became wanted: its usage is read now, not at the next poll.
    pub usage_wake: tokio::sync::Notify,
    keep_awake: Mutex<service::KeepAwake>,
    /// Generation of the authorized-device table and pairing offers: bumped on every
    /// change so open channels re-check their device (and the ACL is republished).
    pub auth: watch::Sender<u64>,
    pub pairing: Mutex<Pairing>,
    /// Phones waiting for this computer to let them in and join their account.
    pub pairing_requests: crate::remote::pairing_requests::PairingRequests,
    /// Device key -> open channels that have exchanged at least one frame.
    pub connected: Mutex<HashMap<String, usize>>,
    /// The cloud's `/v1/host/state` was applied on the current relay connection;
    /// account devices are refused until then (§4.3).
    pub cloud_synced: AtomicBool,
    pub cloud: crate::remote::cloud::Cloud,
    pub screen: Arc<Screen>,
    /// Setup terminals (installs, sign-ins).
    pub terms: Arc<crate::agent::term::Terms>,
    /// Model lists being fetched (`agentModels`).
    pub model_lists: crate::agent::auth::InFlight,
    /// The programs bots keep running, and what they left running through their own shells.
    pub dev_servers: crate::dev_servers::DevServers,
}

/// What tells clients of a saved entry. A thread bot's transcript goes out under a type of its own: an app from before
/// threads skips it, and the others keep it out of the chats they list.
fn entry_event(e: &Entry) -> Value {
    if crate::store::is_thread_id(&e.bot_id) {
        json!({"type": "threadEntry", "rev": e.rev, "thread": e.bot_id, "entry": e})
    } else {
        json!({"type": "entry", "rev": e.rev, "entry": e})
    }
}

impl Hub {
    /// Whether `id` is marked mid-deletion (see [`Hub::deleting`]). Test-only: the check lives inside `save_lead`.
    #[cfg(test)]
    pub(crate) fn is_deleting(&self, id: &str) -> bool {
        self.deleting.locked().contains(id)
    }

    pub fn busy(&self) -> bool {
        self.runtime.locked().values().any(|r| matches!(r.status, BotStatus::Working | BotStatus::NeedsInput))
            || self.terms.busy()
    }

    pub fn new(store: Store, host_id: String, identity: Identity, token: String, port: u16) -> Arc<Self> {
        #[cfg(test)]
        crate::agent::init_runtime();
        let (events, _) = broadcast::channel(1024);
        let screen = Arc::new(Screen::new(Screen::load_enabled(&store), events.clone()));
        Arc::new(Self {
            analytics: crate::analytics::Analytics::load(&store),
            routines: crate::routines::Routines::default(),
            workflows: crate::workflows::Workflows::default(),
            team: crate::chat::team::Requests::default(),
            ceilings: crate::chat::team::Ceilings::default(),
            work: crate::chat::work::Work::default(),
            groups: crate::chat::group::Rooms::default(),
            network: crate::network::Network::default(),
            waiting: crate::chat::team::remote::Waiting::default(),
            assistant: crate::chat::team::assistant::Replies::default(),
            store,
            host_id,
            identity,
            token,
            port,
            events,
            emit_lock: Mutex::new(()),
            deleting: Mutex::default(),
            bots: Mutex::default(),
            runtime: Mutex::default(),
            outside: Mutex::default(),
            phone_clients: AtomicUsize::new(0),
            usage: Mutex::default(),
            usage_wake: tokio::sync::Notify::new(),
            keep_awake: Mutex::default(),
            auth: watch::Sender::new(0),
            pairing: Mutex::default(),
            pairing_requests: crate::remote::pairing_requests::PairingRequests::default(),
            connected: Mutex::default(),
            cloud_synced: AtomicBool::new(false),
            cloud: crate::remote::cloud::Cloud::default(),
            screen,
            terms: Arc::default(),
            model_lists: crate::agent::auth::InFlight::default(),
            dev_servers: crate::dev_servers::DevServers::default(),
        })
    }

    pub fn start(self: &Arc<Self>) -> Result<()> {
        // Approval and choice cards and queued sends from a previous run can't be honored anymore.
        let expired = self.store.expire_pending()?;
        for _ in 0..expired.decisions {
            crate::chat::decision::Expiry::Restart.capture(self);
        }
        crate::chat::memory::import_old_memory(self);
        // Before the routines and the bots' actors read what it changes.
        projects::migrate(self);
        self.routines.restore(&self.store)?;
        crate::market::turn_on_installed_once(&self.store)?;
        for row in self.store.bots()? {
            if row.deleted {
                self.routines.disable_bot(&self.store, &row.config.id)?;
            } else if !row.config.is_group() {
                self.spawn_bot(row.config);
            }
        }
        crate::chat::work::restore(self);
        crate::chat::team::restart::restore(self);
        crate::chat::calls::rewatch(self);
        crate::routines::start(self);
        crate::workflows::start(self);
        crate::analytics::start(self);
        Ok(())
    }

    pub(crate) fn spawn_bot(self: &Arc<Self>, cfg: BotConfig) {
        let id = cfg.id.clone();
        self.runtime.locked().insert(id.clone(), Runtime::default());
        let handle = bot::spawn(self.clone(), cfg);
        self.bots.locked().insert(id, handle);
    }

    /// Stops every bot actor (and so every agent process), waiting briefly for them to exit.
    pub async fn shutdown(&self) {
        self.routines.stop().await;
        let handles: Vec<BotHandle> = self.bots.locked().drain().map(|(_, h)| h).collect();
        let tasks: Vec<_> = handles
            .into_iter()
            .map(|h| {
                let _ = h.tx.send(Cmd::Shutdown);
                h.task
            })
            .collect();
        if tokio::time::timeout(Duration::from_secs(5), futures::future::join_all(tasks)).await.is_err() {
            tracing::warn!("some bots didn't stop within 5 s");
        }
        self.dev_servers.shutdown().await;
        self.keep_awake.locked().set(false);
    }

    /// Whether `bot_id`'s actor runs.
    pub fn has_actor(&self, bot_id: &str) -> bool {
        self.bots.locked().contains_key(bot_id)
    }

    /// Stops `bot_id`'s actor, and so its agent process; its row and transcript stay.
    pub(crate) fn stop_actor(&self, bot_id: &str) {
        if let Some(handle) = self.bots.locked().remove(bot_id) {
            let _ = handle.tx.send(Cmd::Shutdown);
        }
    }

    /// A thread's actor is gone: what it was doing goes with it.
    pub(crate) fn forget_runtime(&self, bot_id: &str) {
        self.runtime.locked().remove(bot_id);
        self.update_keep_awake();
    }

    pub fn send_cmd(&self, bot_id: &str, cmd: Cmd) -> Result<()> {
        let bots = self.bots.locked();
        let h = bots.get(bot_id).ok_or_else(|| anyhow!("unknown bot"))?;
        h.tx.send(cmd).map_err(|_| anyhow!("bot stopped"))
    }

    /// [`Self::send_cmd`], giving `cmd` back when `bot_id`'s actor doesn't run.
    pub(crate) fn offer_cmd(&self, bot_id: &str, cmd: Cmd) -> std::result::Result<(), Box<Cmd>> {
        match self.bots.locked().get(bot_id) {
            Some(h) => h.tx.send(cmd).map_err(|refused| Box::new(refused.0)),
            None => Err(Box::new(cmd)),
        }
    }

    // MARK: bots

    pub fn bot_name(&self, id: &str) -> String {
        self.store.bot(id).ok().flatten().map_or_else(|| "A deleted bot".into(), |b| b.config.name)
    }

    /// Marks what the user has seen as read. Nothing new there is a no-op (a waiting approval card stays counted
    /// until answered), so clients may call it whenever a chat is on screen. So is a thread bot: no client lists it, so it has no unread to show.
    pub fn mark_read(&self, id: &str) -> Result<()> {
        if crate::store::is_thread_id(id) {
            return Ok(());
        }
        let Some(row) = self.store.bot(id)? else { bail!("unknown bot") };
        if self.store.badge(id, row.read_rev).unseen() == 0 {
            return Ok(());
        }
        let _g = self.emit_lock.locked();
        self.store.mark_read(id)?;
        if let Some(v) = self.bot_value(id) {
            let _ = self.events.send(json!({"type": "bot", "rev": v["rev"], "bot": v}));
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn bot_value_for_test(&self, id: &str) -> Value {
        self.bot_value(id).unwrap_or(Value::Null)
    }

    /// Stands in for bot `id`'s actor: what is sent to it arrives on the returned receiver.
    #[cfg(test)]
    pub fn attach_bot_for_test(&self, id: &str) -> tokio::sync::mpsc::UnboundedReceiver<Cmd> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.bots.locked().insert(id.into(), BotHandle { tx, task: tokio::spawn(async {}) });
        rx
    }

    fn bot_value(&self, id: &str) -> Option<Value> {
        self.store.bot(id).ok().flatten().map(|r| self.bot_json(&r))
    }

    /// Saves a change that stamps its own rev and broadcasts the event `save` returns, in rev order.
    pub fn emit_change(&self, save: impl FnOnce() -> Result<Value>) -> Result<()> {
        let _g = self.emit_lock.locked();
        let _ = self.events.send(save()?);
        Ok(())
    }

    /// Bumps the bot's rev and broadcasts its current state. A thread bot isn't a bot to any client: its bot's state,
    /// which lists it, goes out instead.
    pub fn emit_bot(&self, id: &str) {
        let thread = crate::store::is_thread_id(id);
        let owner = if thread { self.store.bot(id).ok().flatten().and_then(|b| b.config.thread_of) } else { None };
        let Some(id) = owner.as_deref().or((!thread).then_some(id)) else {
            return; // a thread bot that is gone
        };
        let _g = self.emit_lock.locked();
        if self.store.touch_bot(id).is_ok()
            && let Some(v) = self.bot_value(id)
        {
            let _ = self.events.send(json!({"type": "bot", "rev": v["rev"], "bot": v}));
        }
    }

    pub fn set_runtime(&self, id: &str, f: impl FnOnce(&mut Runtime)) {
        let (before, after) = {
            let mut map = self.runtime.locked();
            let rt = map.entry(id.to_owned()).or_default();
            let before = rt.clone();
            f(rt);
            (before, rt.clone())
        };
        if before.status != after.status
            || before.activity != after.activity
            || before.lane != after.lane
            || before.fast_mode != after.fast_mode
            || before.agent_step != after.agent_step
            || before.background != after.background
        {
            self.emit_bot(id);
            // A group shows what its busy member is doing.
            for lane in [&before.lane, &after.lane].into_iter().flatten() {
                if lane.chat != id {
                    self.emit_bot(&lane.chat);
                }
            }
            self.update_keep_awake();
        }
        if before.status != after.status && crate::store::is_thread_id(id) {
            // A thread's status is its bot's to what follows the bot from outside its chat.
            if let Some(owner) = self.store.bot(id).ok().flatten().and_then(|row| row.config.thread_of) {
                self.outside_changed(&owner);
                // Waiting on the user, it gives its seat up (`work::dispatch`).
                if after.status == BotStatus::NeedsInput {
                    self.work.seat_freed(&owner);
                }
            }
        } else if before.status != after.status {
            self.outside_changed(id);
            // A group's Live Activity follows its busy member.
            for lane in [&before.lane, &after.lane].into_iter().flatten().filter(|l| l.chat != id) {
                if let Ok(Some(group)) = self.store.bot(&lane.chat) {
                    push::live_activity_update(self, &lane.chat, &self.group_runtime(&group.config));
                }
            }
        }
    }

    pub fn runtime(&self, id: &str) -> Runtime {
        self.runtime.locked().get(id).cloned().unwrap_or_default()
    }

    /// A bot's runtime as what shows it outside its chat reads it (a Live Activity, an ongoing notification): its
    /// chat's, with the status and start its work in the background gives it (`chat::work::outside`).
    pub fn outside_runtime(&self, cfg: &BotConfig) -> Runtime {
        let chat = self.runtime(&cfg.id);
        let (status, started_at) = crate::chat::work::outside(self, cfg, &chat);
        Runtime { status, started_at, ..chat }
    }

    /// `bot`'s chat or its box changed: its Live Activities hear how it stands now, when that is new to them. So
    /// one doesn't end while tasks work, and ends once the box is empty and the chat idle.
    pub fn outside_changed(&self, bot: &str) {
        let row = self.store.bot(bot).ok().flatten().filter(|row| !row.config.is_group() && !row.config.is_thread());
        let Some(row) = row else { return };
        let rt = self.outside_runtime(&row.config);
        let before = self.outside.locked().insert(bot.to_owned(), rt.status);
        if before.unwrap_or_default() != rt.status {
            push::live_activity_update(self, bot, &rt);
        }
    }

    pub(crate) fn update_keep_awake(&self) {
        let busy = self.runtime.locked().values().any(|r| r.status == BotStatus::Working || !r.background.is_empty());
        self.keep_awake
            .locked()
            .set(busy || self.routines.keeps_awake() || crate::workflows::Workflows::keeps_awake(&self.store));
    }

    // MARK: entries

    /// Persists and broadcasts a new transcript entry. A failed write is logged here
    /// (once) and yields `None`: the conversation goes on without that line.
    pub fn add_entry(&self, lane: &Lane, kind: EntryKind, turn: i64, data: &Value) -> Option<Entry> {
        let e = self.insert_entry(lane, kind, turn, data)?;
        self.order_on_message(&e);
        Some(e)
    }

    fn insert_entry(&self, lane: &Lane, kind: EntryKind, turn: i64, data: &Value) -> Option<Entry> {
        let _g = self.emit_lock.locked();
        match self.store.insert_entry(lane, kind, turn, data) {
            Ok(e) => {
                let _ = self.events.send(entry_event(&e));
                Some(e)
            }
            Err(error) => {
                tracing::error!(
                    error = format!("{error:#}"),
                    chat = lane.chat,
                    kind = kind.as_str(),
                    "couldn't save transcript entry"
                );
                None
            }
        }
    }

    pub(crate) fn emit_workflow(&self, body: Value) {
        let _guard = self.emit_lock.locked();
        let _ = self.events.send(body);
    }

    pub fn add_entry_once(
        &self,
        id: &str,
        lane: &Lane,
        kind: EntryKind,
        turn: i64,
        data: &Value,
    ) -> Result<(Entry, bool)> {
        let (entry, added) = {
            let _guard = self.emit_lock.locked();
            let (entry, added) = self.store.insert_entry_once(id, lane, kind, turn, data)?;
            if added {
                let _ = self.events.send(entry_event(&entry));
            }
            (entry, added)
        };
        if added {
            self.order_on_message(&entry);
        }
        Ok((entry, added))
    }

    /// Updates and broadcasts an entry; failures are logged here, like [`Self::add_entry`].
    pub fn set_entry(&self, id: &str, data: &Value) -> Option<Entry> {
        self.set_entry_turn(id, None, data)
    }

    /// [`Self::set_entry`] that also moves the entry into `turn` when given.
    pub fn set_entry_turn(&self, id: &str, turn: Option<i64>, data: &Value) -> Option<Entry> {
        let old = self.store.entry(id);
        // A reply counts as a message once it's finished, which is an update; nothing else here is a new message.
        let finishes = data["final"] == true && old.as_ref().is_some_and(|old| old.data["final"] != true);
        // A card answered or expired leaves its chat's badge (`Store::badge`).
        let settles = data["status"] != "pending"
            && old.is_some_and(|old| old.kind == EntryKind::Permission.as_str() && old.data["status"] == "pending");
        let e = self.update_entry(id, turn, data)?;
        if finishes {
            self.order_on_message(&e);
        }
        if settles {
            self.emit_bot(&e.bot_id);
        }
        Some(e)
    }

    /// Updates entries and kv rows in one write ([`Store::update_entries_kv`]) and broadcasts the entries; a failure
    /// (nothing written) is logged here, like [`Self::set_entry`].
    pub fn set_entries_with_kv(&self, entries: &[(String, Value)], delete: &[String], set: &[(String, String)]) {
        let _g = self.emit_lock.locked();
        match self.store.update_entries_kv(entries, delete, set) {
            Ok(updated) => {
                for e in &updated {
                    let _ = self.events.send(entry_event(e));
                }
            }
            Err(error) => tracing::error!(error = format!("{error:#}"), "couldn't update transcript entries"),
        }
    }

    fn update_entry(&self, id: &str, turn: Option<i64>, data: &Value) -> Option<Entry> {
        let _g = self.emit_lock.locked();
        match self.store.update_entry_turn(id, turn, data) {
            Ok(e) => {
                if let Some(e) = &e {
                    let _ = self.events.send(entry_event(e));
                }
                e
            }
            Err(error) => {
                tracing::error!(error = format!("{error:#}"), entry = id, "couldn't update transcript entry");
                None
            }
        }
    }

    /// Changes part of an entry and broadcasts it, reading and writing under the emission lock: an edit made
    /// meanwhile elsewhere (a reaction from another device) isn't written over. `edit` returns `false` to leave the
    /// entry as it is.
    pub fn edit_entry(&self, id: &str, edit: impl FnOnce(&mut Entry) -> Result<bool>) -> Result<Entry> {
        let _g = self.emit_lock.locked();
        let mut e = self.store.entry(id).ok_or_else(|| anyhow!("unknown message"))?;
        if !edit(&mut e)? {
            return Ok(e);
        }
        let e = self.store.update_entry(id, &e.data)?.ok_or_else(|| anyhow!("unknown message"))?;
        let _ = self.events.send(entry_event(&e));
        Ok(e)
    }

    /// Changes the usage under its lock, so a reading that lands meanwhile isn't written over, and
    /// tells the apps what they may see of it (`hub.usage` also holds the agents the user hides).
    pub fn update_usage(&self, change: impl FnOnce(&mut Usage)) {
        let mut usage = self.usage.locked();
        let before = usage.clone();
        change(&mut usage);
        if *usage != before {
            let _ = self.events.send(json!({"type": "usage", "usage": usage.visible()}));
        }
    }

    // MARK: devices

    /// Tells open channels (and the ACL publisher) that devices or pairing offers changed.
    pub fn auth_changed(&self) {
        self.auth.send_modify(|g| *g += 1);
    }

    /// Removes an authorized device: its tickets go with it and its open channels close.
    pub fn revoke_device(&self, key: &str) -> Result<bool> {
        let removed = self.store.remove_device(key)?;
        if removed {
            tracing::info!(device = key, "device revoked");
            self.auth_changed();
        }
        Ok(removed)
    }

    pub fn phone_connected(&self) -> bool {
        self.phone_clients.load(Ordering::Relaxed) > 0
    }
}
