//! What an account's slot shares with the user's own setup. A slot separates more than the login: skills, rules,
//! MCP servers, settings and sessions live in the same folder, and an agent started on an empty one loses them
//! without a word. Folders are linked, so both sides see one copy; files the agent rewrites by rename (which
//! replaces a link) are copied before each start.

use crate::Error;
use crate::identity::read_json;
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) enum Shared {
    /// Claude Code keeps runtime state of its own beside the setup, so only what's named here is shared.
    Claude,
    /// Everything in the agent's home is linked except `private`, which holds the login (Codex). `sessions` is
    /// linked even before the user has it.
    AllBut { private: &'static [&'static str], sessions: &'static str },
    /// Google Antigravity: everything but its ACP server's folder, where only the conversations are shared.
    Antigravity,
    /// The slot holds only the login: the agent's setup lives elsewhere and every account reads it (`OpenCode`).
    Nothing,
}

type Result<T = ()> = std::result::Result<T, Error>;

/// An I/O failure on `path`, with what was being done.
fn failed(doing: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.to_path_buf();
    move |source| Error::Slot { doing, path, source }
}

/// The ACP server's folder in Antigravity's home, and what of it every account shares.
const ANTIGRAVITY_SERVER: &str = "antigravity-acp";
const ANTIGRAVITY_SHARED: [&str; 2] = ["conversations", "brain"];

pub(crate) fn make(folder: &Path) -> Result {
    fs::create_dir_all(folder).map_err(failed("creating", folder))
}

/// Claude Code's setup folders: linked when the user has them.
const CLAUDE_SETUP: [&str; 6] = ["skills", "agents", "commands", "plugins", "rules", "output-styles"];
/// The folders a session is stored in: always linked, so a bot moved to another account picks its conversation up.
const CLAUDE_SESSIONS: [&str; 8] =
    ["projects", "file-history", "shell-snapshots", "session-env", "todos", "plans", "tasks", "paste-cache"];
const CLAUDE_FILES: [&str; 2] = ["CLAUDE.md", "keybindings.json"];
/// Settings that choose a login: an account keeps its own.
const CLAUDE_LOGIN_SETTINGS: [&str; 3] = ["apiKeyHelper", "forceLoginMethod", "forceLoginOrgUUID"];
pub(crate) const CLAUDE_LOGIN_ENV: [&str; 3] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN"];

/// Makes `slot` share `home` (the agent's folder in `user_home`).
pub(crate) fn share(shared: Shared, user_home: &Path, home: &Path, slot: &Path) -> Result {
    make(slot)?;
    match shared {
        Shared::Nothing => Ok(()),
        Shared::Antigravity => {
            link_all_but(home, slot, &[ANTIGRAVITY_SERVER])?;
            let (server, own) = (home.join(ANTIGRAVITY_SERVER), slot.join(ANTIGRAVITY_SERVER));
            make(&own)?;
            for name in ANTIGRAVITY_SHARED {
                link_shared(&server.join(name), &own.join(name))?;
            }
            Ok(())
        }
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
        Shared::AllBut { private, sessions } => {
            link_shared(&home.join(sessions), &slot.join(sessions))?;
            link_all_but(home, slot, private)
        }
    }
}

/// Links everything in `home` into `slot`, except `private`.
fn link_all_but(home: &Path, slot: &Path, private: &[&str]) -> Result {
    let Ok(entries) = fs::read_dir(home) else { return Ok(()) };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !private.iter().any(|p| name == *p) {
            link(&entry.path(), &slot.join(&name))?;
        }
    }
    Ok(())
}

/// Links a folder both sides must share. One the user doesn't have yet is made first: left to the agent, the
/// slot's would be a folder of its own that is never shared, and goes with the account.
fn link_shared(source: &Path, target: &Path) -> Result {
    make(source)?;
    link(source, target)
}

/// Links `target` to `source`. What the agent or the user put at `target` meanwhile stays: an empty folder is the
/// only thing replaced.
// ponytail: a file the agent rewrote by rename (Codex's `config.toml`) is its own copy from then on. Upgrade:
// merge the slot's changes back into the user's file, then link again.
fn link(source: &Path, target: &Path) -> Result {
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
        Err(e) => return Err(failed("reading", target)(e)),
    }
    make_link(source, target, meta.is_dir()).map_err(failed("linking", target))
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
fn copy(source: &Path, target: &Path) -> Result {
    match fs::read(source) {
        Ok(bytes) => write_if_changed(target, &bytes),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(failed("reading", source)(e)),
    }
}

fn write_if_changed(target: &Path, bytes: &[u8]) -> Result {
    if fs::read(target).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    fs::write(target, bytes).map_err(failed("writing", target))
}

/// JSON as the agents write it. A `Value` always serializes.
fn pretty(value: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(value).unwrap_or_default()
}

fn read_object(path: &Path) -> Option<Value> {
    read_json(path).filter(Value::is_object)
}

