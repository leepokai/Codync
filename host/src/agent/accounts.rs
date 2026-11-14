//! Agent accounts: more than one login to the same agent on this computer, so bots can run on different ones at
//! once ([design](../../../docs/features/agent-accounts.md)). An account is a folder (its slot) the agent keeps its
//! login in, and the agent's own variable points a process at it. The login the computer already has stays where
//! it is: a bot with no account uses it.
//!
//! The agent's own CLI signs in and out; Codync never reads or stores a token.

use crate::LockExt;
use crate::agent::backends;
use crate::hub::Hub;
use crate::store::{BotConfig, Store};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod identity;
mod slot;

const KV: &str = "agent-accounts";
/// Set for an agent on an account, so its helpers know they don't report for the computer's own login.
pub const ACCOUNT_VAR: &str = "CODYNC_AGENT_ACCOUNT";

/// How one agent keeps a login in a folder of its own.
struct Spec {
    backend: &'static str,
    /// Points the agent at the slot.
    home_var: &'static str,
    /// Where the computer's own login and setup live, under the user's home.
    home: &'static str,
    shared: slot::Shared,
    /// The saved keys that would choose another login (`login_keys`), each with the address saved for it.
    login_keys: &'static [&'static str],
    /// The CLI arguments that sign the slot out.
    logout: &'static str,
    /// Who the slot is signed in as, read from its own files.
    email: fn(&Path) -> Option<String>,
}

const SPECS: [Spec; 2] = [
    Spec {
        backend: "claude",
        home_var: "CLAUDE_CONFIG_DIR",
        home: ".claude",
        shared: slot::Shared::Claude,
        login_keys: &slot::CLAUDE_LOGIN_ENV,
        logout: "auth logout",
        email: identity::claude,
    },
    Spec {
        backend: "codex",
        home_var: "CODEX_HOME",
        home: ".codex",
        shared: slot::Shared::AllBut(&["auth.json", "auth.json.lock", "auth.lock", "models_cache.json"]),
        login_keys: &["OPENAI_API_KEY", "CODEX_API_KEY"],
        logout: "logout",
        email: identity::codex,
    },
];

fn spec(backend: &str) -> Option<&'static Spec> {
    let id = backends::harness_of(backend).map_or(backend, |h| h.id);
    SPECS.iter().find(|s| s.backend == id)
}

/// The account `backend`'s process runs on, and how its agent keeps one: none for the computer's own login, or for
/// an account of another agent (a bot on a fallback keeps its own agent's account).
fn resolve(store: &Store, backend: &str, account: Option<&str>) -> Result<Option<(&'static Spec, Account)>> {
    let (Some(id), Some(spec)) = (account, spec(backend)) else { return Ok(None) };
    let account = find(store, id)?;
    Ok((account.backend == spec.backend).then_some((spec, account)))
}

/// An account that couldn't be added or removed, with why for analytics (`category`).
#[derive(Debug)]
pub struct Refused {
    pub category: &'static str,
    message: String,
}

