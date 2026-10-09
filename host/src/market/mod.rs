//! The marketplace behind the phone's Plugins screen:
//!
//! - **Connectors** are MCP servers, found in the official MCP Registry
//!   (<https://registry.modelcontextprotocol.io>) or added by hand. They're
//!   installed once per computer and handed to the agent as `mcpServers` when a
//!   bot that has them turned on starts or resumes its session.
//! - **Apps** are connected through Composio and listed as `composio-*`
//!   connectors; see `composio`.
//! - **Skills** are instruction folders (a `SKILL.md` plus any files it uses),
//!   from <https://github.com/anthropics/skills> or written by hand, kept in
//!   `~/.codync/skills/<id>`. A bot that has one turned on is told where it is
//!   and reads it when the task calls for it.
//! - **Agents** come from the ACP registry; see `backends`.
//!
//! Registry and GitHub JSON is untrusted input: names become ids and paths, so
//! they're reduced to a safe slug first. Secret values (API keys, tokens) stay
//! on this computer; listings only say which keys are set.

pub mod composio;
pub mod logins;
pub mod oauth;
pub mod passwords;
pub mod requests;
pub mod vault;
pub mod verify;

mod install;
mod registry;
mod skills;
mod updates;

pub use install::{add_custom_connector, import_connectors, install_connector, list_connectors, remove_connector};
pub use registry::{browse_connectors, connector_info, refresh_first_page};
pub use skills::{add_custom_skill, browse_skills, install_skill, list_skills, remove_skill, skills_brief};

pub use updates::{automatic_loop, refresh_connector};

use crate::LockExt;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

