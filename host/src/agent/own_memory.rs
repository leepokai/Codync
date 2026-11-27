//! The harnesses' own memory, turned off wherever Codync can, so a bot has one memory and the user sees all of it:
//! every bot unless the user keeps its harness's memory on, and Codync's background jobs
//! ([design](../../../docs/design/bot-memory.md#instructions-in-every-harness)). How each harness's is turned off is
//! the agent crate's (`codync_agent::own_memory`); Claude Code also gets its settings through `_meta` (see
//! `agent::bot`).

use crate::store::BotConfig;
use codync_agent::own_memory::{self, Switch};
use serde_json::Value;

/// Whether Codync can turn the bot's harness's own memory off, and so offers the user a switch for it. A harness
/// that reads it from its working folder has one only in a bot's own workspace: a project folder's files are the
/// user's.
pub fn switchable(cfg: &BotConfig) -> bool {
    match own_memory::switch(&cfg.backend) {
        Switch::Always => true,
        Switch::InOwnFolder => crate::agent::workspace::is_managed(cfg),
        Switch::Never => false,
    }
}

/// Whether the bot's harness keeps one memory of its own for all its sessions that Codync can't turn off
/// ([design](../../../docs/design/bot-memory.md#instructions-in-every-harness)). The apps say so where other
/// harnesses have a switch.
pub fn shared(cfg: &BotConfig) -> bool {
    own_memory::shared(&cfg.backend)
}

/// Whether the user kept the bot's harness's own memory on beside Codync's.
pub fn kept(cfg: &BotConfig) -> bool {
    cfg.harness_memory && switchable(cfg)
}

/// Fits the switch to an edited bot: it was turned on for one harness, so another harness (unless the edit sets it
/// too), or a bot where there is no switch, has it off.
pub fn settle(cfg: &mut BotConfig, before: &BotConfig, patch: &Value) {
    if cfg.backend != before.backend && patch.get("harnessMemory").is_none() {
        cfg.harness_memory = false;
    }
    cfg.harness_memory &= switchable(cfg);
}

/// Sets the harness's own memory for one launch of the bot's agent: off, unless the user kept it on.
pub async fn apply(cfg: &BotConfig, env: &mut Vec<(String, String)>, commands: &mut [String]) {
    // An agent on its own (`BotConfig::plain`) keeps its memory, as it has outside Codync.
    let on = cfg.plain || kept(cfg);
    if !on {
        own_memory::turn_off(&cfg.backend, env, commands);
    }
    if crate::agent::workspace::is_managed(cfg) {
        set_in_folder(&cfg.backend, &cfg.cwd, on).await;
    }
}

/// Turns the harness's own memory off for one launch in `workspace`, a folder of Codync's own: its environment,
/// its command, and the settings it reads from the folder.
pub async fn turn_off(backend: &str, workspace: &str, env: &mut Vec<(String, String)>, commands: &mut [String]) {
    own_memory::turn_off(backend, env, commands);
    set_in_folder(backend, workspace, false).await;
}

async fn set_in_folder(backend: &str, folder: &str, on: bool) {
    let (backend, folder) = (backend.to_owned(), folder.to_owned());
    match tokio::task::spawn_blocking(move || own_memory::set_in_folder(&backend, std::path::Path::new(&folder), on))
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(error = format!("{error:#}"), "couldn't set the harness's own memory"),
        Err(error) => tracing::warn!(%error, "couldn't set the harness's own memory"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_switch_is_offered_only_where_codync_can_turn_it_off() {
        let id = "5f0c7a52-0000-4000-8000-000000000004";
        let bot = |backend: &str, cwd: &str| -> BotConfig {
            serde_json::from_value(
                json!({"id": id, "name": "B", "backend": backend, "cwd": cwd, "harnessMemory": true}),
            )
            .unwrap()
        };
        let own = crate::agent::workspace::path(id).to_string_lossy().into_owned();
        assert!(kept(&bot("claude", "/work/app")), "Claude Code's is switched through its settings, in any folder");
        assert!(
            switchable(&bot("workbuddy", "/work/app")),
            "WorkBuddy is the CodeBuddy CLI and honors the same switch"
        );
        assert!(kept(&bot("qwen", &own)));
        assert!(!switchable(&bot("qwen", "/work/app")), "Qwen Code's lives in the project's own files");
        assert!(!switchable(&bot("codex", &own)) && !switchable(&bot("gemini", &own)));
        // Where there's no switch, the apps say when every bot on the harness shares its memory.
        assert!(shared(&bot("gemini", &own)) && shared(&bot("hermes", "/work/app")));
        assert!(!shared(&bot("claude", &own)) && !shared(&bot("codex", &own)));
        let mut off = bot("claude", &own);
        off.harness_memory = false;
        assert!(!kept(&off));
    }

    #[test]
    fn an_edit_to_another_harness_turns_the_switch_off() {
        let bot = |backend: &str| -> BotConfig {
            serde_json::from_value(
                json!({"id": "a", "name": "B", "backend": backend, "cwd": "/work/app", "harnessMemory": true}),
            )
            .unwrap()
        };
        let mut moved = bot("copilot");
        settle(&mut moved, &bot("claude"), &json!({"backend": "copilot"}));
        assert!(!moved.harness_memory, "the user turned it on for Claude Code, not Copilot");
        let mut chosen = bot("copilot");
        settle(&mut chosen, &bot("claude"), &json!({"backend": "copilot", "harnessMemory": true}));
        assert!(chosen.harness_memory);
        let mut none = bot("codex");
        settle(&mut none, &bot("codex"), &json!({"harnessMemory": true}));
        assert!(!none.harness_memory, "no switch to keep on");
    }
}
