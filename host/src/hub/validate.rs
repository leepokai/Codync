//! What a bot's setup must hold before it's saved, whoever changes it (a client, a managing bot, the host).

use crate::store::BotConfig;
use anyhow::{Result, bail};

pub(crate) fn validate(cfg: &BotConfig) -> Result<()> {
    validate_with(cfg, true)
}

/// `workspace_checked` false: an update that leaves a group's saved folder as it is. A folder renamed or removed
/// since it was saved must not block renaming the group or changing its members (the room reports it missing until
/// the user fixes it; `chat::group::workspace`).
pub(crate) fn validate_with(cfg: &BotConfig, workspace_checked: bool) -> Result<()> {
    if cfg.name.trim().is_empty() {
        bail!("name is required");
    }
    if let Some(workspace) = cfg.workspace.as_deref() {
        if !cfg.is_group() {
            bail!("workspace is a group option; a bot's folder is its own workspace setting");
        }
        if workspace_checked {
            if !std::path::Path::new(workspace).is_absolute() {
                bail!("group workspace must be an absolute path");
            }
            if !std::path::Path::new(workspace).is_dir() {
                bail!("group workspace folder does not exist: {workspace}");
            }
        }
    }
    if cfg.is_group() {
        return Ok(());
    }
    // The bot a call or shortcut that names none talks to stays in sight.
    if cfg.lead && cfg.hidden {
        bail!("the lead bot can't be hidden");
    }
    if !cfg.cwd.is_empty() && !std::path::Path::new(&cfg.cwd).is_dir() {
        bail!("workspace folder does not exist: {}", cfg.cwd);
    }
    if cfg.command.as_deref().map(str::trim).unwrap_or_default().is_empty()
        && !crate::agent::backends::is_known(&cfg.backend)
    {
        bail!("unknown backend {}", cfg.backend);
    }
    // An id the agent listed (`agent::modes`), sent back to it as is.
    if cfg.mode.as_deref().is_some_and(|m| m.trim().is_empty() || m.len() > 64) {
        bail!("mode must be one of the agent's mode ids, or null for its default");
    }
    if !(1..=crate::store::MAX_THREADS).contains(&cfg.max_threads) {
        bail!("maxThreads is 1 to {}", crate::store::MAX_THREADS);
    }
    cfg.compaction.check().map_err(anyhow::Error::msg)?;
    crate::agent::fallback::check(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_shared_workspace_is_groups_only_and_must_exist() {
        let mut group: BotConfig =
            serde_json::from_value(json!({"id": "g", "kind": "group", "name": "Crew", "members": ["a"]})).unwrap();
        assert!(validate(&group).is_ok(), "no workspace: unchanged behavior");
        group.workspace = Some(String::new());
        assert!(validate(&group).is_err(), "empty isn't a folder");
        group.workspace = Some("team".into());
        assert!(validate(&group).is_err(), "relative paths are refused");
        group.workspace = Some("/definitely/not/a-codync-test-folder".into());
        assert!(validate(&group).is_err(), "missing folders are refused");

        let dir = std::env::temp_dir().join(format!("codync-validate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        group.workspace = Some(dir.to_string_lossy().into_owned());
        assert!(validate(&group).is_ok(), "an existing absolute folder passes");

        let mut bot: BotConfig =
            serde_json::from_value(json!({"id": "b", "name": "Bob", "backend": "claude", "cwd": "/tmp"})).unwrap();
        bot.workspace = Some(dir.to_string_lossy().into_owned());
        assert!(validate(&bot).is_err(), "a bot's folder is its own workspace setting, not `workspace`");
        std::fs::remove_dir_all(dir).ok();

        // A folder gone since it was saved doesn't block an edit that leaves it as it is.
        let mut group: BotConfig = serde_json::from_value(
            json!({"id": "g", "kind": "group", "name": "Crew", "members": ["a"], "workspace": "/gone/team"}),
        )
        .unwrap();
        assert!(validate_with(&group, true).is_err(), "a new or changed folder must exist");
        assert!(validate_with(&group, false).is_ok(), "a rename or member change goes through anyway");
        group.workspace = None;
        group.name = "Crew 2".into();
        assert!(validate_with(&group, true).is_ok(), "clearing it is fine too");
    }
}
