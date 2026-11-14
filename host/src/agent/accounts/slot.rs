//! What an account's slot shares with the user's own setup. A slot separates more than the login: skills, rules,
//! MCP servers, settings and sessions live in the same folder, and an agent started on an empty one loses them
//! without a word. Folders are linked, so both sides see one copy; files the agent rewrites by rename (which
//! replaces a link) are copied before each start.

use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

#[derive(Clone, Copy)]
pub(super) enum Shared {
    /// Claude Code keeps runtime state of its own beside the setup, so only what's named here is shared.
    Claude,
    /// Everything in the agent's home is linked except these, which hold the login (Codex).
    AllBut(&'static [&'static str]),
}

/// Claude Code's setup folders: linked when the user has them.
const CLAUDE_SETUP: [&str; 6] = ["skills", "agents", "commands", "plugins", "rules", "output-styles"];
/// The folders a session is stored in: always linked, so a bot moved to another account picks its conversation up.
const CLAUDE_SESSIONS: [&str; 8] =
    ["projects", "file-history", "shell-snapshots", "session-env", "todos", "plans", "tasks", "paste-cache"];
const CLAUDE_FILES: [&str; 2] = ["CLAUDE.md", "keybindings.json"];
/// Settings that choose a login: an account keeps its own.
const CLAUDE_LOGIN_SETTINGS: [&str; 3] = ["apiKeyHelper", "forceLoginMethod", "forceLoginOrgUUID"];
pub(super) const CLAUDE_LOGIN_ENV: [&str; 3] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN"];

/// Makes `slot` share `home` (the agent's folder in `user_home`).
pub(super) fn share(shared: Shared, user_home: &Path, home: &Path, slot: &Path) -> Result<()> {
    fs::create_dir_all(slot).with_context(|| format!("creating {}", slot.display()))?;
    match shared {
        Shared::Claude => {
            for name in CLAUDE_SETUP {
                link(&home.join(name), &slot.join(name))?;
            }
            for name in CLAUDE_SESSIONS {
                link_shared(&home.join(name), &slot.join(name))?;
            }
            for name in CLAUDE_FILES {
                copy(&home.join(name), &slot.join(name))?;
            }
            claude_settings(&home.join("settings.json"), &slot.join("settings.json"))?;
            // Without `CLAUDE_CONFIG_DIR` Claude Code keeps this file beside its folder, with it inside.
            claude_mcp_servers(&user_home.join(".claude.json"), &slot.join(".claude.json"))
        }
        Shared::AllBut(private) => {
            link_shared(&home.join("sessions"), &slot.join("sessions"))?;
            let Ok(entries) = fs::read_dir(home) else { return Ok(()) };
            for entry in entries.flatten() {
                let name = entry.file_name();
                if !private.iter().any(|p| name == *p) {
                    link(&entry.path(), &slot.join(&name))?;
                }
            }
            Ok(())
        }
    }
}

/// Links a folder both sides must share. One the user doesn't have yet is made first: left to the agent, the
/// slot's would be a folder of its own that is never shared, and goes with the account.
fn link_shared(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(source).with_context(|| format!("creating {}", source.display()))?;
    link(source, target)
}

/// Links `target` to `source`. What the agent or the user put at `target` meanwhile stays: an empty folder is the
/// only thing replaced.
// ponytail: a file the agent rewrote by rename (Codex's `config.toml`) is its own copy from then on. Upgrade:
// merge the slot's changes back into the user's file, then link again.
fn link(source: &Path, target: &Path) -> Result<()> {
    let Ok(meta) = fs::metadata(source) else { return Ok(()) };
    match fs::symlink_metadata(target) {
        Ok(existing) if existing.is_dir() && !existing.is_symlink() => {
            // Not empty: `remove_dir` fails and the folder stays the slot's own.
            if fs::remove_dir(target).is_err() {
                return Ok(());
            }
        }
        Ok(_) => return Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("reading {}", target.display())),
    }
    make_link(source, target, meta.is_dir()).with_context(|| format!("linking {}", target.display()))
}

#[cfg(unix)]
fn make_link(source: &Path, target: &Path, _dir: bool) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

/// A junction for a folder: unlike a symlink it needs no privilege. A file symlink does, so without it the file
/// is copied once (never a database: a copy of one is a second, stale database).
#[cfg(windows)]
fn make_link(source: &Path, target: &Path, dir: bool) -> std::io::Result<()> {
    if dir {
        let made =
            std::process::Command::new("cmd").args(["/d", "/c", "mklink", "/J"]).arg(target).arg(source).output()?;
        return if made.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(String::from_utf8_lossy(&made.stderr).into_owned()))
        };
    }
    if std::os::windows::fs::symlink_file(source, target).is_ok() || source.to_string_lossy().contains(".sqlite") {
        return Ok(());
    }
    fs::copy(source, target).map(drop)
}

/// Makes `target` the same file as `source`, when there is one.
fn copy(source: &Path, target: &Path) -> Result<()> {
    match fs::read(source) {
        Ok(bytes) => write_if_changed(target, &bytes),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("reading {}", source.display())),
    }
}

fn write_if_changed(target: &Path, bytes: &[u8]) -> Result<()> {
    if fs::read(target).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    fs::write(target, bytes).with_context(|| format!("writing {}", target.display()))
}

fn read_object(path: &Path) -> Option<Value> {
    super::read_json(path).filter(Value::is_object)
}

