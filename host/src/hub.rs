//! Shared host state: store, event fan-out, bot actors, push, usage.

use crate::LockExt;
use crate::agent::bot::{self, BotHandle, Cmd};
use crate::api::devices::Pairing;
use crate::remote::identity::Identity;
use crate::remote::push;
use crate::screen::Screen;
use crate::service;
use crate::store::{BotConfig, Entry, EntryKind, Lane, ReadScope, Store};
use crate::usage::Usage;
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, watch};

mod sync;

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
    /// Where the running turn talks: its own chat, a thread, or a group.
    pub lane: Option<Lane>,
}

/// Reactions per message: a handful of distinct emoji.
const MAX_REACTIONS: usize = 8;

pub struct Hub {
    pub analytics: crate::analytics::Analytics,
    pub routines: crate::routines::Routines,
    pub team: crate::chat::team::Requests,
    pub groups: crate::chat::group::Rooms,
    pub store: Store,
    pub host_id: String,
    pub identity: Identity,
    /// Loopback-only bearer token for this computer's own apps and helpers.
    pub token: String,
    pub port: u16,
    pub events: broadcast::Sender<Value>,
    /// Held across "mutate store + broadcast" so events leave in rev order.
    emit_lock: Mutex<()>,
    bots: Mutex<HashMap<String, BotHandle>>,
    runtime: Mutex<HashMap<String, Runtime>>,
    pub ios_clients: AtomicUsize,
    pub usage: Mutex<Usage>,
    keep_awake: Mutex<service::KeepAwake>,
    /// Generation of the authorized-device table and pairing offers: bumped on every
    /// change so open channels re-check their device (and the ACL is republished).
    pub auth: watch::Sender<u64>,
    pub pairing: Mutex<Pairing>,
    /// Device key -> open channels that have exchanged at least one frame.
    pub connected: Mutex<HashMap<String, usize>>,
    /// The cloud's `/v1/host/state` was applied on the current relay connection;
    /// account devices are refused until then (§4.3).
    pub cloud_synced: AtomicBool,
    pub cloud: crate::remote::cloud::Cloud,
    pub screen: Arc<Screen>,
    /// Setup terminals (installs, sign-ins).
    pub terms: Arc<crate::agent::term::Terms>,
}

impl Hub {
    pub fn busy(&self) -> bool {
        self.runtime.locked().values().any(|r| matches!(r.status, BotStatus::Working | BotStatus::NeedsInput))
            || self.terms.busy()
    }

