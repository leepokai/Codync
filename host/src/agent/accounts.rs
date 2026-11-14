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
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Mutex;

pub use codync_accounts::{ACCOUNT_VAR, Account};
use codync_accounts::{Agent, SignIn, SignedIn};

const KV: &str = "agent-accounts";

/// How `backend`'s agent keeps accounts, if it can have them: by its own id or its registry entry's.
fn agent_of(backend: &str) -> Option<&'static Agent> {
    codync_accounts::agent(backends::harness_of(backend).map_or(backend, |h| h.id))
}

/// [`agent_of`], refused for an agent that keeps one login.
fn accounts_agent(backend: &str) -> Result<&'static Agent> {
    agent_of(backend)
        .ok_or_else(|| Refused::error("unsupported", format!("{backend} can't have more than one account")))
}

/// Where every account's slot is.
fn slots() -> PathBuf {
    crate::service::data_dir().join("agent-accounts")
}

/// The names the user gave the computer's own logins, by agent.
const OWN_NAMES: &str = "agent-own-account-names";

/// Who the computer's own login of each agent is signed in as, read from its files by [`refresh_own`].
static OWN: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());

/// What clients show of an account. `name`: the user's name for it, else who it signs in once it has (`email`).
/// Never sent to analytics.
fn wire(account: &Account) -> Value {
    json!({"id": account.id, "name": account.name(), "label": account.label, "email": account.email,
        "signedIn": account.signed_in})
}

