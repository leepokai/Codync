//! Each harness's own memory, which an app may want off so the agent keeps one memory of the app's: how each one is
//! turned off for a launch (its environment, its command, the settings it reads from its working folder), and which
//! keep one memory for all their sessions that can't be turned off. Claude Code is also switched through its
//! session settings (`autoMemoryEnabled`, `autoDreamEnabled`), which the app sends with its session.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;

/// Where a harness's own memory can be turned off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    /// For any launch, by its environment, command or session settings.
    Always,
    /// Only through settings in its working folder: a folder the app owns, never the user's project.
    InOwnFolder,
    /// Not at all.
    Never,
}

/// Where harness `id`'s own memory can be turned off.
pub fn switch(id: &str) -> Switch {
    match id {
        "claude" | "copilot" | "codebuddy" | "workbuddy" => Switch::Always,
        "qwen" => Switch::InOwnFolder,
        _ => Switch::Never,
    }
}

/// Whether harness `id` keeps one memory of its own for all its sessions that can't be turned off: every session
/// on it, and the harness used outside the app, share it. Gemini CLI's is `~/.gemini/GEMINI.md`, Hermes Agent's
/// `~/.hermes/memories`.
pub fn shared(id: &str) -> bool {
    matches!(id, "gemini" | "hermes")
}

/// For any harness's process: a harness ignores the variables meant for the others.
const ENV: &[(&str, &str)] = &[("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"), ("CODEBUDDY_DISABLE_AUTO_MEMORY", "1")];

/// Turns harness `id`'s own memory off for one launch: its environment and its ACP `commands`.
pub fn turn_off(id: &str, env: &mut Vec<(String, String)>, commands: &mut [String]) {
    env.extend(ENV.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
    // Copilot Memory writes through these two tools.
    let args = if id == "copilot" { " --excluded-tools=store_memory,vote_memory" } else { "" };
    for command in commands {
        command.push_str(args);
    }
}

/// Turns Claude Code's own memory and its auto-dream off in its session `settings` (claude-agent-acp's
/// `_meta.claudeCode.options.settings`). Kept on, Claude Code sets both itself.
pub fn claude_settings_off(settings: &mut Value) {
    settings["autoMemoryEnabled"] = false.into();
    settings["autoDreamEnabled"] = false.into();
}

/// Blocking: the settings harness `id` reads from `folder` itself, set off, or (`on`) taken out again so the
/// harness's own setting applies. Only for a folder the app owns.
pub fn set_in_folder(id: &str, folder: &Path, on: bool) -> Result<()> {
    match id {
        "qwen" => edit(&folder.join(".qwen/settings.json"), |settings| {
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
    fn folder_settings_keep_what_the_user_set() {
        let dir = std::env::temp_dir().join(format!("codync-agent-own-memory-{}", uuid::Uuid::new_v4()));
        let file = dir.join(".qwen/settings.json");
        std::fs::create_dir_all(dir.join(".qwen")).expect("dir");
        std::fs::write(&file, r#"{"model": {"name": "qwen3"}, "memory": {"other": 1}}"#).expect("settings");
        let settings =
            || -> Value { serde_json::from_str(&std::fs::read_to_string(&file).expect("settings")).expect("json") };
        set_in_folder("qwen", &dir, false).expect("set");
        assert_eq!(settings()["model"]["name"], "qwen3");
        assert_eq!(
            settings()["memory"],
            json!({"other": 1, "enableManagedAutoMemory": false, "enableManagedAutoDream": false})
        );
        set_in_folder("qwen", &dir, true).expect("set");
        assert_eq!(settings()["memory"], json!({"other": 1}), "kept on, Qwen Code's own setting applies again");
        set_in_folder("codex", &dir.join("none"), false).expect("set");
        assert!(!dir.join("none").exists(), "nothing for other harnesses");
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn copilot_loses_its_memory_tools() {
        let (mut env, mut commands) = (Vec::new(), ["copilot --acp".to_owned()]);
        turn_off("copilot", &mut env, &mut commands);
        assert_eq!(commands[0], "copilot --acp --excluded-tools=store_memory,vote_memory");
        assert!(env.iter().any(|(k, _)| k == "CLAUDE_CODE_DISABLE_AUTO_MEMORY"));
        let mut commands = ["claude-agent-acp".to_owned()];
        turn_off("claude", &mut env, &mut commands);
        assert_eq!(commands[0], "claude-agent-acp");
    }
}
