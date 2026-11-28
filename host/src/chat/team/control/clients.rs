//! The outside apps the user lets drive this computer's work over MCP: each one's grant (kv `mcp.clients`), with its
//! token kept only as a hash, the computer's own levels (the maximum, a session on it without a token, the cloud
//! assistant). What each call leaves behind (the audit, replays, the rate) is [`super::calls`]'s.
//! Every app is its own principal: its token works on the `codync` MCP tools alone, never on another host method
//! (the API takes only the computer's own token).

use super::access::{Code, Level, Refusal, Scope};
use super::calls::{audit, recent};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::{BotConfig, Store, now_ms};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const KV: &str = "mcp.clients";
const MAX_KV: &str = "mcp.maxAccess";
const LOCAL_KV: &str = "mcp.localAccess";
const CLOUD_KV: &str = "mcp.cloudAccess";
/// What every app's token starts with, so a leaked one is recognized for what it is.
pub const TOKEN_PREFIX: &str = "cdy_mcp_";
pub(super) const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How long a new app's access lasts, and a renewal.
pub const DEFAULT_DAYS: i64 = 30;
pub const MAX_DAYS: i64 = 90;
const MAX_CLIENTS: usize = 50;
const MAX_LABEL: usize = 60;

/// The id a session on this computer without a token acts under, and the cloud assistant's.
pub const LOCAL: &str = "local";
pub const CLOUD: &str = "cloud";

/// One app the user let in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub id: String,
    pub label: String,
    pub access: Level,
    pub scopes: Vec<Scope>,
    /// The bots whose threads it may start and see; `*` every bot.
    pub bots: Vec<String>,
    /// Folders beyond the bots' projects it may start a thread in, canonical.
    #[serde(default)]
    pub folders: Vec<String>,
    /// It may start threads without a bot (on any agent here).
    pub plain_threads: bool,
    pub created_at: i64,
    pub expires_at: i64,
    #[serde(default)]
    pub last_used_at: Option<i64>,
    #[serde(default)]
    pub revoked_at: Option<i64>,
    /// sha256 of its token, hex.
    token_hash: String,
}

impl Client {
    /// What every listing shows: never the token or its hash.
    pub fn json(&self) -> Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            o.remove("tokenHash");
            o.insert("runs".into(), self.runs().into());
        }
        v
    }

    /// It may run routines and workflows (with Full access): the switch the settings show beside its level.
    pub fn runs(&self) -> bool {
        RUNS.iter().all(|s| self.scopes.contains(s))
    }
}

/// Who calls, as one call sees it.
#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    /// How a thread it starts shows ("Started by <label>").
    pub label: String,
    /// What the user approved (before the computer's maximum and its own limit).
    pub approved: Level,
    pub scopes: Vec<Scope>,
    /// The bots it may reach; `None` every one.
    pub bots: Option<Vec<String>>,
    /// The cloud assistant reaches only the bots open to cloud assistants.
    pub cloud: bool,
    pub folders: Vec<PathBuf>,
    pub plain_threads: bool,
    /// The cloud assistant app a call came through, as the cloud names it (its OAuth client), for the record.
    pub via: Option<String>,
    /// The session on this computer a call without a token came from: its `clientRequestId`s are its own.
    pub session: Option<String>,
}

impl Principal {
    pub fn reaches(&self, bot: &BotConfig) -> bool {
        (!self.cloud || bot.reachable_by_assistants)
            && self.bots.as_ref().is_none_or(|ids| ids.iter().any(|id| id == "*" || *id == bot.id))
    }

    /// Whether `bot` is one of the bots it may use (start threads on, run routines of): a bot of the user's list it
    /// reaches, never the box of threads without a bot, a thread or a group.
    pub fn uses(&self, bot: &BotConfig) -> bool {
        !bot.plain && !bot.hidden && !bot.is_thread() && !bot.is_group() && self.reaches(bot)
    }

    pub fn may(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }
}

fn hash(token: &str) -> String {
    crate::chat::files::digest_hex(&Sha256::digest(token.as_bytes()))
}

fn load(store: &Store) -> Vec<Client> {
    store.kv_get(KV).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// One change at a time to the list.
fn change<T>(store: &Store, f: impl FnOnce(&mut Vec<Client>) -> Result<T>) -> Result<T> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.locked();
    let mut all = load(store);
    let out = f(&mut all)?;
    store.kv_set(KV, &serde_json::to_string(&all)?)?;
    Ok(out)
}

fn level(store: &Store, key: &str, default: Level) -> Level {
    store.kv_get(key).as_deref().and_then(Level::parse).unwrap_or(default)
}

/// The most any app may do here, whatever it was approved (default Auto).
pub fn max_access(store: &Store) -> Level {
    level(store, MAX_KV, Level::Auto)
}

/// What a session on this computer that brings no token may do (default Ask). Not a boundary: any program of the
/// user's can read the computer's own token. It only keeps a session to the least it needs unless the user says more.
pub fn local_access(store: &Store) -> Level {
    level(store, LOCAL_KV, Level::Ask)
}