/// The user's settings, without the ones that would sign the account in as someone else.
fn claude_settings(source: &Path, target: &Path) -> Result {
    let Some(mut settings) = read_object(source) else { return Ok(()) };
    without_login(&mut settings);
    write_if_changed(target, &pretty(&settings))
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
fn claude_mcp_servers(source: &Path, target: &Path) -> Result {
    let Some(servers) = read_object(source).and_then(|mut v| v.get_mut("mcpServers").map(Value::take)) else {
        return Ok(());
    };
    let mut state = read_object(target).unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    if state["mcpServers"] == servers {
        return Ok(());
    }
    state["mcpServers"] = servers;
    write_if_changed(target, &pretty(&state))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn scratch() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("codync-account-slot-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// Whether a file put in `mine` shows up in `theirs`: the two are one folder.
    fn shares(mine: &Path, theirs: &Path) -> bool {
        let probe = mine.join(".probe");
        fs::write(&probe, "x").expect("a file in the folder");
        let seen = theirs.join(".probe").exists();
        fs::remove_file(probe).expect("removed");
        seen
    }

    fn home() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let root = scratch();
        let (home, slot) = (root.join(".claude"), root.join("slot"));
        fs::create_dir_all(home.join("skills/review")).expect("skills");
        fs::create_dir_all(home.join("projects")).expect("projects");
        (root, home, slot)
    }

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

    #[test]
    fn session_folders_the_user_lacks_are_made_so_both_sides_share_them() {
        let root = scratch();
        let (home, slot) = (root.join(".claude"), root.join("slot"));
        share(Shared::Claude, &root, &home, &slot).expect("share");
        assert!(shares(&slot.join("projects"), &home.join("projects")));
        assert!(!home.join("skills").exists(), "setup the user doesn't have isn't made up");
        let (home, slot) = (root.join(".codex"), root.join("codex-slot"));
        share(Shared::AllBut { private: &["auth.json"], sessions: "sessions" }, &root, &home, &slot).expect("share");
        assert!(shares(&slot.join("sessions"), &home.join("sessions")));
    }

    #[test]
    fn sharing_again_keeps_what_the_slot_made_its_own() {
        let (root, home, slot) = home();
        fs::create_dir_all(slot.join("skills/own")).expect("the slot's own skills");
        fs::create_dir_all(slot.join("projects")).expect("an empty folder the agent made");
        share(Shared::Claude, &root, &home, &slot).expect("share");
        share(Shared::Claude, &root, &home, &slot).expect("share again");
        assert!(slot.join("skills/own").is_dir(), "its own skills stay");
        assert!(!home.join("skills/own").exists());
        fs::write(slot.join("projects/s.jsonl"), "{}").expect("a session");
        assert!(home.join("projects/s.jsonl").exists(), "an empty folder becomes the shared one");
    }

    #[test]
    fn an_antigravity_slot_shares_conversations_but_keeps_its_servers_sign_in() {
        let root = scratch();
        let (home, slot) = (root.join(".gemini"), root.join("slot"));
        fs::create_dir_all(home.join("skills")).expect("skills");
        fs::create_dir_all(home.join("antigravity-acp/conversations")).expect("conversations");
        fs::write(home.join("antigravity-acp/acp_token.json"), "secret").expect("login");
        fs::write(home.join("antigravity-acp/settings.json"), "{}").expect("which sign-in");
        share(Shared::Antigravity, &root, &home, &slot).expect("share");
        assert!(shares(&slot.join("skills"), &home.join("skills")));
        let server = slot.join("antigravity-acp");
        assert!(!shares(&server, &home.join("antigravity-acp")), "its own folder");
        let theirs = home.join("antigravity-acp");
        assert!(shares(&server.join("conversations"), &theirs.join("conversations")));
        assert!(shares(&server.join("brain"), &theirs.join("brain")));
        assert!(!server.join("acp_token.json").exists() && !server.join("settings.json").exists());
    }

    #[test]
    fn a_slot_that_holds_only_the_login_takes_nothing_from_the_users_home() {
        let root = scratch();
        fs::create_dir_all(root.join("skills")).expect("the user's own");
        share(Shared::Nothing, &root, &root, &root.join("slot")).expect("share");
        assert_eq!(fs::read_dir(root.join("slot")).expect("the slot").count(), 0);
    }

    #[test]
    fn a_slot_that_shares_all_but_the_login_links_everything_else() {
        let root = scratch();
        let (home, slot) = (root.join(".codex"), root.join("slot"));
        fs::create_dir_all(home.join("sessions")).expect("sessions");
        fs::write(home.join("config.toml"), "model = \"gpt\"").expect("config");
        fs::write(home.join("auth.json"), "secret").expect("login");
        share(Shared::AllBut { private: &["auth.json"], sessions: "sessions" }, &root, &home, &slot).expect("share");
        assert!(shares(&slot.join("sessions"), &home.join("sessions")));
        assert_eq!(fs::read_to_string(slot.join("config.toml")).expect("the user's config"), "model = \"gpt\"");
        assert!(!slot.join("auth.json").exists());
    }
}