impl Refused {
    fn error(category: &'static str, message: String) -> anyhow::Error {
        Self { category, message }.into()
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refused {}

/// The accounts agent processes run on now, one entry per process (`Running`).
static RUNNING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Held by an agent process on an account: its slot isn't removed under it.
pub struct Running(String);

impl Running {
    fn on(id: &str) -> Self {
        RUNNING.locked().push(id.to_owned());
        Self(id.to_owned())
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let mut running = RUNNING.locked();
        if let Some(at) = running.iter().position(|id| *id == self.0) {
            running.swap_remove(at);
        }
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub backend: String,
    /// Who its agent says is signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(default)]
    pub signed_in: bool,
}

impl Account {
    /// What a check of it answers: `{signedIn, account}`.
    pub fn status(&self) -> Value {
        json!({"signedIn": self.signed_in, "account": self.wire()})
    }

    fn wire(&self) -> Value {
        // `name`: who it signs in, once it has. Never sent to analytics.
        json!({"id": self.id, "name": self.email, "signedIn": self.signed_in})
    }

    fn slot(&self) -> Result<PathBuf> {
        let spec = spec(&self.backend).ok_or_else(|| anyhow!("{} has no accounts", self.backend))?;
        // The id came from this host, but the store is a file anyone can edit: it must stay one folder name.
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            bail!("that account's id isn't valid");
        }
        Ok(crate::service::data_dir().join("agent-accounts").join(spec.backend).join(&self.id))
    }
}

/// Read, change and save happen under this, so two changes can't lose each other.
static CHANGING: Mutex<()> = Mutex::new(());

fn all(store: &Store) -> Vec<Account> {
    store.kv_get(KV).and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

fn find(store: &Store, id: &str) -> Result<Account> {
    all(store).into_iter().find(|a| a.id == id).ok_or_else(|| anyhow!("That account was removed from this computer."))
}

fn change<T>(store: &Store, apply: impl FnOnce(&mut Vec<Account>) -> Result<T>) -> Result<T> {
    let _guard = CHANGING.locked();
    let mut accounts = all(store);
    let out = apply(&mut accounts)?;
    store.kv_set(KV, &serde_json::to_string(&accounts)?)?;
    Ok(out)
}

/// Adds each agent's accounts to its entry of the agent list (`backends::list`): `accounts` is there, even when
/// empty, for every agent that can have them.
pub fn annotate(store: &Store, agents: &mut [Value]) {
    let accounts = all(store);
    for agent in agents {
        let Some(spec) = agent["id"].as_str().and_then(spec) else { continue };
        let own: Vec<Value> = accounts.iter().filter(|a| a.backend == spec.backend).map(Account::wire).collect();
        agent["accounts"] = own.into();
    }
}

/// A new, signed-out account for `backend`, with its slot ready for the agent's sign-in.
pub async fn add(store: &Store, backend: &str) -> Result<Value> {
    let spec = spec(backend)
        .ok_or_else(|| Refused::error("unsupported", format!("{backend} can't have more than one account")))?;
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        backend: spec.backend.to_owned(),
        email: None,
        signed_in: false,
    };
    share(spec, account.slot()?).await?;
    change(store, |accounts| {
        accounts.push(account.clone());
        Ok(())
    })?;
    Ok(account.wire())
}

/// Signs the account out and deletes its slot. Refused while a bot is set to it, an agent still runs on it (a bot
/// moved off it mid-turn) or its sign-in is open.
pub async fn remove(hub: &Hub, id: &str) -> Result<()> {
    let account = find(&hub.store, id)?;
    let users: Vec<String> = hub
        .store
        .bots()?
        .into_iter()
        .filter(|b| !b.deleted && b.config.account.as_deref() == Some(id))
        .map(|b| b.config.name)
        .collect();
    if !users.is_empty() {
        let why = format!("{} still use this account. Move them to another account first.", users.join(", "));
        return Err(Refused::error("in_use", why));
    }
    if RUNNING.locked().iter().any(|running| running == id) || hub.terms.signing_in(id) {
        let why = "An agent is still running on this account, or its sign-in is open. Try again once it's done.";
        return Err(Refused::error("in_use", why.into()));
    }
    let dir = account.slot()?;
    if let (Some(spec), Some(h)) = (spec(&account.backend), backends::harness_of(&account.backend)) {
        // Its own CLI forgets the login (on macOS it's in the Keychain, not in the slot).
        if backends::run_cli(h, spec.logout, &slot_env(spec, &account, &dir)).await.is_none() {
            tracing::warn!(agent = h.id, "couldn't sign the account out");
        }
    }
    tokio::task::spawn_blocking(move || match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("removing {}", dir.display()))
        }
        _ => Ok(()),
    })
    .await??;
    change(&hub.store, |accounts| {
        accounts.retain(|a| a.id != id);
        Ok(())
    })
}

/// What `backend`'s process needs to run as `account`: nothing when it runs on the computer's own login
/// (`resolve`).
pub fn env(store: &Store, backend: &str, account: Option<&str>) -> Result<Vec<(String, String)>> {
    let Some((spec, account)) = resolve(store, backend, account)? else { return Ok(Vec::new()) };
    Ok(slot_env(spec, &account, &account.slot()?))
}

/// The saved keys that choose `backend`'s login: a process on an account starts without them, or its agent
/// would run on the key and not on the account.
pub fn login_keys(backend: &str) -> &'static [&'static str] {
    spec(backend).map_or(&[], |spec| spec.login_keys)
}

