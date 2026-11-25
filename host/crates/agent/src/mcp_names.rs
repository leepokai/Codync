//! Agents that leave out a requested MCP server whose name their own config already uses. codex-acp does it for
//! Codex's config ("Codex uses its own entry, or no server if that entry is disabled") and says so only in a notice
//! before the first turn; pi does it for its `mcp.json`. A server named like one of the user's own (`notion`) then
//! never reaches the session, so [`avoid_clashes`] gives it a free name, and [`name_of`] says what the agent calls
//! it in its tool calls.

use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

/// The app's servers that got another name for this agent: requested name -> the name the agent knows.
pub type Renamed = BTreeMap<String, String>;

/// An agent whose own MCP config hides a requested server of the same name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    /// codex-acp: Codex's `config.toml` files.
    Codex,
    /// pi (through pi-acp or [`crate::pi`]): its `mcp.json` files.
    Pi,
}

impl Agent {
    /// The variable that moves the agent's own config folder, which a login profile sets.
    pub fn home_var(self) -> &'static str {
        match self {
            Self::Codex => "CODEX_HOME",
            Self::Pi => "PI_CODING_AGENT_DIR",
        }
    }
}

/// What the agent calls the app's server `name`.
pub fn name_of<'a>(renamed: &'a Renamed, name: &'a str) -> &'a str {
    renamed.get(name).map_or(name, String::as_str)
}

/// Renames each of `servers` (ACP `mcpServers`) whose name `agent`'s config for `cwd` already has, to
/// `<prefix>-<name>`. `home` is the agent's own config folder ([`Agent::home_var`]) when the app sets one.
pub async fn avoid_clashes(
    agent: Agent,
    servers: &mut Value,
    cwd: &str,
    home: Option<PathBuf>,
    prefix: &str,
) -> Renamed {
    let files = config_files(agent, Path::new(cwd), home);
    let taken = match agent {
        Agent::Codex => codex_taken,
        Agent::Pi => pi_taken,
    };
    match tokio::task::spawn_blocking(move || taken(&files)).await {
        Ok(taken) => rename(servers, &taken, prefix),
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "couldn't read the agent's MCP server names");
            Renamed::new()
        }
    }
}

/// A clashing server becomes `<prefix>-<name>`, or `<prefix>-<name>-2`… when that is taken too: by the agent's
/// config or by another requested server. Names that differ only in `-` and `_` count as one, as they do for pi.
fn rename(servers: &mut Value, taken: &HashSet<String>, prefix: &str) -> Renamed {
    let fold = |name: &str| name.replace('-', "_");
    let taken: HashSet<String> = taken.iter().map(|name| fold(name)).collect();
    let list = servers.as_array_mut().map(Vec::as_mut_slice).unwrap_or_default();
    let mut used = taken.clone();
    used.extend(list.iter().filter_map(|s| s["name"].as_str().map(fold)));
    let mut renamed = Renamed::new();
    for server in list {
        let Some(name) = server["name"].as_str().filter(|name| taken.contains(&fold(name))).map(str::to_owned) else {
            continue;
        };
        let free = (1..=used.len() + 1)
            .map(|n| if n == 1 { format!("{prefix}-{name}") } else { format!("{prefix}-{name}-{n}") })
            .find(|candidate| !used.contains(&fold(candidate)))
            .expect("one of more names than are used is free");
        used.insert(fold(&free));
        server["name"] = free.clone().into();
        renamed.insert(name, free);
    }
    renamed
}

