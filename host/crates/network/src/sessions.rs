//! Which session of a coding agent an MCP server serves, and what the agent itself says about it.
//!
//! An agent starts its MCP servers as children, so a server finds its session the way that agent
//! tells it: Claude Code, Copilot CLI and Grok in the environment, Codex and Antigravity with each tool
//! call (`_meta`). The kind comes from the client's own name, never from the environment alone: an
//! agent started from a Claude Code shell inherits that session's variables.
//! ([agent sessions](https://github.com/leepokai/Codync/blob/dev/docs/plans/agent-sessions.md))

use crate::reach::{ClaudeSession, claude_dir, claude_sessions};
use crate::wire::AgentKind;
use serde_json::Value;
use std::path::Path;

/// A title as long as a listing shows it.
const TITLE_MAX: usize = 80;

impl AgentKind {
    /// The kind of agent an MCP client is, by the name it gives in `initialize` (`clientInfo.name`).
    /// Only names seen from the agents themselves; any other client is `other`.
    pub fn of_mcp_client(name: &str) -> Self {
        match name {
            "claude-code" => Self::ClaudeCode,
            "codex-mcp-client" => Self::Codex,
            "Cursor" => Self::Cursor,
            "copilot-cli" => Self::Copilot,
            "antigravity-client" => Self::Antigravity,
            "pi" => Self::Pi,
            "opencode" => Self::Opencode,
            // Grok names each server's client after it: `grok-shell-<server>`.
            name if name.starts_with("grok-shell-") => Self::Grok,
            _ => Self::Other,
        }
    }
}

/// The session an MCP server of `kind` serves: from `env` (the server's environment) or a tool
/// call's `meta`. `None` when the agent doesn't say.
pub fn session_id(kind: AgentKind, env: impl Fn(&str) -> Option<String>, meta: &Value) -> Option<String> {
    let text = |key: &str| meta.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
    match kind {
        // The session Claude Code is in now: after `/clear` the same process (and this server) is
        // in a new one, which its registry names by the inbox socket it gave this server.
        AgentKind::ClaudeCode => {
            let socket = env("CLAUDE_CODE_MESSAGING_SOCKET");
            let now = socket.and_then(|s| claude_dir().ok().and_then(|dir| claude_session_at(&dir, &s)));
            now.or_else(|| env("CLAUDE_CODE_SESSION_ID"))
        }
        AgentKind::Copilot => env("COPILOT_AGENT_SESSION_ID"),
        AgentKind::Grok => env("GROK_SESSION_ID"),
        AgentKind::Codex => text("sessionId").or_else(|| text("threadId")),
        AgentKind::Antigravity => {
            text("antigravity.google/root_conversation_id").or_else(|| text("antigravity.google/conversation_id"))
        }
        AgentKind::Opencode => text("ai.opencode/sessionID"),
        _ => None,
    }
    .filter(|id| !id.is_empty())
}

/// The session whose inbox is `socket`, newest first.
fn claude_session_at(dir: &Path, socket: &str) -> Option<String> {
    claude_sessions(dir)
        .into_iter()
        .filter(|s| s.messaging_socket_path.as_deref() == Some(socket))
        .max_by_key(|s| s.updated_at)
        .and_then(|s| s.session_id)
}

/// What a session's agent says about it, where it says it in a file of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// Its conversation's title, cut to a listing's width.
    pub title: Option<String>,
    /// Working on a turn (`Some(true)`) or waiting (`Some(false)`); `None` when the agent doesn't say.
    pub busy: Option<bool>,
}

/// Reads what `kind`'s own files say about session `id`.
pub fn info(kind: AgentKind, id: &str) -> Info {
    match kind {
        AgentKind::ClaudeCode => claude_dir().map(|dir| claude_info(&dir, id)).unwrap_or_default(),
        AgentKind::Codex => Info { title: codex_dir().and_then(|dir| codex_title(&dir, id)), busy: None },
        _ => Info::default(),
    }
}

/// Whether session `id` is working on a turn now, where its agent says so. Blocking: may run `ps`.
pub fn busy(kind: AgentKind, id: &str) -> Option<bool> {
    match kind {
        AgentKind::ClaudeCode => claude_dir().ok().and_then(|dir| claude_busy(&dir, id)),
        _ => None,
    }
}

/// The newest entry Claude Code's registry has for session `id`.
fn claude_newest(dir: &Path, id: &str) -> Option<ClaudeSession> {
    claude_sessions(dir).into_iter().filter(|s| s.session_id.as_deref() == Some(id)).max_by_key(|s| s.updated_at)
}

fn claude_info(dir: &Path, id: &str) -> Info {
    let Some(s) = claude_newest(dir, id) else { return Info::default() };
    Info { title: s.name.as_deref().and_then(cut), busy: s.status.as_deref().map(|s| s == "busy") }
}

/// What the registry says of session `id`, unless its process is gone: Claude Code killed or
/// crashed leaves its entry behind.
fn claude_busy(dir: &Path, id: &str) -> Option<bool> {
    let s = claude_newest(dir, id).filter(|s| s.pid.is_none_or(alive))?;
    s.status.as_deref().map(|s| s == "busy")
}