fn own_names(store: &Store) -> serde_json::Map<String, Value> {
    store.kv_get(OWN_NAMES).and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

/// The computer's own login of `agent`, as clients show it beside its accounts.
fn own_wire(agent: &Agent, names: &serde_json::Map<String, Value>) -> Value {
    let label = names.get(agent.id).and_then(Value::as_str);
    let email = OWN.locked().iter().find(|(id, _)| *id == agent.id).map(|(_, email)| email.clone());
    json!({"name": label.map(str::to_owned).or_else(|| email.clone()), "label": label, "email": email})
}

/// Reads again who each agent's own login is signed in as.
pub async fn refresh_own() {
    let Some(home) = dirs::home_dir() else { return };
    let read = move || {
        let named = |agent: &'static Agent| agent.own_identity(&home).map(|email| (agent.id, email));
        codync_accounts::agents().iter().filter_map(named).collect::<Vec<_>>()
    };
    match tokio::task::spawn_blocking(read).await {
        Ok(own) => *OWN.locked() = own,
        Err(error) => tracing::warn!(%error, "couldn't read who the agents are signed in as"),
    }
}

/// Names account `id` of `backend`, or with no id the computer's own login: an empty name takes it back to who
/// it signs in.
pub fn rename(store: &Store, backend: &str, id: Option<&str>, name: &str) -> Result<()> {
    let label = codync_accounts::label(name)?;
    let Some(id) = id else {
        let agent = accounts_agent(backend)?;
        let _guard = CHANGING.locked();
        let mut names = own_names(store);
        match label {
            Some(label) => names.insert(agent.id.to_owned(), label.into()),
            None => names.remove(agent.id),
        };
        return store.kv_set(OWN_NAMES, &Value::Object(names).to_string());
    };
    change(store, |accounts| {
        let account = accounts.iter_mut().find(|a| a.id == id).ok_or_else(|| anyhow!("unknown account"))?;
        account.label = label;
        Ok(())
    })
}

/// What a check of an account answers: `{signedIn, account}`.
pub fn status(account: &Account) -> Value {
    json!({"signedIn": account.signed_in, "account": wire(account)})
}

/// The account `backend`'s process runs on, and how its agent keeps one: none for the computer's own login, or for
/// an account of another agent (a bot on a fallback keeps its own agent's account).
fn resolve(store: &Store, backend: &str, account: Option<&str>) -> Result<Option<(&'static Agent, Account)>> {
    let (Some(id), Some(agent)) = (account, agent_of(backend)) else { return Ok(None) };
    let account = find(store, id)?;
    Ok((account.agent == agent.id).then_some((agent, account)))
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

/// Held by an agent process on an account, or by its sign-in: its slot isn't removed under it.
pub struct Running(String);

impl Running {
    pub(crate) fn on(id: &str) -> Self {
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

/// Read, change and save happen under this, so two changes can't lose each other.
static CHANGING: Mutex<()> = Mutex::new(());

fn all(store: &Store) -> Vec<Account> {
    store.kv_get(KV).and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

pub(crate) fn find(store: &Store, id: &str) -> Result<Account> {
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
/// empty, for every agent that can have them, and `ownAccount` is the computer's own login beside them.
pub fn annotate(store: &Store, agents: &mut [Value]) {
    let (accounts, names) = (all(store), own_names(store));
    for agent in agents {
        let Some(kind) = agent["id"].as_str().and_then(agent_of) else { continue };
        let own: Vec<Value> = accounts.iter().filter(|a| a.agent == kind.id).map(wire).collect();
        agent["accounts"] = own.into();
        agent["ownAccount"] = own_wire(kind, &names);
        // How an account signs in: its CLI in a terminal, or the agent itself (a browser on this computer).
        agent["accountSignIn"] = (if kind.sign_in == SignIn::Agent { "browser" } else { "terminal" }).into();
    }
}

/// A new, signed-out account for `backend`, with its slot ready for the agent's sign-in.
pub async fn add(store: &Store, backend: &str) -> Result<Value> {
    let agent = accounts_agent(backend)?;
    let account = change(store, |accounts| {
        let account = Account::new(uuid::Uuid::new_v4().to_string(), agent, accounts);
        accounts.push(account.clone());
        Ok(account)
    })?;
    // A slot that couldn't be set up leaves no account behind.
    if let Err(error) = share(agent, &account).await {
        let dropped = change(store, |accounts| {
            accounts.retain(|a| a.id != account.id);
            Ok(())
        });
        if let Err(error) = dropped {
            tracing::warn!(error = format!("{error:#}"), "couldn't drop the account that wasn't set up");
        }
        return Err(error);
    }
    Ok(wire(&account))
}

/// Signs the account out and deletes its slot. Refused while a bot or one of its fallbacks is set to it, an agent
/// still runs on it (a bot moved off it mid-turn) or its sign-in is open.
pub async fn remove(hub: &Hub, id: &str) -> Result<()> {
    let account = find(&hub.store, id)?;
    let users = users(&hub.store, id)?;
    if !users.is_empty() {
        let why = format!("{} still use this account. Move them to another account first.", users.join(", "));
        return Err(Refused::error("in_use", why));
    }
    if RUNNING.locked().iter().any(|running| running == id) || hub.terms.signing_in(id) {
        let why = "An agent is still running on this account, or its sign-in is open. Try again once it's done.";
        return Err(Refused::error("in_use", why.into()));
    }
    let dir = account.slot(&slots())?;
    let agent = agent_of(&account.agent).zip(backends::harness_of(&account.agent));
    // Its own CLI forgets a login kept outside the slot (on macOS, Claude Code's is in the Keychain).
    if let Some((agent, h)) = agent
        && let Some(sign_out) = agent.sign_out
        && backends::run_cli(h, sign_out, &agent.env(&dir, &account.id)).await.is_none()
    {
        tracing::warn!(agent = h.id, "couldn't sign the account out");
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

/// The bots whose agent or one of whose fallbacks is set to account `id`, by name: saving one of them fails once
/// the account is gone (`settle`).
fn users(store: &Store, id: &str) -> Result<Vec<String>> {
    let uses = |cfg: &BotConfig| {
        cfg.account.as_deref() == Some(id) || cfg.fallbacks.iter().any(|f| f.account.as_deref() == Some(id))
    };
    Ok(store.bots()?.into_iter().filter(|b| !b.deleted && uses(&b.config)).map(|b| b.config.name).collect())
}

/// What `backend`'s process needs to run as `account`: nothing when it runs on the computer's own login
/// (`resolve`).
pub fn env(store: &Store, backend: &str, account: Option<&str>) -> Result<Vec<(String, String)>> {
    let Some((agent, account)) = resolve(store, backend, account)? else { return Ok(Vec::new()) };
    Ok(agent.env(&account.slot(&slots())?, &account.id))
}

/// The saved keys that choose `backend`'s login: a process on an account starts without them, or its agent
/// would run on the key and not on the account.
pub fn login_keys(backend: &str) -> &'static [&'static str] {
    agent_of(backend).map_or(&[], |agent| agent.login_keys)
}

/// The account a bot's agent runs on, when `backend` (the agent it's on now) is the account's own.
pub fn of_bot<'a>(store: &Store, backend: &str, cfg: &'a BotConfig) -> Option<&'a str> {
    cfg.account.as_deref().filter(|id| resolve(store, backend, Some(id)).is_ok_and(|found| found.is_some()))
}

/// A login of an agent: one of its agent accounts, or the computer's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub backend: String,
    pub account: Option<String>,
}

impl Login {
    /// The login `cfg` runs on, as the bot is on now (its own agent, or a fallback step's).
    pub fn of(store: &Store, cfg: &BotConfig) -> Self {
        Self { backend: cfg.backend.clone(), account: of_bot(store, &cfg.backend, cfg).map(str::to_owned) }
    }
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
    let Some((agent, account)) = resolve(store, backend, account)? else { return Ok(None) };
    let running = Running::on(&account.id);
    share(agent, &account).await?;
    Ok(Some(running))
}

async fn share(agent: &'static Agent, account: &Account) -> Result<()> {
    let dir = account.slot(&slots())?;
    let home = dirs::home_dir().context("no home folder")?;
    tokio::task::spawn_blocking(move || agent.share(&home, &dir)).await?.context("setting up the account's folder")
}

/// The command that signs `id` in, for a setup terminal.
pub async fn login_command(store: &Store, id: &str) -> Result<String> {
    let account = find(store, id)?;
    let env = env(store, &account.agent, Some(id))?;
    let prefix = crate::shell::env_prefix(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    Ok(format!("{prefix}{}", backends::login_command(&account.agent).await?))
}

/// Whether the account is signed in, and as whom: asked of its agent's CLI, or read from the slot's own files.
pub async fn check(store: &Store, id: &str) -> Result<Account> {
    let account = find(store, id)?;
    let (agent, h) = agent_of(&account.agent)
        .zip(backends::harness_of(&account.agent))
        .ok_or_else(|| anyhow!("{} has no accounts", account.agent))?;
    let dir = account.slot(&slots())?;
    let env = agent.env(&dir, id);
    let said = match agent.signed_in {
        SignedIn::AgentCheck => backends::cli_signed_in(h, &env).await,
        SignedIn::Lists { args, none } => backends::run_cli(h, args, &env)
            .await
            .map(|out| out.status.success() && !String::from_utf8_lossy(&out.stdout).contains(none)),
        SignedIn::File(_) | SignedIn::Logins(_) => None,
    };
    let (on_disk, email) =
        tokio::task::spawn_blocking(move || (agent.signed_in_on_disk(&dir), agent.identity(&dir))).await?;
    // Its check can still say signed in after a prompt was refused for auth (a revoked token).
    let signed_in = backends::apply_prompt_refusal(id, said.or(on_disk));
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

/// What to call agent account `account` of `backend`; none for the computer's own login, or for an account that
/// isn't `backend`'s.
pub fn name_of(store: &Store, backend: &str, account: Option<&str>) -> Option<String> {
    Some(resolve(store, backend, account).ok()??.1.called())
}

/// Keeps the accounts a bot names its agents' own, for the bot's agent and each fallback: an agent changed to
/// another, or run by a custom command, leaves its account behind. An account that is gone is an error.
pub fn settle(store: &Store, cfg: &mut BotConfig) -> Result<()> {
    cfg.account = settled(store, &cfg.backend, cfg.command.as_deref(), cfg.account.take())?;
    for step in &mut cfg.fallbacks {
        step.account = settled(store, &step.backend, step.command.as_deref(), step.account.take())?;
    }
    Ok(())
}

fn settled(store: &Store, backend: &str, command: Option<&str>, account: Option<String>) -> Result<Option<String>> {
    let Some(id) = account.filter(|id| !id.is_empty()) else { return Ok(None) };
    let custom = command.is_some_and(|c| !c.trim().is_empty());
    Ok(resolve(store, backend, Some(&id))?.filter(|_| !custom).map(|_| id))
}

#[cfg(test)]
pub(crate) mod tests;
