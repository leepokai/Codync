//! Login profiles: more than one login to the same agent on this computer, so bots can run on different ones at
//! once ([design](../../../docs/features/login-profiles.md)). A bot with none runs on the default one: the login
//! the computer already has, which stays where it is. A profile is one of two kinds:
//!
//! - an **account**: a folder (its slot) the agent keeps its own sign-in in, which the agent's variables point a
//!   process at (`account`). The agent signs in and out itself; Codync never reads or stores its token.
//! - an **API key**: named environment variables the user typed or imported from their login shell, kept in the
//!   vault and set for the process (`keys`). Any agent can have one.

use crate::LockExt;
use crate::agent::backends;
use crate::hub::Hub;
use crate::keeper::client::Grants;
use crate::store::{BotConfig, Store};
use anyhow::{Context, Result, anyhow, bail};
use codync_accounts::{Account, Agent, SignIn};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Mutex;

mod account;
pub mod keys;
pub use account::{login_command, refresh_default};

/// Set for an agent on a login profile, so its helpers know they don't report for the default login.
pub use codync_accounts::ACCOUNT_VAR as PROFILE_VAR;

const KV: &str = "login-profiles";
/// The names the user gave the default logins, by agent.
const DEFAULT_NAMES: &str = "login-profile-default-names";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// The agent's own sign-in, in a folder of the profile's.
    #[default]
    Account,
    /// Environment variables Codync keeps and sets.
    ApiKey,
}

impl Kind {
    /// The `kind` a client sent: an account when it sent none.
    pub fn parse(kind: &Value) -> Result<Self> {
        if kind.is_null() {
            return Ok(Self::default());
        }
        serde_json::from_value(kind.clone()).context("unknown login profile kind")
    }
}

/// One more login to an agent, beside the default one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Its id, agent, name, number and sign-in state; for an account, also who it signs in.
    #[serde(flatten)]
    pub account: Account,
    #[serde(default)]
    pub kind: Kind,
    /// The names of an API key profile's variables. Their values are in the vault and never leave the host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<String>,
}

impl Profile {
    /// What to call it: the user's name for it, else, once it's signed in, who it signs in or its number. Never
    /// sent to analytics.
    fn name(&self) -> Option<String> {
        let a = &self.account;
        a.label.clone().or_else(|| a.signed_in.then(|| a.email.clone().unwrap_or_else(|| self.numbered())))
    }

    /// [`Self::name`], or its number while nothing names it: what to say of it in a sentence.
    pub fn called(&self) -> String {
        self.name().unwrap_or_else(|| self.numbered())
    }

    fn numbered(&self) -> String {
        format!("Login profile {}", self.account.number.max(2))
    }

    /// What clients show of it.
    fn wire(&self) -> Value {
        let a = &self.account;
        json!({"id": a.id, "kind": self.kind, "name": self.name(), "label": a.label, "email": a.email,
            "signedIn": a.signed_in, "vars": self.vars})
    }

    /// What a check of it answers: `{signedIn, loginProfile}`.
    pub fn status(&self) -> Value {
        json!({"signedIn": self.account.signed_in, "loginProfile": self.wire()})
    }
}

/// The agent `backend` is, as profiles name it: its harness id, also for its registry entry's.
fn agent_id(backend: &str) -> &str {
    backends::harness_of(backend).map_or(backend, |h| h.id)
}

/// How `backend`'s agent keeps accounts, if it can have them.
fn accounts_of(backend: &str) -> Option<&'static Agent> {
    codync_accounts::agent(agent_id(backend))
}