/// Whether process `pid` still runs; true where this can't tell.
fn alive(pid: u32) -> bool {
    if cfg!(windows) {
        return true;
    }
    std::process::Command::new("ps").args(["-p", &pid.to_string()]).output().map_or(true, |out| out.status.success())
}

/// Codex's own folder: `$CODEX_HOME`, else `~/.codex`.
fn codex_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|d| !d.is_empty())
        .map(Into::into)
        .or_else(|| Some(std::env::home_dir()?.join(".codex")))
}

/// A thread's name, the newest line `session_index.jsonl` has for it.
// ponytail: reads the whole index on every look; keep an offset if it grows large.
fn codex_title(dir: &Path, thread: &str) -> Option<String> {
    let index = std::fs::read_to_string(dir.join("session_index.jsonl")).ok()?;
    index
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["id"] == thread)
        .and_then(|v| v["thread_name"].as_str().and_then(cut))
}

/// One line of at most `TITLE_MAX` bytes, cut at a character.
fn cut(title: &str) -> Option<String> {
    let line = title.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(line[..line.floor_char_boundary(TITLE_MAX)].to_owned()).filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_kind_comes_from_the_client_and_the_id_only_from_that_kind() {
        let env = |key: &str| match key {
            "CLAUDE_CODE_SESSION_ID" => Some("parent-claude".to_owned()),
            "COPILOT_AGENT_SESSION_ID" => Some("copilot-1".to_owned()),
            "GROK_SESSION_ID" => Some("grok-1".to_owned()),
            _ => None,
        };
        let meta = json!({"sessionId": "thread-1", "antigravity.google/conversation_id": "conv-1"});
        assert_eq!(AgentKind::of_mcp_client("copilot-cli"), AgentKind::Copilot);
        assert_eq!(session_id(AgentKind::Copilot, env, &meta).as_deref(), Some("copilot-1"));
        assert_eq!(session_id(AgentKind::Codex, env, &meta).as_deref(), Some("thread-1"));
        assert_eq!(AgentKind::of_mcp_client("grok-shell-codync"), AgentKind::Grok);
        assert_eq!(session_id(AgentKind::Grok, env, &meta).as_deref(), Some("grok-1"));
        assert_eq!(session_id(AgentKind::Antigravity, env, &meta).as_deref(), Some("conv-1"));
        // pi started from a Claude Code shell carries that session's id: it isn't pi's.
        assert_eq!(AgentKind::of_mcp_client("pi"), AgentKind::Pi);
        assert_eq!(session_id(AgentKind::Pi, env, &meta), None);
        assert_eq!(AgentKind::of_mcp_client("some-editor"), AgentKind::Other);
        assert_eq!(session_id(AgentKind::Other, env, &meta), None);
    }

    #[test]
    fn claude_code_says_the_session_its_inbox_is_in_now_with_its_title() {
        let dir = std::env::temp_dir().join(format!("codync-network-sessions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        let entry = |sid: &str, at: u64, name: &str, status: &str| json!({"sessionId": sid, "updatedAt": at, "messagingSocketPath": "/s.sock", "name": name, "status": status});
        std::fs::write(dir.join("sessions/1.json"), entry("before-clear", 1, "Old", "idle").to_string()).unwrap();
        std::fs::write(dir.join("sessions/2.json"), entry("after-clear", 2, "Fix   the\nlogin", "busy").to_string())
            .unwrap();
        assert_eq!(claude_session_at(&dir, "/s.sock").as_deref(), Some("after-clear"));
        assert_eq!(claude_info(&dir, "after-clear"), Info { title: Some("Fix the login".into()), busy: Some(true) });
        assert_eq!(claude_info(&dir, "gone"), Info::default());
        let me = std::process::id();
        std::fs::write(dir.join(format!("sessions/{me}.json")), entry("running", 3, "Here", "idle").to_string())
            .unwrap();
        std::fs::write(dir.join("sessions/999999999.json"), entry("killed", 4, "Gone", "busy").to_string()).unwrap();
        assert_eq!(claude_busy(&dir, "running"), Some(false));
        // Windows doesn't look at the process (`alive` says yes there).
        #[cfg(unix)]
        assert_eq!(claude_busy(&dir, "killed"), None, "a killed Claude Code leaves its entry behind");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_titles_come_from_its_index_and_get_cut() {
        let dir = std::env::temp_dir().join(format!("codync-network-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lines = [json!({"id": "t1", "thread_name": "First"}), json!({"id": "t1", "thread_name": "審".repeat(40)})];
        std::fs::write(dir.join("session_index.jsonl"), lines.map(|l| l.to_string()).join("\n")).unwrap();
        let title = codex_title(&dir, "t1").unwrap();
        assert!(title.len() <= TITLE_MAX && title.starts_with('審'));
        assert_eq!(codex_title(&dir, "t2"), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