/// The user's settings, without the ones that would sign the account in as someone else.
fn claude_settings(source: &Path, target: &Path) -> Result<()> {
    let Some(mut settings) = read_object(source) else { return Ok(()) };
    without_login(&mut settings);
    write_if_changed(target, &serde_json::to_vec_pretty(&settings)?)
}

fn without_login(settings: &mut Value) {
    let Some(map) = settings.as_object_mut() else { return };
    map.retain(|key, _| !CLAUDE_LOGIN_SETTINGS.contains(&key.as_str()));
    if let Some(env) = map.get_mut("env").and_then(Value::as_object_mut) {
        env.retain(|key, _| !CLAUDE_LOGIN_ENV.contains(&key.as_str()));
    }
}

/// The user's own MCP servers are in a file that also says who is signed in, so only that one key is carried
/// over. The file is written only when the servers changed: Claude Code writes it too while it runs.
fn claude_mcp_servers(source: &Path, target: &Path) -> Result<()> {
    let Some(servers) = read_object(source).and_then(|mut v| v.get_mut("mcpServers").map(Value::take)) else {
        return Ok(());
    };
    let mut state = read_object(target).unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    if state["mcpServers"] == servers {
        return Ok(());
    }
    state["mcpServers"] = servers;
    write_if_changed(target, &serde_json::to_vec_pretty(&state)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-account-slot-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn home() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let root = scratch();
        let (home, slot) = (root.join(".claude"), root.join("slot"));
        fs::create_dir_all(home.join("skills/review")).expect("skills");
        fs::create_dir_all(home.join("projects")).expect("projects");
        (root, home, slot)
    }

    #[cfg(unix)]
    #[test]
    fn a_claude_slot_shares_setup_and_sessions_but_not_the_login() {
        let (root, home, slot) = home();
        fs::write(home.join("skills/review/SKILL.md"), "# Review").expect("skill");
        fs::write(home.join("CLAUDE.md"), "Be brief.").expect("rules");
        fs::write(home.join(".credentials.json"), "secret").expect("login");
        fs::write(
            home.join("settings.json"),
            json!({"model": "opus", "apiKeyHelper": "key.sh",
            "env": {"ANTHROPIC_API_KEY": "sk", "EDITOR": "vi"}})
            .to_string(),
        )
        .expect("settings");
        fs::write(
            root.join(".claude.json"),
            json!({"mcpServers": {"docs": {"command": "docs"}},
            "oauthAccount": {"emailAddress": "own@example.com"}})
            .to_string(),
        )
        .expect("state");
        fs::create_dir_all(&slot).expect("slot");
        fs::write(slot.join(".claude.json"), json!({"oauthAccount": {"emailAddress": "work@example.com"}}).to_string())
            .expect("slot state");

        share(Shared::Claude, &root, &home, &slot).expect("share");

        assert_eq!(fs::read_to_string(slot.join("skills/review/SKILL.md")).expect("linked"), "# Review");
        fs::write(slot.join("projects/session.jsonl"), "{}").expect("a session written on the account");
        assert!(home.join("projects/session.jsonl").exists(), "the user's own login finds it");
        assert_eq!(fs::read_to_string(slot.join("CLAUDE.md")).expect("copied"), "Be brief.");
        assert!(!slot.join(".credentials.json").exists());
        let settings = read_object(&slot.join("settings.json")).expect("settings");
        assert_eq!(settings, json!({"model": "opus", "env": {"EDITOR": "vi"}}));
        let state = read_object(&slot.join(".claude.json")).expect("state");
        assert_eq!(state["mcpServers"]["docs"]["command"], "docs");
        assert_eq!(state["oauthAccount"]["emailAddress"], "work@example.com", "the account stays signed in as itself");
    }

    #[cfg(unix)]
    #[test]
    fn session_folders_the_user_lacks_are_made_so_both_sides_share_them() {
        let root = scratch();
        let (home, slot) = (root.join(".claude"), root.join("slot"));
        share(Shared::Claude, &root, &home, &slot).expect("share");
        assert!(slot.join("projects").is_symlink() && home.join("projects").is_dir());
        assert!(!home.join("skills").exists(), "setup the user doesn't have isn't made up");
        let (home, slot) = (root.join(".codex"), root.join("codex-slot"));
        share(Shared::AllBut(&["auth.json"]), &root, &home, &slot).expect("share");
        assert!(slot.join("sessions").is_symlink() && home.join("sessions").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn sharing_again_keeps_what_the_slot_made_its_own() {
        let (root, home, slot) = home();
        fs::create_dir_all(slot.join("skills/own")).expect("the slot's own skills");
        fs::create_dir_all(slot.join("projects")).expect("an empty folder the agent made");
        share(Shared::Claude, &root, &home, &slot).expect("share");
        share(Shared::Claude, &root, &home, &slot).expect("share again");
        assert!(slot.join("skills/own").is_dir() && !slot.join("skills").is_symlink());
        assert!(slot.join("projects").is_symlink(), "an empty folder becomes the shared one");
    }

    #[cfg(unix)]
    #[test]
    fn a_slot_that_shares_all_but_the_login_links_everything_else() {
        let root = scratch();
        let (home, slot) = (root.join(".codex"), root.join("slot"));
        fs::create_dir_all(home.join("sessions")).expect("sessions");
        fs::write(home.join("config.toml"), "model = \"gpt\"").expect("config");
        fs::write(home.join("auth.json"), "secret").expect("login");
        share(Shared::AllBut(&["auth.json"]), &root, &home, &slot).expect("share");
        assert!(slot.join("sessions").is_symlink() && slot.join("config.toml").is_symlink());
        assert!(!slot.join("auth.json").exists());
    }
}