fn default_names(store: &Store) -> serde_json::Map<String, Value> {
    store.kv_get(DEFAULT_NAMES).and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

/// Agent `id`'s default login, as clients show it beside its profiles.
fn default_wire(id: &str, names: &serde_json::Map<String, Value>) -> Value {
    let label = names.get(id).and_then(Value::as_str);
    let email = account::default_identity(id);
    json!({"name": label.map(str::to_owned).or_else(|| email.clone()), "label": label, "email": email})
}

/// Names profile `id`, or with no id `backend`'s default login: an empty name takes it back to who it signs in.
pub fn rename(store: &Store, backend: &str, id: Option<&str>, name: &str) -> Result<()> {
    let label = codync_accounts::label(name)?;
    let Some(id) = id else {
        let agent = agent_id(backend);
        let _guard = CHANGING.locked();
        let mut names = default_names(store);
        match label {
            Some(label) => names.insert(agent.to_owned(), label.into()),
            None => names.remove(agent),
        };
        return store.kv_set(DEFAULT_NAMES, &Value::Object(names).to_string());
    };
    change(store, |profiles| {
        found(profiles, id)?.account.label = label;
        Ok(())
    })
}

/// The profile `backend`'s process runs on: none for the default login, or for a profile of another agent (a
/// fallback step on another agent keeps to its own).
fn resolve(store: &Store, backend: &str, profile: Option<&str>) -> Result<Option<Profile>> {
    let Some(id) = profile else { return Ok(None) };
    let profile = find(store, id)?;
    Ok((profile.account.agent == agent_id(backend)).then_some(profile))
}

/// A login profile that couldn't be added, changed or removed, with why for analytics (`category`).
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

/// The login profiles agent processes run on now, one entry per process (`Running`).
static RUNNING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Held by an agent process on a login profile, or by its sign-in: it isn't removed under it.
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

fn all(store: &Store) -> Vec<Profile> {
    store.kv_get(KV).and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

pub(crate) fn find(store: &Store, id: &str) -> Result<Profile> {
    let gone = || anyhow!("That login profile was removed from this computer.");
    all(store).into_iter().find(|p| p.account.id == id).ok_or_else(gone)
}

/// Account profile `id`. An API key profile is refused: it has no sign-in of its agent's.
pub(crate) fn find_account(store: &Store, id: &str) -> Result<Account> {
    let profile = find(store, id)?;
    if profile.kind != Kind::Account {
        bail!("That login profile is an API key: it has no sign-in to open.");
    }
    Ok(profile.account)
}

fn found<'a>(profiles: &'a mut [Profile], id: &str) -> Result<&'a mut Profile> {
    profiles.iter_mut().find(|p| p.account.id == id).ok_or_else(|| anyhow!("unknown login profile"))
}

fn change<T>(store: &Store, apply: impl FnOnce(&mut Vec<Profile>) -> Result<T>) -> Result<T> {
    let _guard = CHANGING.locked();
    let mut profiles = all(store);
    let out = apply(&mut profiles)?;
    store.kv_set(KV, &serde_json::to_string(&profiles)?)?;
    Ok(out)
}

/// Adds each agent's login profiles to its entry of the agent list (`backends::list`): `loginProfiles`,
/// `defaultLoginProfile` (the login the computer already has), `loginProfileSignIn` (how an account of it signs
/// in: `terminal`, `browser`, or null for an agent that keeps one sign-in, which can still have API keys) and
/// `loginKeys` (the variables its API key usually goes in).
pub fn annotate(store: &Store, agents: &mut [Value]) {
    let (profiles, names) = (all(store), default_names(store));
    for agent in agents {
        let Some(id) = agent["id"].as_str().map(|id| agent_id(id).to_owned()) else { continue };
        let own: Vec<Value> = profiles.iter().filter(|p| p.account.agent == id).map(Profile::wire).collect();
        let accounts = codync_accounts::agent(&id);
        agent["loginProfiles"] = own.into();
        agent["defaultLoginProfile"] = default_wire(&id, &names);
        agent["loginProfileSignIn"] = match accounts.map(|a| a.sign_in) {
            Some(SignIn::Agent) => "browser".into(),
            Some(SignIn::Terminal) => "terminal".into(),
            None => Value::Null,
        };
        agent["loginKeys"] = json!(accounts.map_or(&[][..], |a| a.login_keys));
    }
}