/// What the user's cloud assistant may do here (default read only): its calls pass through the Codync cloud, which
/// can read them.
pub fn cloud_access(store: &Store) -> Level {
    level(store, CLOUD_KV, Level::ReadOnly)
}

/// `mcpAccess`: the computer's own levels.
pub fn levels(store: &Store) -> Value {
    json!({"max": max_access(store), "local": local_access(store), "cloud": cloud_access(store)})
}

/// `setMcpAccess`: changes the levels named in `b` (`max`, `local`, `cloud`). Local only.
pub fn set_levels(store: &Store, b: &Value) -> Result<Value> {
    for (field, key) in [("max", MAX_KV), ("local", LOCAL_KV), ("cloud", CLOUD_KV)] {
        if let Some(word) = b[field].as_str() {
            let level = Level::parse(word).with_context(|| format!("{field}: readOnly, ask, auto or full"))?;
            store.kv_set(key, level.word())?;
        }
    }
    tracing::info!(levels = %levels(store), "MCP access levels changed");
    Ok(levels(store))
}

/// `mcpClients`: every app, without its token, with what it did lately.
pub fn list(store: &Store) -> Value {
    let audit = audit(store);
    let clients: Vec<Value> = load(store)
        .iter()
        .map(|c| {
            let mut v = c.json();
            v["recent"] = recent(&audit, &c.id);
            v
        })
        .collect();
    json!({"clients": clients, "levels": levels(store), "recent": {LOCAL: recent(&audit, LOCAL), CLOUD: recent(&audit, CLOUD)}})
}

/// What `mcpClientCreate` and `mcpClientUpdate` take.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Grant {
    pub label: Option<String>,
    pub access: Option<Level>,
    pub scopes: Option<Vec<Scope>>,
    pub bots: Option<Vec<String>>,
    pub folders: Option<Vec<String>>,
    pub plain_threads: Option<bool>,
    /// Days from now it stops working.
    pub days: Option<i64>,
    /// It works for [`DEFAULT_DAYS`] from now.
    pub renew: bool,
    /// Adds (true) or takes away (false) the routines and workflows scopes, keeping the others.
    pub runs: Option<bool>,
}

/// The scopes the settings' one switch gives or takes: running the owner's routines and workflows.
const RUNS: [Scope; 2] = [Scope::Routines, Scope::Workflows];

impl Grant {
    fn apply(self, store: &Store, client: &mut Client, now: i64) -> Result<()> {
        if let Some(label) = self.label {
            let label = label.trim();
            ensure!(!label.is_empty() && label.chars().count() <= MAX_LABEL, "a name of 1 to {MAX_LABEL} characters");
            label.clone_into(&mut client.label);
        }
        if let Some(access) = self.access {
            client.access = access;
        }
        if let Some(mut scopes) = self.scopes {
            scopes.dedup();
            client.scopes = scopes;
        }
        if let Some(runs) = self.runs {
            client.scopes.retain(|s| !RUNS.contains(s));
            if runs {
                client.scopes.extend(RUNS);
            }
        }
        if let Some(bots) = self.bots {
            let known = |id: &String| id == "*" || store.bot(id).ok().flatten().is_some_and(|r| !r.deleted);
            ensure!(bots.iter().all(known), "unknown bot");
            client.bots = bots;
        }
        if let Some(folders) = self.folders {
            client.folders = folders.iter().map(|f| allowed_folder(f)).collect::<Result<_>>()?;
        }
        if let Some(plain) = self.plain_threads {
            client.plain_threads = plain;
        }
        if let Some(days) = self.days.or(self.renew.then_some(DEFAULT_DAYS)) {
            ensure!((1..=MAX_DAYS).contains(&days), "it lasts 1 to {MAX_DAYS} days");
            client.expires_at = now + days * DAY_MS;
        }
        Ok(())
    }
}

/// A folder the user lets an app work in: a full path to a folder that exists, canonical, never Codync's own data.
pub fn allowed_folder(folder: &str) -> Result<String> {
    let path = Path::new(folder);
    ensure!(path.is_absolute(), "a folder is a full path");
    let real = std::fs::canonicalize(path).with_context(|| format!("no folder {folder}"))?;
    ensure!(real.is_dir(), "{folder} isn't a folder");
    let data = std::fs::canonicalize(crate::service::data_dir()).unwrap_or_else(|_| crate::service::data_dir());
    ensure!(
        !real.starts_with(&data) && !data.starts_with(&real),
        "Codync's own data folder is never an app's to work in"
    );
    Ok(real.to_string_lossy().into_owned())
}

/// What a coding agent runs to reach this computer as the app: its token in the environment, never in its arguments.
pub fn command(token: &str) -> String {
    format!("CODYNC_MCP_TOKEN={token} codync-host mcp network")
}

