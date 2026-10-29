//! codex-acp leaves out a requested MCP server whose name Codex's own config already uses ("Codex uses its own
//! entry, or no server if that entry is disabled"), and says so only in a notice before the first turn. A connector
//! named like one of the user's Codex servers (`notion`) then never reaches the bot (#217), so it gets a free name.

use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

/// Codync's servers that got another name for this agent: requested name -> the name the agent knows.
pub(super) type Renamed = BTreeMap<String, String>;

/// What the agent calls Codync's server `name`.
pub(super) fn name_of<'a>(renamed: &'a Renamed, name: &'a str) -> &'a str {
    renamed.get(name).map_or(name, String::as_str)
}

/// Renames each of `servers` whose name a Codex config for `cwd` already defines. `codex_home` is the agent's
/// own `CODEX_HOME` when it has one.
pub(super) async fn avoid_clashes(servers: &mut Value, cwd: &str, codex_home: Option<PathBuf>) -> Renamed {
    let files = config_files(Path::new(cwd), codex_home);
    match tokio::task::spawn_blocking(move || taken(&files)).await {
        Ok(taken) => rename(servers, &taken),
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "couldn't read Codex's MCP server names");
            Renamed::new()
        }
    }
}

/// A clashing server becomes `codync-<name>`, or `codync-<name>-2`… when that is taken too: by Codex's config
/// or by another requested server.
fn rename(servers: &mut Value, taken: &HashSet<String>) -> Renamed {
    let list = servers.as_array_mut().map(Vec::as_mut_slice).unwrap_or_default();
    let mut used: HashSet<String> = taken.clone();
    used.extend(list.iter().filter_map(|s| s["name"].as_str().map(str::to_owned)));
    let mut renamed = Renamed::new();
    for server in list {
        let Some(name) = server["name"].as_str().filter(|name| taken.contains(*name)).map(str::to_owned) else {
            continue;
        };
        let free = (1..=used.len() + 1)
            .map(|n| if n == 1 { format!("codync-{name}") } else { format!("codync-{name}-{n}") })
            .find(|candidate| !used.contains(candidate))
            .expect("one of more names than are used is free");
        used.insert(free.clone());
        server["name"] = free.clone().into();
        renamed.insert(name, free);
    }
    renamed
}

/// The files Codex layers its config from: the user's and the system's, and `.codex/config.toml` in the folders
/// from `cwd` up (projects).
// ponytail: `[profiles.<name>.mcp_servers]` isn't read; add it if a profile's server ever clashes.
fn config_files(cwd: &Path, codex_home: Option<PathBuf>) -> Vec<PathBuf> {
    let user = codex_home
        .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
        .or_else(|| dirs::home_dir().map(|h| h.join(".codex")));
    let mut files: Vec<PathBuf> = user.into_iter().map(|dir| dir.join("config.toml")).collect();
    if cfg!(unix) {
        files.push("/etc/codex/config.toml".into());
    }
    files.extend(cwd.ancestors().map(|dir| dir.join(".codex/config.toml")));
    files
}

/// Every `[mcp_servers.<name>]` in `files`, disabled ones too: codex-acp skips those as well.
fn taken(files: &[PathBuf]) -> HashSet<String> {
    let mut names = HashSet::new();
    for text in files.iter().filter_map(|file| std::fs::read_to_string(file).ok()) {
        let Ok(doc) = text.parse::<DocumentMut>() else { continue };
        if let Some(servers) = doc.get("mcp_servers").and_then(Item::as_table_like) {
            names.extend(servers.iter().map(|(name, _)| name.to_owned()));
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn servers_codex_already_names_get_a_free_name() {
        let dir = std::env::temp_dir().join(format!("codync-codex-names-{}", uuid::Uuid::new_v4()));
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
        let renamed = rename(&mut servers, &taken(&files));
        assert_eq!(servers, json!([{"name": "codync-notion"}, {"name": "codync-memory"}, {"name": "chat"}]));
        assert_eq!(name_of(&renamed, "memory"), "codync-memory");
        assert_eq!(name_of(&renamed, "chat"), "chat");
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_replacement_name_that_is_taken_too_moves_on() {
        let taken: HashSet<String> = ["notion", "codync-notion"].map(str::to_owned).into();
        let mut servers = json!([{"name": "notion"}, {"name": "codync-notion-2"}]);
        rename(&mut servers, &taken);
        assert_eq!(servers, json!([{"name": "codync-notion-3"}, {"name": "codync-notion-2"}]));
    }
}