fn slot_env(spec: &Spec, account: &Account, dir: &Path) -> Vec<(String, String)> {
    vec![(spec.home_var.to_owned(), dir.to_string_lossy().into_owned()), (ACCOUNT_VAR.to_owned(), account.id.clone())]
}

/// The account a bot's agent runs on, when `backend` (the agent it's on now) is the account's own.
pub fn of_bot<'a>(store: &Store, backend: &str, cfg: &'a BotConfig) -> Option<&'a str> {
    cfg.account.as_deref().filter(|id| resolve(store, backend, Some(id)).is_ok_and(|found| found.is_some()))
}

/// What a turn showed about the account's sign-in: a prompt it refused for auth, or one it answered. Whether
/// that's news. A refusal stands over a check that still says signed in (`check`).
pub fn note_signed_in(store: &Store, id: &str, signed_in: bool) -> bool {
    if signed_in {
        backends::clear_prompt_refused(id);
    } else {
        backends::note_prompt_refused(id);
    }
    change(store, |accounts| {
        let account = accounts.iter_mut().find(|a| a.id == id).ok_or_else(|| anyhow!("unknown account"))?;
        Ok(std::mem::replace(&mut account.signed_in, signed_in) != signed_in)
    })
    .unwrap_or_else(|error| {
        tracing::warn!(error = format!("{error:#}"), "couldn't record the account's sign-in");
        false
    })
}

/// Brings the account's slot up to date with the user's own setup (skills, rules, MCP servers, sessions) before
/// its agent starts. The process keeps what this returns for as long as it runs.
pub async fn prepare(store: &Store, backend: &str, account: Option<&str>) -> Result<Option<Running>> {
    let Some((spec, account)) = resolve(store, backend, account)? else { return Ok(None) };
    let running = Running::on(&account.id);
    share(spec, account.slot()?).await?;
    Ok(Some(running))
}

async fn share(spec: &'static Spec, dir: PathBuf) -> Result<()> {
    let home = dirs::home_dir().context("no home folder")?;
    tokio::task::spawn_blocking(move || slot::share(spec.shared, &home, &home.join(spec.home), &dir))
        .await?
        .context("setting up the account's folder")
}

/// The command that signs `id` in, for a setup terminal.
pub async fn login_command(store: &Store, id: &str) -> Result<String> {
    let account = find(store, id)?;
    let env = env(store, &account.backend, Some(id))?;
    let prefix = crate::shell::env_prefix(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    Ok(format!("{prefix}{}", backends::login_command(&account.backend).await?))
}

/// Asks the account's agent whether it's signed in, and as whom.
pub async fn check(store: &Store, id: &str) -> Result<Account> {
    let account = find(store, id)?;
    let (spec, h) = spec(&account.backend)
        .zip(backends::harness_of(&account.backend))
        .ok_or_else(|| anyhow!("{} has no accounts", account.backend))?;
    let dir = account.slot()?;
    let said = backends::cli_signed_in(h, &slot_env(spec, &account, &dir)).await;
    // Its check can still say signed in after a prompt was refused for auth (a revoked token).
    let signed_in = backends::apply_prompt_refusal(id, said);
    let email = tokio::task::spawn_blocking(move || (spec.email)(&dir)).await?;
    change(store, |accounts| {
        let account = accounts.iter_mut().find(|a| a.id == id).ok_or_else(|| anyhow!("unknown account"))?;
        // A check that couldn't tell keeps what the last one found.
        if let Some(signed_in) = signed_in {
            account.signed_in = signed_in;
            account.email = email.filter(|_| signed_in);
        }
        Ok(account.clone())
    })
}

/// Keeps a bot's account one of its own agent's: a bot moved to another agent leaves the account behind.
pub fn settle(store: &Store, cfg: &mut BotConfig) -> Result<()> {
    let Some(id) = cfg.account.as_deref().filter(|id| !id.is_empty()) else {
        cfg.account = None;
        return Ok(());
    };
    if resolve(store, &cfg.backend, Some(id))?.is_none() || cfg.command.as_deref().is_some_and(|c| !c.trim().is_empty())
    {
        cfg.account = None;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
