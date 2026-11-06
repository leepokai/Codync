//! Adds and removes Codync's hook in Claude Code's `settings.json`, leaving the person's own hooks alone.

use crate::service::{read_settings, write_settings};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::path::Path;

/// The events Codync listens to; the rest of Claude Code's are of no use to it.
const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Stop",
    "SessionEnd",
];

/// Seconds Claude Code lets a hook run. Only a permission request waits for a person.
const PERMISSION_TIMEOUT: u64 = 120;
const QUICK_TIMEOUT: u64 = 10;
/// The tools whose answer a person may give from any client: Claude Code's questions and plan approvals.
const ASK_MATCHER: &str = "AskUserQuestion|ExitPlanMode";
/// Marks the hook that waits for that answer; the plain `PreToolUse` hook never does.
pub const ASK_FLAG: &str = "--ask";

/// The command Claude Code runs. A forward-slash path in double quotes works in Git Bash and PowerShell alike.
fn command(exe: &Path) -> String {
    let path = exe.to_string_lossy();
    if cfg!(windows) {
        format!("\"{}\" hook", path.replace('\\', "/"))
    } else {
        let quoted = shlex::try_quote(&path).map_or_else(|_| format!("'{path}'"), std::borrow::Cow::into_owned);
        format!("{quoted} hook")
    }
}

fn is_ours(command: &str) -> bool {
    (command.ends_with(" hook") || command.ends_with(&format!(" hook {ASK_FLAG}"))) && command.contains("codync-host")
}

fn is_ask_group(group: &Value) -> bool {
    group["matcher"].as_str() == Some(ASK_MATCHER) && group_is_ours(group)
}

fn group_is_ours(group: &Value) -> bool {
    group["hooks"].as_array().into_iter().flatten().any(|h| h["command"].as_str().is_some_and(is_ours))
}

fn hooks_of(root: &mut Value) -> Result<&mut serde_json::Map<String, Value>> {
    let Some(root) = root.as_object_mut() else { bail!("Claude Code's settings aren't a JSON object") };
    match root.entry("hooks").or_insert_with(|| json!({})).as_object_mut() {
        Some(hooks) => Ok(hooks),
        None => bail!("the `hooks` setting of Claude Code isn't an object"),
    }
}

/// Points every event at `exe`. True when the file changed.
pub fn install(settings: &Path, exe: &Path) -> Result<bool> {
    let mut root = read_settings(settings)?.unwrap_or_else(|| json!({}));
    let before = root.clone();
    let hooks = hooks_of(&mut root)?;
    for event in EVENTS {
        let Some(groups) = hooks.entry(*event).or_insert_with(|| json!([])).as_array_mut() else {
            bail!("the `{event}` hooks of Claude Code aren't a list");
        };
        // A host that moved or was reinstalled leaves a stale command behind: replace it.
        groups.retain(|g| !group_is_ours(g));
        let timeout = if *event == "PermissionRequest" { PERMISSION_TIMEOUT } else { QUICK_TIMEOUT };
        groups.push(json!({"hooks": [{"type": "command", "command": command(exe), "timeout": timeout}]}));
        if *event == "PreToolUse" {
            let ask = format!("{} {ASK_FLAG}", command(exe));
            groups.push(json!({"matcher": ASK_MATCHER, "hooks": [{"type": "command", "command": ask, "timeout": PERMISSION_TIMEOUT}]}));
        }
    }
    if root == before {
        return Ok(false);
    }
    write_settings(settings, &root)?;
    Ok(true)
}

/// Takes Codync's hooks out again. True when the file changed.
pub fn uninstall(settings: &Path) -> Result<bool> {
    let Some(mut root) = read_settings(settings)? else { return Ok(false) };
    let before = root.clone();
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else { return Ok(false) };
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        groups.retain(|g| !group_is_ours(g));
    }
    hooks.retain(|_, v| v.as_array().is_none_or(|groups| !groups.is_empty()));
    if hooks.is_empty()
        && let Some(root) = root.as_object_mut()
    {
        root.remove("hooks");
    }
    if root == before {
        return Ok(false);
    }
    write_settings(settings, &root)?;
    Ok(true)
}

/// Whether Codync's hook is set for every event it listens to.
pub fn installed(settings: &Path) -> Result<bool> {
    let Some(root) = read_settings(settings)? else { return Ok(false) };
    let ask = root["hooks"]["PreToolUse"].as_array().is_some_and(|g| g.iter().any(is_ask_group));
    Ok(ask && EVENTS.iter().all(|event| root["hooks"][event].as_array().is_some_and(|g| g.iter().any(group_is_ours))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn settings(contents: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-hooks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        if let Some(text) = contents {
            std::fs::write(&file, text).unwrap();
        }
        file
    }

    fn read(file: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
    }

    const EXE: &str = "/opt/codync/codync-host";

    #[test]
    fn install_adds_every_event_and_keeps_the_persons_hooks() {
        let mine = json!({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "lint"}]}]}, "theme": "dark"});
        let file = settings(Some(&mine.to_string()));
        assert!(install(&file, Path::new(EXE)).unwrap());
        let v = read(&file);
        assert_eq!(v["theme"], "dark");
        assert_eq!(
            v["hooks"]["PreToolUse"].as_array().unwrap().len(),
            3,
            "theirs stays, ours and the asking one are added"
        );
        let ask = &v["hooks"]["PreToolUse"][2];
        assert_eq!(ask["matcher"], ASK_MATCHER);
        assert!(ask["hooks"][0]["command"].as_str().unwrap().ends_with(" hook --ask"));
        assert_eq!(ask["hooks"][0]["timeout"], PERMISSION_TIMEOUT);
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "lint");
        assert_eq!(v["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"], PERMISSION_TIMEOUT);
        assert!(installed(&file).unwrap());
        assert!(!install(&file, Path::new(EXE)).unwrap(), "a second install changes nothing");
    }

    #[test]
    fn a_moved_host_replaces_its_old_command() {
        let file = settings(None);
        install(&file, Path::new("/old/codync-host")).unwrap();
        assert!(install(&file, Path::new("/new/codync-host")).unwrap());
        let groups = read(&file)["hooks"]["Stop"].clone();
        assert_eq!(groups.as_array().unwrap().len(), 1);
        assert!(groups[0]["hooks"][0]["command"].as_str().unwrap().contains("/new/"));
    }

    #[test]
    fn uninstall_leaves_only_the_persons_hooks() {
        let mine = json!({"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "lint"}]}]}});
        let file = settings(Some(&mine.to_string()));
        install(&file, Path::new(EXE)).unwrap();
        assert!(uninstall(&file).unwrap());
        assert_eq!(read(&file), mine);
        assert!(!installed(&file).unwrap());
        assert!(!uninstall(&file).unwrap());

        let empty = settings(None);
        install(&empty, Path::new(EXE)).unwrap();
        uninstall(&empty).unwrap();
        assert_eq!(read(&empty), json!({}), "no empty `hooks` left behind");
    }

    #[test]
    fn settings_that_are_not_ours_to_edit_are_refused() {
        let file = settings(Some(r#"{"hooks": []}"#));
        assert!(install(&file, Path::new(EXE)).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), r#"{"hooks": []}"#);
    }
}