/// `mcpClientCreate`: a new app, and its token, which is shown this once and never kept. What the grant leaves out
/// is what every client gives a new app: Ask, threads on every bot and without one, for [`DEFAULT_DAYS`].
pub fn create(hub: &Hub, grant: Grant) -> Result<(Value, String)> {
    let now = now_ms();
    let mut client = Client {
        id: uuid::Uuid::new_v4().to_string(),
        label: String::new(),
        access: Level::Ask,
        scopes: vec![Scope::Threads],
        bots: vec!["*".into()],
        folders: Vec::new(),
        plain_threads: true,
        created_at: now,
        expires_at: now + DEFAULT_DAYS * DAY_MS,
        last_used_at: None,
        revoked_at: None,
        token_hash: String::new(),
    };
    ensure!(grant.label.is_some(), "an app needs a name");
    grant.apply(&hub.store, &mut client, now)?;
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|_| anyhow::anyhow!("couldn't make a token"))?;
    let token = format!("{TOKEN_PREFIX}{}", crate::chat::files::digest_hex(&secret));
    client.token_hash = hash(&token);
    let shown = client.json();
    change(&hub.store, |all| {
        all.retain(|c| c.revoked_at.is_none() || now - c.revoked_at.unwrap_or(now) < MAX_DAYS * DAY_MS);
        ensure!(all.iter().filter(|c| c.revoked_at.is_none()).count() < MAX_CLIENTS, "at most {MAX_CLIENTS} apps");
        all.push(client);
        Ok(())
    })?;
    crate::analytics::capture(hub, crate::analytics::Event::McpClientCreated, props(&shown));
    Ok((shown, token))
}

fn props(client: &Value) -> serde_json::Map<String, Value> {
    let p = json!({"access": client["access"], "scopes": client["scopes"], "plainThreads": client["plainThreads"],
        "allBots": client["bots"].as_array().is_some_and(|b| b.iter().any(|b| b == "*"))});
    p.as_object().cloned().unwrap_or_default()
}

/// `mcpClientUpdate`: changes what an app may do (wider or narrower), its name or how long it lasts.
pub fn update(store: &Store, id: &str, grant: Grant) -> Result<Value> {
    change(store, |all| {
        let client = all.iter_mut().find(|c| c.id == id && c.revoked_at.is_none()).context("unknown app")?;
        grant.apply(store, client, now_ms())?;
        Ok(client.json())
    })
}

/// `mcpClientRevoke`: the app's token stops working at once. Never capped: any device that may list apps may.
pub fn revoke(hub: &Hub, id: &str) -> Result<()> {
    change(&hub.store, |all| {
        let client = all.iter_mut().find(|c| c.id == id).context("unknown app")?;
        client.revoked_at.get_or_insert(now_ms());
        Ok(())
    })?;
    crate::analytics::capture(hub, crate::analytics::Event::McpClientRevoked, serde_json::Map::new());
    Ok(())
}

/// The app `token` stands for, while it works; refused once revoked or expired.
fn by_token(store: &Store, token: &str) -> Result<Client> {
    let wanted = hash(token);
    let now = now_ms();
    change(store, |all| {
        // Hashes of 256-bit random tokens: a timing difference tells nothing worth having.
        let client = all.iter_mut().find(|c| c.token_hash == wanted).ok_or_else(|| {
            Refusal::err(Code::Revoked, "this Codync MCP token isn't valid here: make a new one in Codync's settings")
        })?;
        if client.revoked_at.is_some() {
            return Err(Refusal::err(Code::Revoked, "this app's access was removed in Codync's settings"));
        }
        if client.expires_at <= now {
            return Err(Refusal::err(Code::Expired, "this app's access expired: renew it in Codync's settings"));
        }
        client.last_used_at = Some(now);
        Ok(client.clone())
    })
}

/// Who a call over `codync-host mcp network` is: the app its token names, else a session on this computer.
pub fn local(store: &Store, token: Option<&str>) -> Result<Principal> {
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return Ok(Principal {
            id: LOCAL.into(),
            label: "a session on this computer".into(),
            approved: local_access(store),
            scopes: vec![Scope::Threads, Scope::Routines, Scope::Workflows],
            bots: None,
            cloud: false,
            folders: Vec::new(),
            plain_threads: true,
            via: None,
            session: None,
        });
    };
    let c = by_token(store, token)?;
    Ok(Principal {
        id: c.id,
        label: c.label,
        approved: c.access,
        scopes: c.scopes,
        bots: Some(c.bots),
        cloud: false,
        folders: c.folders.into_iter().map(PathBuf::from).collect(),
        plain_threads: c.plain_threads,
        via: None,
        session: None,
    })
}

/// The user's cloud assistant, one principal for every assistant they connected.
pub fn cloud(store: &Store) -> Principal {
    Principal {
        id: CLOUD.into(),
        label: "Cloud assistant".into(),
        approved: cloud_access(store),
        scopes: vec![Scope::Threads, Scope::Routines, Scope::Workflows],
        bots: None,
        cloud: true,
        folders: Vec::new(),
        plain_threads: true,
        via: None,
        session: None,
    }
}

#[cfg(test)]
mod tests;
