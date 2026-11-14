//! An account login profile's side: the folder (its slot) the agent keeps that sign-in in, the sign-in itself, and
//! who each agent's default login is signed in as. What each agent needs for it is the `codync-accounts` crate's.

use super::{accounts_of, find_account};
use crate::LockExt;
use crate::agent::backends;
use crate::store::Store;
use anyhow::{Context, Result, anyhow};
use codync_accounts::{Account, Agent, SignedIn};
use std::path::PathBuf;
use std::sync::Mutex;

/// Where every account's slot is.
fn slots() -> PathBuf {
    crate::service::data_dir().join("login-profiles")
}

/// Who the default login of each agent is signed in as, read from its files by [`refresh_default`].
static DEFAULT: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());

/// Who agent `id`'s default login is signed in as, as last read.
pub(super) fn default_identity(id: &str) -> Option<String> {
    DEFAULT.locked().iter().find(|(agent, _)| *agent == id).map(|(_, email)| email.clone())
}

/// Reads again who each agent's default login is signed in as.
pub async fn refresh_default() {
    let Some(home) = dirs::home_dir() else { return };
    let read = move || {
        let named = |agent: &'static Agent| agent.own_identity(&home).map(|email| (agent.id, email));
        codync_accounts::agents().iter().filter_map(named).collect::<Vec<_>>()
    };
    match tokio::task::spawn_blocking(read).await {
        Ok(found) => *DEFAULT.locked() = found,
        Err(error) => tracing::warn!(%error, "couldn't read who the agents are signed in as"),
    }
}

/// The variables that point `account`'s agent at its slot.
pub(super) fn env(account: &Account) -> Result<Vec<(String, String)>> {
    let Some(agent) = accounts_of(&account.agent) else { return Ok(Vec::new()) };
    let mut env = agent.env(&account.slot(&slots())?, &account.id);
    super::only_own_login(&account.agent, &mut env);
    Ok(env)
}

/// Gives `account`'s slot the user's own setup (skills, rules, MCP servers, sessions).
pub(super) async fn share(account: &Account) -> Result<()> {
    let Some(agent) = accounts_of(&account.agent) else { return Ok(()) };
    let dir = account.slot(&slots())?;
    let home = dirs::home_dir().context("no home folder")?;
    tokio::task::spawn_blocking(move || agent.share(&home, &dir)).await?.context("setting up the account's folder")
}

/// Signs `account` out and deletes its slot.
pub(super) async fn remove(account: &Account) -> Result<()> {
    let dir = account.slot(&slots())?;
    let agent = accounts_of(&account.agent).zip(backends::harness_of(&account.agent));
    // Its own CLI forgets a login kept outside the slot (on macOS, Claude Code's is in the Keychain).
    if let Some((agent, h)) = agent
        && let Some(sign_out) = agent.sign_out
        && backends::run_cli(h, sign_out, &env(account)?).await.is_none()
    {
        tracing::warn!(agent = h.id, "couldn't sign the account out");
    }
    tokio::task::spawn_blocking(move || match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("removing {}", dir.display()))
        }
        _ => Ok(()),
    })
    .await?
}

/// The command that signs account profile `id` in, for a setup terminal. Never an API key profile's: its
/// variables would show in the terminal.
pub async fn login_command(store: &Store, id: &str) -> Result<String> {
    let account = find_account(store, id)?;
    let env = env(&account)?;
    let prefix = crate::shell::env_prefix(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    Ok(format!("{prefix}{}", backends::login_command(&account.agent).await?))
}

/// `account`'s sign-in state (none: its check couldn't tell) and who it signs in: asked of its agent's CLI, or
/// read from the slot's own files.
pub(super) async fn state(account: &Account) -> Result<(Option<bool>, Option<String>)> {
    let (agent, h) = accounts_of(&account.agent)
        .zip(backends::harness_of(&account.agent))
        .ok_or_else(|| anyhow!("{} has no accounts", account.agent))?;
    let dir = account.slot(&slots())?;
    let env = env(account)?;
    let said = match agent.signed_in {
        SignedIn::AgentCheck => backends::cli_signed_in(h, &env).await,
        SignedIn::Lists { args, none } => backends::run_cli(h, args, &env)
            .await
            .map(|out| out.status.success() && !String::from_utf8_lossy(&out.stdout).contains(none)),
        SignedIn::File(_) | SignedIn::Logins(_) => None,
    };
    let (on_disk, email) =
        tokio::task::spawn_blocking(move || (agent.signed_in_on_disk(&dir), agent.identity(&dir))).await?;
    Ok((said.or(on_disk), email))
}
