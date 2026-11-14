//! An API key login profile's variables: named environment variables the user typed, or picked from their login
//! shell, kept in the vault and set for the agent's process. Their values never reach a client or a log; clients
//! see the names. Nothing reads the login shell when an agent starts: an import is a copy taken when the user
//! asks for it.

use super::{Kind, PROFILE_VAR, Profile, Refused, change, find, found};
use crate::agent::auth;
use crate::keeper::client::Grants;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// Most variables one profile holds.
const MAX_VARS: usize = 16;
/// The login shell's variables that are the shell's own, or the computer's, and never a key.
const NOT_KEYS: [&str; 12] =
    ["PATH", "HOME", "PWD", "OLDPWD", "SHLVL", "_", "SHELL", "USER", "LOGNAME", "TERM", "TMPDIR", "LANG"];

/// Where profile `id`'s variables are in the vault.
fn slot(id: &str) -> String {
    format!("login-profile:{id}")
}

/// A variable the user may keep in a profile: a name of their own, never Codync's.
fn offered(name: &str) -> bool {
    crate::shell::is_env_name(name)
        && !name.starts_with("CODYNC_")
        && !name.starts_with("LC_")
        && !NOT_KEYS.contains(&name)
}

/// Profile `id`'s variables, for its agent's process, with the grants a cloud computer's proxied keys are on.
pub(super) fn env(store: &Store, id: &str) -> Result<(Vec<(String, String)>, Grants)> {
    let (mut env, grants) = auth::env_without(store, &slot(id), &[])?;
    env.push((PROFILE_VAR.to_owned(), id.to_owned()));
    Ok((env, grants))
}

/// Forgets profile `id`'s variables.
pub(super) fn forget(store: &Store, id: &str) -> Result<()> {
    auth::forget_env(store, &slot(id))
}

/// The names of the login shell's variables the user can import, sorted.
pub async fn shell_names() -> Result<Vec<String>> {
    let names: BTreeSet<String> = shell_env().await?.into_iter().map(|(name, _)| name).filter(|n| offered(n)).collect();
    Ok(names.into_iter().collect())
}

/// The login shell's variables, as a fresh terminal has them.
async fn shell_env() -> Result<Vec<(String, String)>> {
    tokio::task::spawn_blocking(crate::shell::user_env).await?.context("couldn't read your login shell's variables")
}

/// Sets API key profile `id`'s variables: `typed` (name → value; an empty value removes it) and `import`, names
/// whose values are read from the login shell now. Returns the profile as it is then.
pub async fn set(store: &Store, id: &str, typed: &Map<String, Value>, import: &[String]) -> Result<Profile> {
    if find(store, id)?.kind != Kind::ApiKey {
        bail!("that login profile signs in with an account, not a key");
    }
    if let Some(name) = typed.keys().chain(import).find(|name| !offered(name)) {
        return Err(Refused::error("bad_name", format!("`{name}` can't be a login profile's variable")));
    }
    let mut changes = auth::env_changes(typed)?;
    if !import.is_empty() {
        let shell = shell_env().await?;
        for name in import {
            let Some((_, value)) = shell.iter().find(|(n, _)| n == name) else {
                return Err(Refused::error("not_in_shell", format!("your login shell doesn't set `{name}`")));
            };
            changes.push((name.clone(), Some(value.clone())));
        }
    }
    // The vault and the profile's list of names change together, so two edits at once can't lose each other.
    let profile = change(store, |profiles| {
        let profile = found(profiles, id)?;
        let mut names: BTreeSet<String> = auth::saved_names(store, &slot(id))?.into_iter().collect();
        for (name, value) in &changes {
            if value.is_some() {
                names.insert(name.clone());
            } else {
                names.remove(name);
            }
        }
        if names.len() > MAX_VARS {
            return Err(Refused::error("too_many", format!("a login profile holds at most {MAX_VARS} variables")));
        }
        auth::save_env(store, &slot(id), &changes)?;
        profile.vars = names.into_iter().collect();
        profile.account.signed_in = !profile.vars.is_empty();
        Ok(profile.clone())
    })?;
    // New keys are a new sign-in: the prompt the old ones got refused no longer counts.
    crate::agent::backends::clear_prompt_refused(id);
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_users_own_variables_are_offered() {
        for name in ["MY_KEY", "ANTHROPIC_API_KEY", "OPENAI_BASE_URL", "a1", "1KEY"] {
            assert!(offered(name), "{name}");
        }
        for name in ["PATH", "HOME", "_", "SHLVL", "CODYNC_HOME", "LC_ALL", "MY-KEY", "MY KEY", ""] {
            assert!(!offered(name), "{name}");
        }
    }
}
