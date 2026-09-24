//! Known coding-agent backends and the ACP command that drives each one.

use serde_json::{Value, json};

pub struct Backend {
    pub id: &'static str,
    pub name: &'static str,
    /// ACP stdio command.
    pub command: &'static str,
    /// CLI that must be installed and logged in for the adapter to work.
    pub requires: &'static str,
    pub install_hint: &'static str,
}

pub const BACKENDS: &[Backend] = &[
    Backend {
        id: "claude",
        name: "Claude Code",
        command: "npx -y @agentclientprotocol/claude-agent-acp",
        requires: "claude",
        install_hint: "Install Claude Code and run `claude` once to log in. Node.js (npx) is required for the ACP adapter.",
    },
    Backend {
        id: "codex",
        name: "Codex",
        command: "npx -y @agentclientprotocol/codex-acp",
        requires: "codex",
        install_hint: "Install Codex CLI and run `codex login`. Node.js (npx) is required for the ACP adapter.",
    },
    Backend {
        id: "opencode",
        name: "OpenCode",
        command: "opencode acp",
        requires: "opencode",
        install_hint: "Install OpenCode: curl -fsSL https://opencode.ai/install | bash",
    },
    Backend {
        id: "grok",
        name: "Grok Build",
        command: "grok agent stdio",
        requires: "grok",
        install_hint: "Install Grok Build: curl -fsSL https://x.ai/cli/install.sh | bash",
    },
    Backend {
        id: "gemini",
        name: "Gemini CLI",
        command: "gemini --experimental-acp",
        requires: "gemini",
        install_hint: "Install Gemini CLI: npm install -g @google/gemini-cli",
    },
];

pub fn command(id: &str) -> Option<&'static str> {
    BACKENDS.iter().find(|b| b.id == id).map(|b| b.command)
}

pub fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

pub fn list() -> Vec<Value> {
    BACKENDS
        .iter()
        .map(|b| {
            let needs_npx = b.command.starts_with("npx ");
            let available = on_path(b.requires) && (!needs_npx || on_path("npx"));
            json!({
                "id": b.id,
                "name": b.name,
                "available": available,
                "command": b.command,
                "installHint": b.install_hint,
            })
        })
        .collect()
}
