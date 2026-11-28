//! What a change to a bot's setup sets, checked against the agents' model catalogs (`agent::auth::models`): the
//! model it leaves the bot on and each fallback's model are ones their agent offers, and the model's settings
//! (`modelOptions`) are ones it lists. The catalogs come from probe sessions the host starts for that alone; loading
//! one counts against the call's `deadline`, which its approval card shares. A backend a custom `command` starts has
//! its model list as the command's to answer: its model is taken as it comes (probing it would run the command) but
//! its settings are refused. A CLI that isn't installed (checking it would start a registry build) is taken as it
//! comes.

use super::Refused;
use crate::hub::Hub;
use crate::store::Fallback;
use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::Instant;

/// The model-level part of a change: the model it sets (`set`, none: not touched), the model the bot is on after it
/// (`effective`, none: the agent's default), the settings it sets for that model, and the fallbacks it replaces the
/// chain with (empty: not touched).
pub(super) struct Models<'a> {
    pub set: Option<&'a str>,
    pub effective: Option<&'a str>,
    pub options: Option<&'a Map<String, Value>>,
    pub fallbacks: &'a [Fallback],
}

/// Checked before the change's approval card. Loading an agent's model list goes at most once per call; a list that
/// didn't load accepts a model (the bot's session reports it clearly) but refuses settings (`modelOptions`), whose
/// wrong value would keep the session from starting at all.
pub(super) async fn change(
    hub: &Arc<Hub>,
    backend: &str,
    command: Option<&str>,
    login_profile: Option<&str>,
    models: Models<'_>,
    deadline: Instant,
) -> Result<()> {
    let mut fetched: HashMap<(String, Option<String>), Result<Value, String>> = HashMap::new();
    let options = models.options.filter(|o| !o.is_empty());
    let set = models.set.filter(|m| !m.is_empty());
    if command.is_some_and(|c| !c.trim().is_empty()) {
        // The command's model list is its own to answer: the model id is taken as it comes (probing it would
        // run the command), but its settings are refused — a wrong value keeps the session from starting at
        // all, and there is no list to check them against. The fallbacks below are still checked against
        // their own agents.
        if options.is_some() {
            return Err(Refused::error(
                "options",
                "the bot runs a custom command, whose model list is its own to answer: modelOptions can't be checked against it, so they are refused".to_owned(),
            ));
        }
    } else if set.is_some() || options.is_some() {
        // A change that only moves the model probes agents that are here; settings need the list either way.
        // `installed` stats every candidate on PATH. That runs off this worker, so a slow folder there
        // cannot stall other tasks. Same rule as a save's folder check.
        if options.is_some() || cli_installed(backend).await? {
            match catalog(hub, &mut fetched, backend, login_profile, deadline).await {
                Ok(catalog) => {
                    if let Some(m) = set {
                        codync_agent::acp::models::check_model(&catalog, m)
                            .map_err(|e| Refused::error("model", format!("{e}{}", engine_hint(m, backend))))?;
                    }
                    if let Some(options) = options {
                        let settings = options.iter().map(|(id, v)| (id.as_str(), v.as_str().unwrap_or_default()));
                        codync_agent::acp::models::check_options(
                            &catalog,
                            models.effective.filter(|m| !m.is_empty()),
                            settings,
                        )
                        .map_err(|e| Refused::error("options", e.to_string()))?;
                    }
                }
                Err(e) if options.is_some() => {
                    return Err(Refused::error(
                        "catalog",
                        format!("couldn't load the agent's model list to check modelOptions: {e:#}"),
                    ));
                }
                Err(e) => {
                    tracing::warn!(backend, error = format!("{e:#}"), "a bot's model goes unchecked");
                }
            }
        }
    }
    for fallback in models.fallbacks {
        let Some(m) = fallback.model.as_deref().filter(|m| !m.is_empty()) else { continue };
        if fallback.command.as_deref().is_some_and(|c| !c.trim().is_empty()) {
            continue;
        }
        if !cli_installed(&fallback.backend).await? {
            tracing::warn!(
                backend = %fallback.backend,
                model = m,
                "a fallback's agent isn't installed here; its model goes unchecked"
            );
            continue;
        }
        match catalog(hub, &mut fetched, &fallback.backend, fallback.login_profile.as_deref(), deadline).await {
            Ok(catalog) => {
                codync_agent::acp::models::check_model(&catalog, m)
                    .map_err(|e| Refused::error("model", format!("{e}{}", engine_hint(m, &fallback.backend))))?;
            }
            Err(e) => {
                tracing::warn!(
                    backend = %fallback.backend,
                    model = m,
                    error = format!("{e:#}"),
                    "a fallback's model goes unchecked"
                );
            }
        }
    }
    Ok(())
}

