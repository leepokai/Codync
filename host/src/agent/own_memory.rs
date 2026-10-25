//! The harnesses' own memory, turned off wherever Codync can, so a bot has one memory and the user sees all of it:
//! every bot unless the user keeps its harness's memory on, and Codync's background jobs
//! ([design](../../../docs/design/bot-memory.md#instructions-in-every-harness)). Claude Code also gets its settings
//! through `_meta` (see `agent::bot`).

use crate::store::BotConfig;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;

/// For the agent's process. A harness ignores the variables meant for the others.
pub const ENV: &[(&str, &str)] = &[("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"), ("CODEBUDDY_DISABLE_AUTO_MEMORY", "1")];

/// Appended to the harness's ACP command.
pub fn args(backend: &str) -> &'static str {
    match backend {
        // Copilot Memory writes through these two tools.
        "copilot" => " --excluded-tools=store_memory,vote_memory",
        _ => "",
    }
}

/// Whether Codync can turn the bot's harness's own memory off, and so offers the user a switch for it. Qwen Code
/// reads it from the workspace folder, which Codync writes only in a bot's own workspace: a project folder's files
/// are the user's.
pub fn switchable(cfg: &BotConfig) -> bool {
    match cfg.backend.as_str() {
        "claude" | "copilot" | "codebuddy" | "workbuddy" => true,
        "qwen" => crate::agent::workspace::is_managed(cfg),
        _ => false,
    }
}

/// Whether the bot's harness keeps one memory of its own for all its sessions that Codync can't turn off: every bot
/// on it, and the harness used outside Codync, share it. Gemini CLI's is `~/.gemini/GEMINI.md`, Hermes Agent's
/// `~/.hermes/memories` ([design](../../../docs/design/bot-memory.md#instructions-in-every-harness)). The apps say so
/// where other harnesses have a switch.
pub fn shared(cfg: &BotConfig) -> bool {
    matches!(cfg.backend.as_str(), "gemini" | "hermes")
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
    let on = kept(cfg);
    if !on {
        switch_off(&cfg.backend, env, commands);
    }
    if crate::agent::workspace::is_managed(cfg) {
        write_settings(&cfg.backend, &cfg.cwd, on).await;
    }
}

/// Turns the harness's own memory off for one launch in `workspace`, a folder of Codync's own: its environment,
/// its command, and the settings it reads from the folder.
pub async fn turn_off(backend: &str, workspace: &str, env: &mut Vec<(String, String)>, commands: &mut [String]) {
    switch_off(backend, env, commands);
    write_settings(backend, workspace, false).await;
}

fn switch_off(backend: &str, env: &mut Vec<(String, String)>, commands: &mut [String]) {
    env.extend(ENV.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
    for command in commands {
        command.push_str(args(backend));
    }
}

async fn write_settings(backend: &str, workspace: &str, on: bool) {
    let (backend, workspace) = (backend.to_owned(), workspace.to_owned());
    match tokio::task::spawn_blocking(move || prepare_workspace(&backend, Path::new(&workspace), on)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(error = format!("{error:#}"), "couldn't set the harness's own memory"),
        Err(error) => tracing::warn!(%error, "couldn't set the harness's own memory"),
    }
}

/// Settings a harness reads from the workspace folder itself: off, or (kept on) taken out again so the harness's
/// own setting applies.
fn prepare_workspace(backend: &str, workspace: &Path, on: bool) -> Result<()> {
    match backend {
        "qwen" => edit(&workspace.join(".qwen/settings.json"), |settings| {
            const KEYS: [&str; 2] = ["enableManagedAutoMemory", "enableManagedAutoDream"];
            if on {
                if let Some(memory) = settings["memory"].as_object_mut() {
                    for key in KEYS {
                        memory.remove(key);
                    }
                }
            } else {
                overlay(settings, &json!({"memory": {KEYS[0]: false, KEYS[1]: false}}));
            }
        }),
        _ => Ok(()),
    }
}

/// Changes the JSON file at `path` (none yet: `{}`), keeping the rest of it, and writes it only if it changed.
fn edit(path: &Path, change: impl FnOnce(&mut Value)) -> Result<()> {
    let mut current = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let before = current.clone();
    change(&mut current);
    if current == before {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&current)?).with_context(|| format!("writing {}", path.display()))
}

fn overlay(target: &mut Value, settings: &Value) {
    match (target, settings) {
        (Value::Object(target), Value::Object(settings)) => {
            for (key, value) in settings {
                overlay(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (target, value) => *target = value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_settings_keep_what_the_user_set() {
        let dir = std::env::temp_dir().join(format!("codync-own-memory-{}", uuid::Uuid::new_v4()));
        let file = dir.join(".qwen/settings.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, r#"{"model": {"name": "qwen3"}, "memory": {"other": 1}}"#).unwrap();
        prepare_workspace("qwen", &dir, false).unwrap();
        let settings = || -> Value { serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap() };
        assert_eq!(settings()["model"]["name"], "qwen3");
        assert_eq!(
            settings()["memory"],
            json!({"other": 1, "enableManagedAutoMemory": false, "enableManagedAutoDream": false})
        );
        prepare_workspace("qwen", &dir, true).unwrap();
        assert_eq!(settings()["memory"], json!({"other": 1}), "kept on, Qwen Code's own setting applies again");
        assert_eq!(settings()["model"]["name"], "qwen3");
        prepare_workspace("codex", &dir.join("none"), false).unwrap();
        assert!(!dir.join("none").exists(), "nothing for other harnesses");
        std::fs::remove_dir_all(dir).unwrap();
    }

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
