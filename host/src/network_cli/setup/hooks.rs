//! The hooks that tell a session of new letters (`network hook`): a file of our own for Copilot CLI
//! and Grok; for Codex, a group per event appended to its shared `hooks.json`, never reordered:
//! Codex trusts each hook by its place.

use super::{Failed, json_file, read, write};
use crate::analytics::reported::SetupFailure;
use crate::network_cli::HookAgent;
use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The hook events every hooked agent runs us on.
const EVENTS: [&str; 2] = ["PostToolUse", "Stop"];

/// Where `agent`'s hooks go, in its config folder `dir`.
pub(super) fn file(agent: HookAgent, dir: &Path) -> PathBuf {
    match agent {
        HookAgent::Codex => dir.join("hooks.json"),
        HookAgent::Copilot | HookAgent::Grok => dir.join("hooks/codync.json"),
    }
}

/// Writes `agent`'s hooks to `file`; whether anything changed.
pub(super) fn add(file: &Path, agent: HookAgent, command: &str) -> Result<bool> {
    let old = json_file(file)?;
    let new = match agent {
        HookAgent::Codex => with_ours(old.clone(), command)
            .with_context(|| Failed(SetupFailure::UnreadableConfig, format!("adding hooks to {}", file.display())))?,
        HookAgent::Copilot | HookAgent::Grok => {
            only_ours(&old, file)?;
            own(agent, command)
        }
    };
    if new == old {
        return Ok(false);
    }
    write(file, &new)?;
    Ok(true)
}

/// Takes `agent`'s hooks out of `file`; whether there were any.
pub(super) fn remove(file: &Path, agent: HookAgent) -> Result<bool> {
    if read(file)?.is_none() {
        return Ok(false);
    }
    if !matches!(agent, HookAgent::Codex) {
        only_ours(&json_file(file)?, file)?;
        std::fs::remove_file(file)
            .with_context(|| Failed(SetupFailure::Write, format!("removing {}", file.display())))?;
        return Ok(true);
    }
    let old = json_file(file)?;
    let new = without_ours(old.clone());
    if new == old {
        return Ok(false);
    }
    write(file, &new)?;
    Ok(true)
}

/// Codex's shared file with our group in each event: appended, or an older one of ours replaced
/// where it is.
fn with_ours(mut config: Value, command: &str) -> Result<Value> {
    let Some(root) = config.as_object_mut() else { bail!("it isn't a JSON object") };
    let Some(hooks) = root.entry("hooks").or_insert_with(|| json!({})).as_object_mut() else {
        bail!("its hooks aren't an object")
    };
    for event in EVENTS {
        let Some(list) = hooks.entry(event).or_insert_with(|| json!([])).as_array_mut() else {
            bail!("its {event} hooks aren't a list")
        };
        // An older hook of ours gets the new command where it is; the hooks beside it stay.
        let ours = list
            .iter_mut()
            .filter_map(|g| g.get_mut("hooks").and_then(Value::as_array_mut))
            .flatten()
            .find(|h| is_ours(h));
        match ours {
            Some(hook) => hook["command"] = command.into(),
            None => list.push(json!({"hooks": [{"type": "command", "command": command}]})),
        }
    }
    Ok(config)
}

/// Codex's shared file without our hooks: a group left with none of its own goes too.
fn without_ours(mut config: Value) -> Value {
    for event in EVENTS {
        if let Some(list) = config.get_mut("hooks").and_then(|h| h.get_mut(event)).and_then(Value::as_array_mut) {
            list.retain_mut(|g| {
                let Some(hooks) = g.get_mut("hooks").and_then(Value::as_array_mut) else { return true };
                let before = hooks.len();
                hooks.retain(|h| !is_ours(h));
                hooks.len() == before || !hooks.is_empty()
            });
        }
    }
    config
}

/// A hook that runs `network hook`.
fn is_ours(hook: &Value) -> bool {
    hook["command"].as_str().is_some_and(|c| c.contains("codync-host") && c.contains("network hook"))
}

/// A file of our own holds only our hooks: one the user added a hook to is left alone.
fn only_ours(config: &Value, file: &Path) -> Result<()> {
    fn commands<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    match (k.as_str(), v.as_str()) {
                        ("command" | "bash" | "powershell", Some(c)) => out.push(c),
                        _ => commands(v, out),
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|v| commands(v, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    commands(config, &mut found);
    if found.iter().any(|c| !(c.contains("codync-host") && c.contains("network hook"))) {
        bail!(Failed(SetupFailure::UnreadableConfig, format!("{} has hooks of yours too: left alone", file.display())));
    }
    Ok(())
}

/// A file of our own, in Copilot CLI's shape or (Grok) Codex's.
fn own(agent: HookAgent, command: &str) -> Value {
    let events = |hook: Value| {
        EVENTS.iter().map(|e| ((*e).to_owned(), json!([hook.clone()]))).collect::<serde_json::Map<_, _>>()
    };
    match agent {
        HookAgent::Copilot => json!({"version": 1, "hooks": events(json!({"type": "command", "bash": command}))}),
        _ => json!({"hooks": events(json!({"hooks": [{"type": "command", "command": command}]}))}),
    }
}

#[cfg(test)]
mod tests;
