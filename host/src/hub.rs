//! Shared host state: store, event fan-out, bot actors, push, usage.

use crate::bot::{self, BotHandle, Cmd};
use crate::store::{BotConfig, BotRow, Entry, Store};
use crate::{push, service};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone, Default, Debug)]
pub struct Runtime {
    /// idle | working | needsInput | error
    pub status: String,
    pub activity: String,
    pub started_at: Option<i64>,
}

pub struct Hub {
    pub store: Store,
    pub host_id: String,
    pub token: String,
    pub port: u16,
    pub events: broadcast::Sender<Value>,
    /// Held across "mutate store + broadcast" so events leave in rev order.
    emit_lock: Mutex<()>,
    bots: Mutex<HashMap<String, BotHandle>>,
    runtime: Mutex<HashMap<String, Runtime>>,
    pub ios_clients: AtomicUsize,
    pub usage: Mutex<Value>,
    keep_awake: Mutex<service::KeepAwake>,
    /// botId -> Live Activity push tickets.
    pub activities: Mutex<HashMap<String, Vec<String>>>,
}

impl Hub {
    pub fn new(store: Store, host_id: String, token: String, port: u16) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        Arc::new(Self {
            store,
            host_id,
            token,
            port,
            events,
            emit_lock: Mutex::new(()),
            bots: Default::default(),
            runtime: Default::default(),
            ios_clients: AtomicUsize::new(0),
            usage: Mutex::new(json!({"providers": []})),
            keep_awake: Default::default(),
            activities: Default::default(),
        })
    }

    pub fn start(self: &Arc<Self>) -> Result<()> {
        // Approval cards and queued sends from a previous run can't be honored anymore.
        self.store.expire_pending()?;
        for row in self.store.bots()? {
            if !row.deleted {
                self.spawn_bot(row.config);
            }
        }
        Ok(())
    }

    fn spawn_bot(self: &Arc<Self>, cfg: BotConfig) {
        let id = cfg.id.clone();
        self.runtime.lock().unwrap().insert(
            id.clone(),
            Runtime { status: "idle".into(), ..Default::default() },
        );
        let handle = bot::spawn(self.clone(), cfg);
        self.bots.lock().unwrap().insert(id, handle);
    }

    pub fn send_cmd(&self, bot_id: &str, cmd: Cmd) -> Result<()> {
        let bots = self.bots.lock().unwrap();
        let h = bots.get(bot_id).ok_or_else(|| anyhow!("unknown bot"))?;
        h.tx.send(cmd).map_err(|_| anyhow!("bot stopped"))
    }

    // MARK: bots

    pub fn bot_json(&self, row: &BotRow) -> Value {
        let mut v = serde_json::to_value(&row.config).unwrap();
        let rt = self.runtime.lock().unwrap().get(&row.config.id).cloned().unwrap_or_default();
        let last = self.store.last_message(&row.config.id);
        v["rev"] = row.rev.into();
        v["deleted"] = row.deleted.into();
        v["status"] = if rt.status.is_empty() { "idle".into() } else { rt.status.into() };
        v["activity"] = rt.activity.into();
        v["startedAt"] = rt.started_at.into();
        v["unread"] = self.store.unread(&row.config.id, row.read_rev).into();
        v["lastMessage"] = last.as_ref().map(|l| crate::acp::truncate(&l.0, 280)).into();
        v["lastAt"] = last.map(|l| l.1).unwrap_or(row.config.created_at).into();
        v
    }

    pub fn bots_json(&self, since: i64) -> Result<Vec<Value>> {
        Ok(self
            .store
            .bots()?
            .iter()
            .filter(|b| b.rev > since && !(since == 0 && b.deleted))
            .map(|b| self.bot_json(b))
            .collect())
    }

    pub fn create_bot(self: &Arc<Self>, mut cfg: BotConfig) -> Result<Value> {
        cfg.id = uuid::Uuid::new_v4().to_string();
        cfg.created_at = crate::store::now_ms();
        validate(&cfg)?;
        {
            let _g = self.emit_lock.lock().unwrap();
            self.store.save_bot(&cfg)?;
        }
        self.spawn_bot(cfg.clone());
        self.emit_bot(&cfg.id);
        Ok(self.bot_value(&cfg.id).unwrap_or(Value::Null))
    }

    pub fn update_bot(&self, patch: Value) -> Result<Value> {
        let id = patch["id"].as_str().ok_or_else(|| anyhow!("id required"))?.to_owned();
        let row = self.store.bot(&id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
        let mut merged = serde_json::to_value(&row.config)?;
        for (k, v) in patch.as_object().into_iter().flatten() {
            if k != "id" && k != "createdAt" {
                merged[k] = v.clone();
            }
        }
        let cfg: BotConfig = serde_json::from_value(merged)?;
        validate(&cfg)?;
        {
            let _g = self.emit_lock.lock().unwrap();
            self.store.save_bot(&cfg)?;
        }
        self.send_cmd(&id, Cmd::Reconfigure(Box::new(cfg)))?;
        self.emit_bot(&id);
        Ok(self.bot_value(&id).unwrap_or(Value::Null))
    }

    pub fn delete_bot(&self, id: &str) -> Result<()> {
        let _ = self.send_cmd(id, Cmd::Shutdown);
        self.bots.lock().unwrap().remove(id);
        self.runtime.lock().unwrap().remove(id);
        let _g = self.emit_lock.lock().unwrap();
        let rev = self.store.delete_bot(id)?;
        let _ = self.events.send(json!({"type": "bot", "rev": rev, "bot": {"id": id, "deleted": true, "rev": rev}}));
        Ok(())
    }

    pub fn mark_read(&self, id: &str) -> Result<()> {
        {
            let _g = self.emit_lock.lock().unwrap();
            self.store.mark_read(id)?;
        }
        self.emit_bot_row(id);
        Ok(())
    }

    fn bot_value(&self, id: &str) -> Option<Value> {
        self.store.bot(id).ok().flatten().map(|r| self.bot_json(&r))
    }

    /// Bumps the bot's rev and broadcasts its current state.
    pub fn emit_bot(&self, id: &str) {
        let _g = self.emit_lock.lock().unwrap();
        if self.store.touch_bot(id).is_ok()
            && let Some(v) = self.bot_value(id)
        {
            let _ = self.events.send(json!({"type": "bot", "rev": v["rev"], "bot": v}));
        }
    }

    fn emit_bot_row(&self, id: &str) {
        let _g = self.emit_lock.lock().unwrap();
        if let Some(v) = self.bot_value(id) {
            let _ = self.events.send(json!({"type": "bot", "rev": v["rev"], "bot": v}));
        }
    }

    pub fn set_runtime(&self, id: &str, f: impl FnOnce(&mut Runtime)) {
        let (before, after) = {
            let mut map = self.runtime.lock().unwrap();
            let rt = map.entry(id.to_owned()).or_default();
            let before = rt.clone();
            f(rt);
            (before, rt.clone())
        };
        if before.status != after.status || before.activity != after.activity {
            self.emit_bot(id);
            self.update_keep_awake();
            push::live_activity_update(self, id, &after);
        }
    }

    pub fn runtime(&self, id: &str) -> Runtime {
        self.runtime.lock().unwrap().get(id).cloned().unwrap_or_default()
    }

    fn update_keep_awake(&self) {
        let busy = self.runtime.lock().unwrap().values().any(|r| r.status == "working");
        self.keep_awake.lock().unwrap().set(busy);
    }

    // MARK: entries

    pub fn add_entry(&self, bot_id: &str, kind: &str, turn: i64, data: Value) -> Result<Entry> {
        let _g = self.emit_lock.lock().unwrap();
        let e = self.store.insert_entry(bot_id, kind, turn, &data)?;
        let _ = self.events.send(json!({"type": "entry", "rev": e.rev, "entry": e}));
        Ok(e)
    }

    pub fn set_entry(&self, id: &str, data: Value) -> Result<Option<Entry>> {
        let _g = self.emit_lock.lock().unwrap();
        let e = self.store.update_entry(id, &data)?;
        if let Some(e) = &e {
            let _ = self.events.send(json!({"type": "entry", "rev": e.rev, "entry": e}));
        }
        Ok(e)
    }

    pub fn set_usage(&self, usage: Value) {
        *self.usage.lock().unwrap() = usage.clone();
        let _ = self.events.send(json!({"type": "usage", "usage": usage}));
    }

    pub fn ios_connected(&self) -> bool {
        self.ios_clients.load(Ordering::Relaxed) > 0
    }
}

fn validate(cfg: &BotConfig) -> Result<()> {
    if cfg.name.trim().is_empty() {
        return Err(anyhow!("name is required"));
    }
    if !std::path::Path::new(&cfg.cwd).is_dir() {
        return Err(anyhow!("workspace folder does not exist: {}", cfg.cwd));
    }
    if cfg.command.as_deref().map(str::trim).unwrap_or_default().is_empty() && !crate::backends::is_known(&cfg.backend) {
        return Err(anyhow!("unknown backend {}", cfg.backend));
    }
    Ok(())
}
