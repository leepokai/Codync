//! What an agent already brings, read from the user's home. This never starts the agent,
//! and it does not look in a project: those lists belong to the agent, not to a bot.
//!
//! MCP environment variables, headers and tokens stay in the files. A connector's
//! detail is a command with credentials removed, or only a URL's origin.

mod connectors;
mod plugins;
mod rules;
mod skills;

use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ITEMS: usize = 200;
const MAX_DEPTH: usize = 3;
const NAME_CHARS: usize = 80;
const TEXT_CHARS: usize = 180;
const SKILL_BYTES: u64 = 8 * 1024;
/// Claude's `~/.claude.json` also stores every project's history, so a small cap
/// cuts the file in half and the JSON no longer parses.
const MCP_BYTES: u64 = 32 * 1024 * 1024;

pub(super) struct Named {
    pub name: String,
    pub description: String,
    pub detail: String,
}

/// `{ skills, connectors, rules, plugins }` for `backend`, from the user's home only.
/// Each entry is `{name, description}` except connectors, which use `{name, detail}`.
pub async fn list(backend: &str) -> Value {
    let backend = backend.to_owned();
    match tokio::task::spawn_blocking(move || scan(&backend)).await {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "listing what an agent brings");
            json!({"skills": [], "connectors": [], "rules": [], "plugins": []})
        }
    }
}

fn scan(backend: &str) -> Value {
    let home = dirs::home_dir().unwrap_or_default();
    let skills = skills::list(backend, &home);
    let connectors = connectors::list(backend, &home);
    let rules = rules::list(backend, &home);
    let plugins = plugins::list(backend, &home);
    json!({
        "skills": skills.iter().map(|s| json!({"name": s.name, "description": s.description})).collect::<Vec<_>>(),
        "connectors": connectors.iter().map(|c| json!({"name": c.name, "detail": c.detail})).collect::<Vec<_>>(),
        "rules": rules.iter().map(|s| json!({"name": s.name, "description": s.description})).collect::<Vec<_>>(),
        "plugins": plugins.iter().map(|s| json!({"name": s.name, "description": s.description})).collect::<Vec<_>>(),
    })
}

pub(super) fn homes(base: &Path, rels: &[&str]) -> Vec<PathBuf> {
    if base.as_os_str().is_empty() {
        return Vec::new();
    }
    rels.iter().map(|rel| base.join(rel)).collect()
}

pub(super) fn dedupe(items: &mut Vec<Named>) {
    let mut index = std::collections::HashMap::<String, usize>::new();
    let mut kept = Vec::new();
    for item in items.drain(..) {
        if let Some(at) = index.get(&item.name).copied() {
            kept[at] = item;
        } else {
            index.insert(item.name.clone(), kept.len());
            kept.push(item);
        }
    }
    *items = kept;
}

pub(super) fn clip(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max { collapsed } else { collapsed.chars().take(max).collect() }
}

/// The start of a file, as text. A cap that lands inside a character still returns
/// the valid prefix; `read_to_string` would drop the whole file.
pub(super) fn read_limited(path: &Path, cap: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?.take(cap);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub(super) fn skipped_name(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "node_modules" | "dist" | "target" | "__pycache__" | "cache" | "marketplaces")
}

/// A directory, following a symlink. `MAX_DEPTH` stops a link back to a parent.
pub(super) fn is_dir(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_dir())
}