    pub fn new(store: Store, host_id: String, identity: Identity, token: String, port: u16) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        let screen = Arc::new(Screen::new(Screen::load_enabled(&store), events.clone()));
        Arc::new(Self {
            analytics: crate::analytics::Analytics::load(&store),
            routines: crate::routines::Routines::default(),
            team: crate::chat::team::Requests::default(),
            groups: crate::chat::group::Rooms::default(),
            store,
            host_id,
            identity,
            token,
            port,
            events,
            emit_lock: Mutex::new(()),
            bots: Mutex::default(),
            runtime: Mutex::default(),
            ios_clients: AtomicUsize::new(0),
            usage: Mutex::default(),
            keep_awake: Mutex::default(),
            auth: watch::Sender::new(0),
            pairing: Mutex::default(),
            connected: Mutex::default(),
            cloud_synced: AtomicBool::new(false),
            cloud: crate::remote::cloud::Cloud::default(),
            screen,
            terms: Arc::default(),
        })
    }

    pub fn start(self: &Arc<Self>) -> Result<()> {
        // Approval cards and queued sends from a previous run can't be honored anymore.
        self.store.expire_pending()?;
        self.routines.restore(&self.store)?;
        for row in self.store.bots()? {
            if row.deleted {
                self.routines.disable_bot(&self.store, &row.config.id)?;
            } else if !row.config.is_group() {
                self.spawn_bot(row.config);
            }
        }
        crate::routines::start(self);
        crate::analytics::start(self);
        Ok(())
    }

    fn spawn_bot(self: &Arc<Self>, cfg: BotConfig) {
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
        self.keep_awake.locked().set(false);
    }

    pub fn send_cmd(&self, bot_id: &str, cmd: Cmd) -> Result<()> {
        let bots = self.bots.locked();
        let h = bots.get(bot_id).ok_or_else(|| anyhow!("unknown bot"))?;
        h.tx.send(cmd).map_err(|_| anyhow!("bot stopped"))
    }

    // MARK: bots

    pub fn bot_name(&self, id: &str) -> String {
        self.store.bot(id).ok().flatten().map_or_else(|| "A deleted bot".into(), |b| b.config.name)
    }

    /// A group is busy while one of its members works in it: that member's status and activity.
    fn group_runtime(&self, group: &BotConfig) -> Runtime {
        let busy = {
            let runtime = self.runtime.locked();
            group.members.iter().find_map(|m| {
                let r = runtime.get(m)?;
                (r.status != BotStatus::Idle && r.lane.as_ref().is_some_and(|l| l.chat == group.id))
                    .then(|| (m.clone(), r.clone()))
            })
        };
        let Some((member, mut r)) = busy else { return Runtime::default() };
        let name = self.bot_name(&member);
        r.activity = if r.activity.is_empty() { name } else { format!("{name}: {}", r.activity) };
        r
    }

    pub fn create_bot(self: &Arc<Self>, mut cfg: BotConfig) -> Result<Value> {
        cfg.id = uuid::Uuid::new_v4().to_string();
        cfg.created_at = crate::store::now_ms();
        // No name yet (Grok Bot's flow): it gets one after its first few conversations.
        cfg.auto_name = !cfg.is_group() && cfg.name.trim().is_empty();
        if cfg.auto_name {
            crate::chat::naming::PLACEHOLDER.clone_into(&mut cfg.name);
        }
        if cfg.is_group() {
            cfg.members = crate::chat::group::valid_members(&self.store, &cfg.id, &cfg.members)?;
            // The same bots again: open the group they already share (Grok Bot's rule).
            if let Some(existing) = crate::chat::group::with_members(&self.store, &cfg.members)? {
                return Ok(self.bot_value(&existing).unwrap_or(Value::Null));
            }
        }
        validate(&cfg)?;
        crate::agent::workspace::prepare(&mut cfg)?;
        {
            let _g = self.emit_lock.locked();
            self.store.save_bot(&cfg)?;
        }
        if !cfg.is_group() {
            self.spawn_bot(cfg.clone());
        }
        self.emit_bot(&cfg.id);
        Ok(self.bot_value(&cfg.id).unwrap_or(Value::Null))
    }

    pub fn update_bot(&self, patch: &Value) -> Result<Value> {
        let id = patch["id"].as_str().ok_or_else(|| anyhow!("id required"))?.to_owned();
        let row = self.store.bot(&id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
        let mut merged = serde_json::to_value(&row.config)?;
        for (k, v) in patch.as_object().into_iter().flatten() {
            if k != "id" && k != "createdAt" {
                merged[k] = v.clone();
            }
        }
        let mut cfg: BotConfig = serde_json::from_value(merged)?;
        if cfg.name.trim().is_empty() {
            cfg.name.clone_from(&row.config.name);
        }
        // A name the user picked ends automatic naming; the namer passes `autoName` itself.
        if cfg.name != row.config.name && patch.get("autoName").is_none() {
            cfg.auto_name = false;
        }
        if cfg.kind != row.config.kind {
            bail!("a bot can't become a group, or a group a bot");
        }
        if cfg.is_group() {
            cfg.members = crate::chat::group::valid_members(&self.store, &cfg.id, &cfg.members)?;
        }
        validate(&cfg)?;
        crate::agent::workspace::prepare(&mut cfg)?;
        {
            let _g = self.emit_lock.locked();
            self.store.save_bot(&cfg)?;
        }
        if !cfg.is_group() {
            self.send_cmd(&id, Cmd::Reconfigure(Box::new(cfg)))?;
        }
        self.emit_bot(&id);
        Ok(self.bot_value(&id).unwrap_or(Value::Null))
    }

    pub fn delete_bot(&self, id: &str) -> Result<()> {
        self.routines.disable_bot(&self.store, id)?;
        self.team.cancel_bot(id);
        self.groups.stop(self, id);
        let _ = self.send_cmd(id, Cmd::Shutdown);
        self.bots.locked().remove(id);
        self.runtime.locked().remove(id);
        {
            let _g = self.emit_lock.locked();
            let rev = self.store.delete_bot(id)?;
            let _ =
                self.events.send(json!({"type": "bot", "rev": rev, "bot": {"id": id, "deleted": true, "rev": rev}}));
        }
        // Its shared files are unreachable once the bot is deleted.
        let files = crate::chat::files::root(id);
        tokio::task::spawn_blocking(move || {
            if let Err(error) = std::fs::remove_dir_all(&files)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %error, "couldn't remove a deleted bot's shared files");
            }
        });
        // A deleted bot leaves its groups; what it said there stays.
        for row in self.store.bots()? {
            let mut group = row.config;
            if !row.deleted && group.is_group() && group.members.iter().any(|m| m == id) {
                group.members.retain(|m| m != id);
                {
                    let _g = self.emit_lock.locked();
                    self.store.save_bot(&group)?;
                }
                self.emit_bot(&group.id);
            }
        }
        Ok(())
    }

    /// Marks what the user has seen as read. Nothing unread there is a no-op, so clients
    /// may call it whenever a chat or thread is on screen.
    pub fn mark_read(&self, id: &str, scope: ReadScope) -> Result<()> {
        let Some(row) = self.store.bot(id)? else { bail!("unknown bot") };
        if let ReadScope::Thread(root) = scope
            && !self.store.entry(root).is_some_and(|r| r.bot_id == id && r.thread_id.is_none())
        {
            bail!("unknown thread");
        }
        // A thread gets its own read mark the first time, or the main chat's would cover it.
        let first = matches!(scope, ReadScope::Thread(root) if self.store.kv_get(&format!("read.{root}")).is_none());
        if !first && self.store.unread(id, row.read_rev, scope) == 0 {
            return Ok(());
        }
        let roots = match scope {
            ReadScope::All => self.store.unread_roots(id),
            ReadScope::Thread(root) => vec![root.to_owned()],
            ReadScope::Chat => Vec::new(),
        };
        {
            let _g = self.emit_lock.locked();
            match scope {
                ReadScope::All => self.store.mark_all_read(id)?,
                ReadScope::Chat => self.store.mark_read(id)?,
                ReadScope::Thread(root) => self.store.mark_thread_read(id, root)?,
            };
            // Before the summaries' later revs: a client resuming after them must not miss the row.
            if let Some(v) = self.bot_value(id) {
                let _ = self.events.send(json!({"type": "bot", "rev": v["rev"], "bot": v}));
            }
        }
        for root in roots {
            self.refresh_thread_summary(id, &root);
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn bot_value_for_test(&self, id: &str) -> Value {
        self.bot_value(id).unwrap_or(Value::Null)
    }

    fn bot_value(&self, id: &str) -> Option<Value> {
        self.store.bot(id).ok().flatten().map(|r| self.bot_json(&r))
    }

    /// Bumps the bot's rev and broadcasts its current state.
    pub fn emit_bot(&self, id: &str) {
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
        if before.status != after.status || before.activity != after.activity || before.lane != after.lane {
            self.emit_bot(id);
            // A group shows what its busy member is doing.
            for lane in [&before.lane, &after.lane].into_iter().flatten() {
                if lane.chat != id {
                    self.emit_bot(&lane.chat);
                }
            }
            self.update_keep_awake();
        }
        if before.status != after.status {
            push::live_activity_update(self, id, &after);
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

    pub(crate) fn update_keep_awake(&self) {
        let busy = self.runtime.locked().values().any(|r| r.status == BotStatus::Working);
        self.keep_awake.locked().set(busy || self.routines.keeps_awake());
    }

    // MARK: entries

    /// Persists and broadcasts a new transcript entry. A failed write is logged here
    /// (once) and yields `None`: the conversation goes on without that line.
    pub fn add_entry(&self, lane: &Lane, kind: EntryKind, turn: i64, data: &Value) -> Option<Entry> {
        let added = {
            let _g = self.emit_lock.locked();
            match self.store.insert_entry(lane, kind, turn, data) {
                Ok(e) => {
                    let _ = self.events.send(json!({"type": "entry", "rev": e.rev, "entry": e}));
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
        };
        if let Some(e) = &added {
            self.update_thread_summary(e);
        }
        added
    }

    pub fn add_entry_once(&self, id: &str, lane: &Lane, kind: EntryKind, data: &Value) -> Result<(Entry, bool)> {
        let (entry, added) = {
            let _guard = self.emit_lock.locked();
            let (entry, added) = self.store.insert_entry_once(id, lane, kind, data)?;
            if added {
                let _ = self.events.send(json!({"type":"entry","rev":entry.rev,"entry":entry}));
            }
            (entry, added)
        };
        if added {
            self.update_thread_summary(&entry);
        }
        Ok((entry, added))
    }

    /// Updates and broadcasts an entry; failures are logged here, like [`Self::add_entry`].
    pub fn set_entry(&self, id: &str, data: &Value) -> Option<Entry> {
        let updated = {
            let _g = self.emit_lock.locked();
            match self.store.update_entry(id, data) {
                Ok(e) => {
                    if let Some(e) = &e {
                        let _ = self.events.send(json!({"type": "entry", "rev": e.rev, "entry": e}));
                    }
                    e
                }
                Err(error) => {
                    tracing::error!(error = format!("{error:#}"), entry = id, "couldn't update transcript entry");
                    None
                }
            }
        };
        if let Some(e) = &updated {
            self.update_thread_summary(e);
        }
        updated
    }

    /// Toggles the user's `emoji` reaction on a chat message (`data.reactions`, oldest first).
    /// A message keeps `reactions: []` once reacted to, so it never turns unread again.
    pub fn react(&self, id: &str, emoji: &str) -> Result<Entry> {
        let emoji = emoji.trim();
        if emoji.is_empty() || emoji.len() > 32 || emoji.chars().any(|c| c.is_alphanumeric() || c.is_whitespace()) {
            bail!("`emoji` must be one emoji");
        }
        let mut e = self.store.entry(id).ok_or_else(|| anyhow!("unknown message"))?;
        let visible =
            e.kind == EntryKind::User.as_str() || (e.kind == EntryKind::Agent.as_str() && e.data["final"] == true);
        if !visible {
            bail!("only chat messages take reactions");
        }
        let mut reactions: Vec<String> = serde_json::from_value(e.data["reactions"].clone()).unwrap_or_default();
        if let Some(i) = reactions.iter().position(|r| r == emoji) {
            reactions.remove(i);
        } else if reactions.len() >= MAX_REACTIONS {
            bail!("a message takes at most {MAX_REACTIONS} reactions");
        } else {
            reactions.push(emoji.to_owned());
        }
        e.data["reactions"] = json!(reactions);
        self.set_entry(id, &e.data).ok_or_else(|| anyhow!("couldn't save the reaction"))
    }

    /// A message in a thread refreshes the summary on its root (`data.thread`: reply count,
    /// newest reply, who replied, unread replies), which the main chat shows under it.
    fn update_thread_summary(&self, e: &Entry) {
        let Some(root) = &e.thread_id else { return };
        let visible =
            e.kind == EntryKind::User.as_str() || (e.kind == EntryKind::Agent.as_str() && e.data["final"] == true);
        if visible {
            self.refresh_thread_summary(&e.bot_id, root);
        }
    }

    fn refresh_thread_summary(&self, chat: &str, root: &str) {
        let Some(mut root_entry) = self.store.entry(root) else { return };
        match self.store.thread_summary(chat, root) {
            Ok(summary) if root_entry.data["thread"] != summary => {
                root_entry.data["thread"] = summary;
                self.set_entry(root, &root_entry.data);
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(error = format!("{error:#}"), root, "couldn't summarize thread"),
        }
    }

    pub fn set_usage(&self, usage: Usage) {
        let _ = self.events.send(json!({"type": "usage", "usage": usage}));
        *self.usage.locked() = usage;
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

    pub fn ios_connected(&self) -> bool {
        self.ios_clients.load(Ordering::Relaxed) > 0
    }
}

fn validate(cfg: &BotConfig) -> Result<()> {
    if cfg.name.trim().is_empty() {
        bail!("name is required");
    }
    if cfg.is_group() {
        return Ok(());
    }
    if !cfg.cwd.is_empty() && !std::path::Path::new(&cfg.cwd).is_dir() {
        bail!("workspace folder does not exist: {}", cfg.cwd);
    }
    if cfg.command.as_deref().map(str::trim).unwrap_or_default().is_empty()
        && !crate::agent::backends::is_known(&cfg.backend)
    {
        bail!("unknown backend {}", cfg.backend);
    }
    Ok(())
}