/// A new profile of `kind` for `backend`, not signed in yet: an account with its slot ready for the agent's
/// sign-in, or an API key profile with no keys.
pub async fn add(store: &Store, backend: &str, kind: Kind) -> Result<Value> {
    let agent = agent_id(backend).to_owned();
    if kind == Kind::Account && accounts_of(backend).is_none() {
        let why = format!("{backend} keeps one sign-in: add an API key instead");
        return Err(Refused::error("unsupported", why));
    }
    let profile = change(store, |profiles| {
        let others: Vec<Account> = profiles.iter().map(|p| p.account.clone()).collect();
        let account = Account::new(uuid::Uuid::new_v4().to_string(), &agent, &others);
        let profile = Profile { account, kind, vars: Vec::new() };
        profiles.push(profile.clone());
        Ok(profile)
    })?;
    // A slot that couldn't be set up leaves no profile behind.
    if let Err(error) = share(&profile).await {
        let dropped = change(store, |profiles| {
            profiles.retain(|p| p.account.id != profile.account.id);
            Ok(())
        });
        if let Err(error) = dropped {
            tracing::warn!(error = format!("{error:#}"), "couldn't drop the login profile that wasn't set up");
        }
        return Err(error);
    }
    Ok(profile.wire())
}

/// Signs the profile out and deletes it: an account's slot, an API key profile's keys. Refused while a bot or one
/// of its fallbacks is set to it, an agent still runs on it (a bot moved off it mid-turn) or its sign-in is open.
pub async fn remove(hub: &Hub, id: &str) -> Result<()> {
    let profile = find(&hub.store, id)?;
    let users = users(&hub.store, id)?;
    if !users.is_empty() {
        let why = format!("{} still use this login profile. Move them to another one first.", users.join(", "));
        return Err(Refused::error("in_use", why));
    }
    if RUNNING.locked().iter().any(|running| running == id) || hub.terms.signing_in(id) {
        let why = "An agent is still running on this login profile, or its sign-in is open. Try again once it's done.";
        return Err(Refused::error("in_use", why.into()));
    }
    match profile.kind {
        Kind::Account => account::remove(&profile.account).await?,
        Kind::ApiKey => keys::forget(&hub.store, id)?,
    }
    change(&hub.store, |profiles| {
        profiles.retain(|p| p.account.id != id);
        Ok(())
    })
}

/// The bots whose agent or one of whose fallbacks is set to profile `id`, by name: saving one of them fails once
/// the profile is gone (`settle`).
fn users(store: &Store, id: &str) -> Result<Vec<String>> {
    let uses = |cfg: &BotConfig| {
        cfg.login_profile.as_deref() == Some(id) || cfg.fallbacks.iter().any(|f| f.login_profile.as_deref() == Some(id))
    };
    Ok(store.bots()?.into_iter().filter(|b| !b.deleted && uses(&b.config)).map(|b| b.config.name).collect())
}

/// What `backend`'s process needs to run on `profile`: an account's folder, or an API key profile's variables (with
/// the grants a cloud computer's proxied keys are on). Nothing when it runs on the default login (`resolve`).
pub fn env(store: &Store, backend: &str, profile: Option<&str>) -> Result<(Vec<(String, String)>, Grants)> {
    let Some(profile) = resolve(store, backend, profile)? else { return Ok(Default::default()) };
    match profile.kind {
        Kind::Account => Ok((account::env(&profile.account)?, Grants::default())),
        Kind::ApiKey => keys::env(store, &profile.account.id),
    }
}

/// The names of the variables `profile` sets for `backend`'s process (an account's folder aside); none when it
/// runs on the default login.
pub fn vars(store: &Store, backend: &str, profile: Option<&str>) -> Option<Vec<String>> {
    Some(resolve(store, backend, profile).ok()??.vars)
}

/// Whether running on `profile` reads the vault: an API key profile's variables are kept there.
pub fn reads_vault(store: &Store, backend: &str, profile: Option<&str>) -> bool {
    resolve(store, backend, profile).is_ok_and(|found| found.is_some_and(|p| p.kind == Kind::ApiKey))
}

/// The saved keys that choose `backend`'s login: a process on a profile starts without them, or its agent would
/// run on the key and not on the profile.
pub fn login_keys(backend: &str) -> &'static [&'static str] {
    accounts_of(backend).map_or(&[], |agent| agent.login_keys)
}

/// The profile a bot's agent runs on, when `backend` (the agent it's on now) is the profile's own.
pub fn of_bot<'a>(store: &Store, backend: &str, cfg: &'a BotConfig) -> Option<&'a str> {
    cfg.login_profile.as_deref().filter(|id| resolve(store, backend, Some(id)).is_ok_and(|found| found.is_some()))
}