/// A model id often carries its engine's name (`grok-4.7`, `claude-sonnet-4-6`, `gpt-5.4`); when it clearly
/// belongs to a different backend than the one being checked, say so — a bare "not in the list" sent a
/// manager hunting for a model that was never this engine's to give.
fn engine_hint(model: &str, backend: &str) -> String {
    const FAMILIES: &[(&str, &str)] =
        &[("grok", "grok"), ("gpt-", "codex"), ("codex", "codex"), ("claude", "claude"), ("cursor", "cursor")];
    let bare = model.rsplit('/').next().unwrap_or(model).to_ascii_lowercase();
    match FAMILIES.iter().find(|(token, _)| bare.starts_with(token)) {
        Some((_, family)) if *family != backend => {
            format!(" — {model} looks like a {family}-backend model, and this bot's backend is {backend}")
        }
        _ => String::new(),
    }
}

#[cfg(test)]
tokio::task_local! {
    static INSTALL_CHECK_THREAD: std::sync::mpsc::Sender<std::thread::ThreadId>;
}

/// Whether `id`'s CLI is on this computer. `backends::installed` searches PATH and stats each candidate
/// (`which` → `is_file`), which is blocking filesystem work, so it runs on the blocking pool. A join
/// error is returned: the check did not run, and a model change must not proceed as if it had.
async fn cli_installed(id: &str) -> Result<bool> {
    let id = id.to_owned();
    #[cfg(test)]
    let observer = INSTALL_CHECK_THREAD.try_with(Clone::clone).ok();
    let found = tokio::task::spawn_blocking(move || {
        #[cfg(test)]
        if let Some(observer) = observer {
            let _ = observer.send(std::thread::current().id());
        }
        codync_agent::backends::installed(&id)
    })
    .await?;
    Ok(found)
}

/// `agent`'s model catalog on its login profile, loaded at most once per call. `Err` says why it didn't load: the
/// agent, or the `deadline` the call's approval card shares.
async fn catalog(
    hub: &Arc<Hub>,
    fetched: &mut HashMap<(String, Option<String>), Result<Value, String>>,
    agent: &str,
    login_profile: Option<&str>,
    deadline: Instant,
) -> Result<Value> {
    let login_profile = login_profile.filter(|s| !s.is_empty());
    let key = (agent.to_owned(), login_profile.map(str::to_owned));
    if let Some(catalog) = fetched.get(&key) {
        return catalog.clone().map_err(anyhow::Error::msg);
    }
    let catalog = match tokio::time::timeout_at(
        deadline,
        crate::agent::auth::shared_models(hub, agent, login_profile, crate::agent::auth::ModelListWanted::Fresh),
    )
    .await
    {
        Ok(catalog) => catalog.map_err(|e| format!("{e:#}")),
        Err(_) => Err("the agent's model list took too long to load".to_owned()),
    };
    fetched.insert(key, catalog.clone());
    catalog.map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_hint_names_the_backend_a_model_belongs_to() {
        assert_eq!(
            engine_hint("grok-4.7", "opencode"),
            " — grok-4.7 looks like a grok-backend model, and this bot's backend is opencode"
        );
        assert_eq!(
            engine_hint("gpt-5.4", "claude"),
            " — gpt-5.4 looks like a codex-backend model, and this bot's backend is claude"
        );
        assert_eq!(
            engine_hint("provider/cursor-max", "claude"),
            " — provider/cursor-max looks like a cursor-backend model, and this bot's backend is claude",
            "a prefixed id is read by its bare name"
        );
        assert_eq!(engine_hint("claude-sonnet-4-6", "claude"), String::new(), "its own family says nothing");
        assert_eq!(engine_hint("opus", "claude"), String::new(), "an unrecognized id says nothing");
    }

    /// The PATH search has to leave the async worker. An unknown id is not installed, and the thread
    /// that answered is the blocking pool's, not the one polling this test.
    #[tokio::test(flavor = "current_thread")]
    async fn an_install_check_leaves_the_async_worker() {
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::current().id();
        let found =
            INSTALL_CHECK_THREAD.scope(tx, cli_installed("not-a-real-codync-agent")).await.expect("install check");
        assert!(!found, "an unknown agent is not installed");
        let ran_on = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("the check recorded its thread");
        assert_ne!(ran_on, worker, "the PATH search must not run on the async worker");
    }
}