/// The files the agent layers its MCP config from.
fn config_files(agent: Agent, cwd: &Path, home: Option<PathBuf>) -> Vec<PathBuf> {
    match agent {
        // The user's and the system's, and `.codex/config.toml` in the folders from `cwd` up (projects).
        // ponytail: `[profiles.<name>.mcp_servers]` isn't read; add it if a profile's server ever clashes.
        Agent::Codex => {
            let user = home
                .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
                .or_else(|| dirs::home_dir().map(|h| h.join(".codex")));
            let mut files: Vec<PathBuf> = user.into_iter().map(|dir| dir.join("config.toml")).collect();
            if cfg!(unix) {
                files.push("/etc/codex/config.toml".into());
            }
            files.extend(cwd.ancestors().map(|dir| dir.join(".codex/config.toml")));
            files
        }
        // `mcp.json` in pi's agent folder and the project's `.pi`.
        Agent::Pi => {
            let agent_dir = home
                .or_else(|| std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from))
                .or_else(|| dirs::home_dir().map(|h| h.join(".pi/agent")));
            agent_dir.into_iter().chain([cwd.join(".pi")]).map(|dir| dir.join("mcp.json")).collect()
        }
    }
}

/// Every `[mcp_servers.<name>]` in `files`, disabled ones too: codex-acp skips those as well.
fn codex_taken(files: &[PathBuf]) -> HashSet<String> {
    let mut names = HashSet::new();
    for text in files.iter().filter_map(|file| std::fs::read_to_string(file).ok()) {
        let Ok(doc) = text.parse::<DocumentMut>() else { continue };
        if let Some(servers) = doc.get("mcp_servers").and_then(Item::as_table_like) {
            names.extend(servers.iter().map(|(name, _)| name.to_owned()));
        }
    }
    names
}

/// Every server name in the `mcpServers` of `files`, disabled entries too.
fn pi_taken(files: &[PathBuf]) -> HashSet<String> {
    files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .filter_map(|text| serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')).ok())
        .filter_map(|config| config["mcpServers"].as_object().map(|s| s.keys().cloned().collect::<Vec<_>>()))
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codync-agent-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn servers_codex_already_names_get_a_free_name() {
        let dir = temp("codex-names");
        let project = dir.join("work/app");
        std::fs::create_dir_all(dir.join("work/.codex")).expect("dir");
        std::fs::create_dir_all(&project).expect("dir");
        std::fs::write(
            dir.join("work/.codex/config.toml"),
            "[mcp_servers.notion]\nurl = \"https://mcp.notion.com/mcp\"\n\n[mcp_servers.memory]\ncommand = \"npx\"\nenabled = false\n",
        )
        .expect("config");
        let files: Vec<PathBuf> = project.ancestors().map(|d| d.join(".codex/config.toml")).collect();
        let mut servers = json!([{"name": "notion"}, {"name": "memory"}, {"name": "chat"}]);
        let renamed = rename(&mut servers, &codex_taken(&files), "app");
        assert_eq!(servers, json!([{"name": "app-notion"}, {"name": "app-memory"}, {"name": "chat"}]));
        assert_eq!(name_of(&renamed, "memory"), "app-memory");
        assert_eq!(name_of(&renamed, "chat"), "chat");
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_replacement_name_that_is_taken_too_moves_on() {
        let taken: HashSet<String> = ["notion", "app-notion"].map(str::to_owned).into();
        let mut servers = json!([{"name": "notion"}, {"name": "app-notion-2"}]);
        rename(&mut servers, &taken, "app");
        assert_eq!(servers, json!([{"name": "app-notion-3"}, {"name": "app-notion-2"}]));
    }

    #[tokio::test]
    async fn servers_pi_already_names_get_a_free_name() {
        let dir = temp("pi-names");
        std::fs::create_dir_all(dir.join("agent")).expect("dir");
        std::fs::create_dir_all(dir.join("work/.pi")).expect("dir");
        std::fs::write(dir.join("agent/mcp.json"), "\u{feff}{\"mcpServers\": {\"chat\": {}}}").expect("config");
        std::fs::write(dir.join("work/.pi/mcp.json"), r#"{"mcpServers": {"my_memory": {}}}"#).expect("config");
        let mut servers = json!([{"name": "chat"}, {"name": "my-memory"}, {"name": "manage"}]);
        let cwd = dir.join("work");
        avoid_clashes(Agent::Pi, &mut servers, &cwd.to_string_lossy(), Some(dir.join("agent")), "app").await;
        assert_eq!(servers, json!([{"name": "app-chat"}, {"name": "app-my-memory"}, {"name": "manage"}]));
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