/// A login of an agent: one of its login profiles, or the default one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub backend: String,
    pub profile: Option<String>,
}

impl Login {
    /// The login `cfg` runs on, as the bot is on now (its own agent, or a fallback step's).
    pub fn of(store: &Store, cfg: &BotConfig) -> Self {
        Self { backend: cfg.backend.clone(), profile: of_bot(store, &cfg.backend, cfg).map(str::to_owned) }
    }
}

/// What a turn showed about the profile's sign-in: a prompt it refused for auth, or one it answered. Whether
/// that's news. A refusal stands over a check that still says signed in (`check`).
pub fn note_signed_in(store: &Store, id: &str, signed_in: bool) -> bool {
    if signed_in {
        backends::clear_prompt_refused(id);
    } else {
        backends::note_prompt_refused(id);
    }
    change(store, |profiles| {
        let account = &mut found(profiles, id)?.account;
        Ok(std::mem::replace(&mut account.signed_in, signed_in) != signed_in)
    })
    .unwrap_or_else(|error| {
        tracing::warn!(error = format!("{error:#}"), "couldn't record the login profile's sign-in");
        false
    })
}

/// Before an agent starts on a profile: an account's slot is brought up to date with the user's own setup
/// (skills, rules, MCP servers, sessions). The process keeps what this returns for as long as it runs.
pub async fn prepare(store: &Store, backend: &str, profile: Option<&str>) -> Result<Option<Running>> {
    let Some(profile) = resolve(store, backend, profile)? else { return Ok(None) };
    let running = Running::on(&profile.account.id);
    share(&profile).await?;
    Ok(Some(running))
}

/// Gives an account's slot the user's setup. An API key profile has no folder.
async fn share(profile: &Profile) -> Result<()> {
    match profile.kind {
        Kind::Account => account::share(&profile.account).await,
        Kind::ApiKey => Ok(()),
    }
}

/// Whether the profile is signed in, and as whom. An account: asked of its agent's CLI, or read from the slot's
/// own files. An API key can't be tried (that would be a call with it): it counts once it has a key.
pub async fn check(store: &Store, id: &str) -> Result<Profile> {
    let profile = find(store, id)?;
    let (said, email) = match profile.kind {
        Kind::ApiKey => (Some(!profile.vars.is_empty()), None),
        Kind::Account => account::state(&profile.account).await?,
    };
    // A check can still say signed in after a prompt was refused for auth (a revoked token, a dead key).
    let signed_in = backends::apply_prompt_refusal(id, said);
    change(store, |profiles| {
        let profile = found(profiles, id)?;
        // A check that couldn't tell keeps what the last one found.
        if let Some(signed_in) = signed_in {
            profile.account.signed_in = signed_in;
            profile.account.email = email.filter(|_| signed_in);
        }
        Ok(profile.clone())
    })
}

/// What to call login profile `profile` of `backend`; none for the default login, or for a profile that isn't
/// `backend`'s.
pub fn name_of(store: &Store, backend: &str, profile: Option<&str>) -> Option<String> {
    Some(resolve(store, backend, profile).ok()??.called())
}

/// Keeps the profiles a bot names its agents' own, for the bot's agent and each fallback: an agent changed to
/// another, or run by a custom command, leaves its profile behind. A profile that is gone is an error.
pub fn settle(store: &Store, cfg: &mut BotConfig) -> Result<()> {
    cfg.login_profile = settled(store, &cfg.backend, cfg.command.as_deref(), cfg.login_profile.take())?;
    for step in &mut cfg.fallbacks {
        step.login_profile = settled(store, &step.backend, step.command.as_deref(), step.login_profile.take())?;
    }
    Ok(())
}

fn settled(store: &Store, backend: &str, command: Option<&str>, profile: Option<String>) -> Result<Option<String>> {
    let Some(id) = profile.filter(|id| !id.is_empty()) else { return Ok(None) };
    let custom = command.is_some_and(|c| !c.trim().is_empty());
    Ok(resolve(store, backend, Some(&id))?.filter(|_| !custom).map(|_| id))
}

#[cfg(test)]
pub(crate) mod tests;