const MCP_REGISTRY: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const SKILLS_REPO: &str = "anthropics/skills";
const UA: &str = concat!("codync-host/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(15);
/// Registry search is slow (often 20-30 s); lookups by name are fast.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(45);

// MARK: stored items

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Connector {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The MCP Registry name it was installed from, if any.
    #[serde(default)]
    pub registry_name: Option<String>,
    /// stdio servers: the command the agent spawns.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Remote (streamable HTTP) servers.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Remote servers that want sign-in; see `oauth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<oauth::OAuth>,
}

impl Connector {
    /// ACP `McpServer`. Remote servers run through this host's `remote` proxy (`codync-host mcp remote`),
    /// which keeps sign-in tokens fresh and works with agents that only speak stdio.
    /// A remote server still waiting for sign-in is left out.
    pub fn acp(&self, exe: &std::path::Path, port: u16) -> Option<Value> {
        if self.command.is_some() {
            return Some(
                json!({"name":self.id,"command":exe,"args":["mcp","local","--connector",self.id,"--port",port.to_string()],"env":[]}),
            );
        }
        self.url.as_ref()?;
        if self.oauth.as_ref().is_some_and(|o| !o.signed_in()) {
            return None;
        }
        Some(json!({
            "name": self.id, "command": exe,
            "args": ["mcp", "remote", "--connector", self.id, "--port", port.to_string()],
            "env": [],
        }))
    }

    /// What clients see: everything but the secret values.
    pub fn public(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "registryName": self.registry_name,
            "kind": if self.command.is_some() { "local" } else { "remote" },
            "command": self.command.clone(),
            "url": self.url.as_deref().and_then(|raw| reqwest::Url::parse(raw).ok()).map(|mut url| {
                let _ = url.set_username(""); let _ = url.set_password(None);
                url.set_query(None); url.set_fragment(None); url.to_string()
            }),
            "keys": self.env.keys().chain(self.headers.keys()).collect::<Vec<_>>(),
            "auth": match &self.oauth {
                None => "none",
                Some(o) if o.signed_in() => "signedIn",
                Some(_) => "signedOut",
            },
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// "anthropics/skills" or "custom".
    #[serde(default)]
    pub source: String,
}

impl Skill {
    pub fn dir(&self) -> PathBuf {
        skills_dir().join(&self.id)
    }

    fn public(&self) -> Value {
        json!({"id": self.id, "name": self.name, "description": self.description, "source": self.source,
               "path": self.dir().join("SKILL.md").to_string_lossy()})
    }
}

fn skills_dir() -> PathBuf {
    crate::service::data_dir().join("skills")
}

/// Lowercase ASCII letters, digits and dashes; never empty, never a path.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').chars().take(48).collect::<String>();
    if out.is_empty() { "item".into() } else { out }
}

fn load<T: for<'de> Deserialize<'de>>(store: &crate::store::Store, key: &str) -> Vec<T> {
    store.kv_get(key).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn connectors(store: &crate::store::Store) -> Result<Vec<Connector>> {
    vault::read(store, "connectors")?.map_or_else(|| Ok(Vec::new()), |s| serde_json::from_str(&s).map_err(Into::into))
}

pub fn skills(store: &crate::store::Store) -> Vec<Skill> {
    load(store, "skills")
}

fn save<T: Serialize>(store: &crate::store::Store, key: &str, items: &[T]) -> Result<()> {
    store.kv_set(key, &serde_json::to_string(items)?)
}

pub fn save_connectors(store: &crate::store::Store, items: &[Connector]) -> Result<()> {
    vault::write(store, "connectors", &serde_json::to_string(items)?)
}

/// Serialize read/modify/write so OAuth refreshes and installations cannot replace each other.
pub fn update_connectors<T>(
    store: &crate::store::Store,
    change: impl FnOnce(&mut Vec<Connector>) -> Result<T>,
) -> Result<T> {
    let _guard = store.connector_lock.locked();
    let mut all = connectors(store)?;
    let result = change(&mut all)?;
    save_connectors(store, &all)?;
    Ok(result)
}

fn add_connector(store: &crate::store::Store, mut c: Connector) -> Result<Value> {
    update_connectors(store, |all| {
        if let Some(name) = c.registry_name.as_deref()
            && let Some(existing) = all.iter().find(|x| x.registry_name.as_deref() == Some(name))
        {
            return Ok(existing.public());
        }
        c.id = unique_id(&c.name, &all.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
        let public = c.public();
        all.push(c);
        Ok(public)
    })
}

fn unique_id(base: &str, taken: &[String]) -> String {
    let base = slug(base);
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

async fn get_json(url: &str) -> Result<Value> {
    get_json_within(url, TIMEOUT).await
}

async fn get_json_within(url: &str, timeout: Duration) -> Result<Value> {
    let res = crate::http().get(url).header("user-agent", UA).timeout(timeout).send().await?;
    if !res.status().is_success() {
        bail!("{} answered {}", url.split('/').nth(2).unwrap_or(url), res.status());
    }
    Ok(res.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_safe() {
        assert_eq!(slug("GitHub MCP Server"), "github-mcp-server");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("中文"), "item");
    }

    #[test]
    fn connectors_become_acp_servers() {
        let mut c = Connector {
            id: "gh".into(),
            name: "GitHub".into(),
            description: String::new(),
            registry_name: None,
            command: Some("npx".into()),
            args: vec!["-y".into(), "pkg@1".into()],
            env: BTreeMap::from([("TOKEN".into(), "x".into())]),
            url: None,
            headers: BTreeMap::new(),
            oauth: None,
        };
        let exe = std::path::Path::new("/bin/codync-host");
        let config = c.acp(exe, 1).unwrap();
        assert_eq!(config["args"][1], "local");
        assert_eq!(config["env"], json!([]));
        assert!(!config.to_string().contains("secret"));
        assert!(c.public().get("env").is_none(), "secrets never leave the host");
        c.command = None;
        c.url = Some("https://example.com/mcp".into());
        assert_eq!(c.acp(exe, 1).unwrap()["args"][1], "remote", "remote servers go through the proxy");
        c.oauth = Some(oauth::OAuth::default());
        assert!(c.acp(exe, 1).is_none(), "not signed in yet");
        assert_eq!(c.public()["auth"], "signedOut");
    }
}
