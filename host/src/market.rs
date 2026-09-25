//! The marketplace behind the phone's Plugins screen:
//!
//! - **Connectors** are MCP servers, found in the official MCP Registry
//!   (<https://registry.modelcontextprotocol.io>) or added by hand. They're
//!   installed once per computer and handed to the agent as `mcpServers` when a
//!   bot that has them turned on starts or resumes its session.
//! - **Skills** are instruction folders (a `SKILL.md` plus any files it uses),
//!   from <https://github.com/anthropics/skills> or written by hand, kept in
//!   `~/.codync/skills/<id>`. A bot that has one turned on is told where it is
//!   and reads it when the task calls for it.
//! - **Agents** come from the ACP registry; see `backends`.
//!
//! Registry and GitHub JSON is untrusted input: names become ids and paths, so
//! they're reduced to a safe slug first. Secret values (API keys, tokens) stay
//! on this computer; listings only say which keys are set.

use crate::LockExt;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
}

impl Connector {
    /// ACP `McpServer`. Remote servers need the agent's `mcpCapabilities.http`.
    pub fn acp(&self, http_ok: bool) -> Option<Value> {
        let pairs = |m: &BTreeMap<String, String>| -> Vec<Value> {
            m.iter().map(|(k, v)| json!({"name": k, "value": v})).collect()
        };
        if let Some(command) = &self.command {
            return Some(json!({"name": self.id, "command": command, "args": self.args, "env": pairs(&self.env)}));
        }
        let url = self.url.as_ref()?;
        http_ok.then(|| json!({"type": "http", "name": self.id, "url": url, "headers": pairs(&self.headers)}))
    }

    /// What clients see: everything but the secret values.
    fn public(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "registryName": self.registry_name,
            "kind": if self.command.is_some() { "local" } else { "remote" },
            "command": self.command.as_ref().map(|c| std::iter::once(c.clone()).chain(self.args.iter().cloned()).collect::<Vec<_>>().join(" ")),
            "url": self.url,
            "keys": self.env.keys().chain(self.headers.keys()).collect::<Vec<_>>(),
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

pub fn connectors(store: &crate::store::Store) -> Vec<Connector> {
    load(store, "connectors")
}

pub fn skills(store: &crate::store::Store) -> Vec<Skill> {
    load(store, "skills")
}

fn save<T: Serialize>(store: &crate::store::Store, key: &str, items: &[T]) -> Result<()> {
    store.kv_set(key, &serde_json::to_string(items)?)
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

// MARK: connectors

/// Well-known connectors shown before you search, by MCP Registry name.
/// Names that aren't in the registry (yet) are simply skipped.
const FEATURED: &[&str] = &[
    "io.github.github/github-mcp-server",
    "app.linear/linear",
    "com.notion/mcp",
    "com.figma.mcp/mcp",
    "com.stripe/mcp",
    "com.vercel/vercel-mcp",
    "com.supabase/mcp",
    "com.cloudflare.mcp/mcp",
    "com.atlassian/atlassian-mcp-server",
    "io.github.upstash/context7",
    "io.github.microsoft/playwright-mcp",
];

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

async fn registry_search(search: &str, limit: usize) -> Result<Vec<Value>> {
    let mut url = reqwest::Url::parse(MCP_REGISTRY)?;
    url.query_pairs_mut().append_pair("version", "latest").append_pair("limit", &limit.to_string());
    if !search.is_empty() {
        url.query_pairs_mut().append_pair("search", search);
    }
    let v = get_json_within(url.as_str(), SEARCH_TIMEOUT).await.context("searching the MCP Registry")?;
    Ok(v["servers"].as_array().cloned().unwrap_or_default().into_iter().map(|s| s["server"].clone()).collect())
}

/// One registry server by exact name (its latest version).
async fn registry_server(name: &str) -> Result<Value> {
    let mut url = reqwest::Url::parse(MCP_REGISTRY)?;
    url.path_segments_mut().map_err(|()| anyhow!("bad registry URL"))?.extend([name, "versions", "latest"]);
    let v = get_json(url.as_str()).await.with_context(|| format!("{name} isn't in the MCP Registry"))?;
    Ok(v["server"].clone())
}

/// The ways a registry server can run here, each with what the user must fill in.
fn install_options(server: &Value) -> Vec<Value> {
    let mut out = vec![];
    for (i, p) in server["packages"].as_array().into_iter().flatten().enumerate() {
        let kind = p["registryType"].as_str().unwrap_or_default();
        if !matches!(kind, "npm" | "pypi" | "oci") || p["transport"]["type"].as_str().is_some_and(|t| t != "stdio") {
            continue;
        }
        let inputs: Vec<Value> = p["environmentVariables"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                json!({"name": e["name"], "description": e["description"], "secret": e["isSecret"].as_bool().unwrap_or(false),
                       "required": e["isRequired"].as_bool().unwrap_or(false), "default": e["default"]})
            })
            .collect();
        out.push(json!({"id": format!("package:{i}"), "kind": kind, "label": format!("{kind} · {}", p["identifier"].as_str().unwrap_or_default()), "inputs": inputs}));
    }
    for (i, r) in server["remotes"].as_array().into_iter().flatten().enumerate() {
        if r["type"] != "streamable-http" || r["url"].as_str().is_none_or(|u| u.contains('{')) {
            continue;
        }
        let inputs: Vec<Value> = r["headers"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|h| {
                json!({"name": h["name"], "description": h["description"], "secret": h["isSecret"].as_bool().unwrap_or(true),
                       "required": h["isRequired"].as_bool().unwrap_or(false), "placeholder": h["value"]})
            })
            .collect();
        out.push(json!({"id": format!("remote:{i}"), "kind": "remote", "label": "Hosted", "inputs": inputs}));
    }
    // Some servers list a remote per region; one of each kind is enough.
    let mut kinds = std::collections::HashSet::new();
    out.retain(|o| kinds.insert(o["kind"].as_str().unwrap_or_default().to_owned()));
    out
}

/// "com.notion/mcp" -> "Notion", "io.github.microsoft/playwright-mcp" -> "Playwright".
fn title_from_name(name: &str) -> String {
    let (org, last) = name.split_once('/').unwrap_or(("", name));
    let words: Vec<&str> =
        last.split(['-', '_', '.']).filter(|w| !w.is_empty() && !matches!(*w, "mcp" | "server")).collect();
    let words = if words.is_empty() {
        let label = org
            .split('.')
            .find(|l| !matches!(*l, "com" | "io" | "app" | "ai" | "dev" | "org" | "github"))
            .unwrap_or(org);
        vec![label]
    } else {
        words
    };
    words
        .iter()
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn listing(server: &Value, installed: &[Connector]) -> Option<Value> {
    let name = server["name"].as_str()?;
    let options = install_options(server);
    if options.is_empty() {
        return None;
    }
    let title = server["title"].as_str().map_or_else(|| title_from_name(name), str::to_owned);
    Some(json!({
        "name": name,
        "title": title,
        "description": server["description"],
        "version": server["version"],
        "website": server["websiteUrl"].as_str().or_else(|| server["repository"]["url"].as_str()),
        "installed": installed.iter().any(|c| c.registry_name.as_deref() == Some(name)),
        "options": options,
    }))
}

/// Browse the MCP Registry: featured connectors, or a search.
pub async fn browse_connectors(store: &crate::store::Store, search: &str) -> Result<Value> {
    let installed = connectors(store);
    let servers = if search.trim().is_empty() {
        let lookups = FEATURED.iter().map(|n| registry_server(n));
        futures::future::join_all(lookups).await.into_iter().filter_map(Result::ok).collect()
    } else {
        registry_search(search.trim(), 60).await?
    };
    let mut seen = std::collections::HashSet::new();
    let items: Vec<Value> = servers
        .iter()
        .filter(|s| seen.insert(s["name"].as_str().unwrap_or_default().to_owned()))
        .filter_map(|s| listing(s, &installed))
        .collect();
    Ok(json!({"items": items}))
}

/// Fills `{placeholder}` in a header template with the user's value, or uses it as-is.
fn fill(template: Option<&str>, value: &str) -> String {
    match template {
        Some(t) if t.contains('{') && t.contains('}') => {
            let (head, rest) = t.split_once('{').unwrap_or((t, ""));
            let tail = rest.split_once('}').map_or("", |(_, tail)| tail);
            format!("{head}{value}{tail}")
        }
        _ => value.to_owned(),
    }
}

/// Installs a registry server with the user's inputs (`{NAME: value}`).
pub async fn install_connector(store: &crate::store::Store, name: &str, option: &str, inputs: &Value) -> Result<Value> {
    let server = registry_server(name).await?;
    let input = |k: &str| inputs[k].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    let (kind, index) = option.split_once(':').ok_or_else(|| anyhow!("unknown install option"))?;
    let index: usize = index.parse()?;
    let mut all = connectors(store);
    let title =
        listing(&server, &all).and_then(|l| l["title"].as_str().map(str::to_owned)).unwrap_or_else(|| name.into());
    let mut c = Connector {
        id: unique_id(&title, &all.iter().map(|c| c.id.clone()).collect::<Vec<_>>()),
        name: title,
        description: server["description"].as_str().unwrap_or_default().to_owned(),
        registry_name: Some(name.to_owned()),
        command: None,
        args: vec![],
        env: BTreeMap::new(),
        url: None,
        headers: BTreeMap::new(),
    };
    match kind {
        "package" => {
            let p = server["packages"].get(index).ok_or_else(|| anyhow!("unknown package"))?;
            let id = p["identifier"].as_str().ok_or_else(|| anyhow!("package has no identifier"))?;
            let version = p["version"].as_str().unwrap_or("latest");
            for e in p["environmentVariables"].as_array().into_iter().flatten() {
                let Some(k) = e["name"].as_str() else { continue };
                match input(k).or_else(|| e["default"].as_str().map(str::to_owned)) {
                    Some(v) => {
                        c.env.insert(k.to_owned(), v);
                    }
                    None if e["isRequired"].as_bool().unwrap_or(false) => bail!("{k} is required"),
                    None => {}
                }
            }
            match p["registryType"].as_str() {
                Some("npm") => {
                    c.command = Some("npx".into());
                    c.args = vec!["-y".into(), format!("{id}@{version}")];
                }
                Some("pypi") => {
                    c.command = Some("uvx".into());
                    c.args = vec![format!("{id}=={version}")];
                }
                Some("oci") => {
                    c.command = Some("docker".into());
                    c.args = vec!["run".into(), "-i".into(), "--rm".into()];
                    for k in c.env.keys() {
                        c.args.extend(["-e".into(), k.clone()]);
                    }
                    c.args.push(id.to_owned());
                }
                _ => bail!("this package type isn't supported"),
            }
        }
        "remote" => {
            let r = server["remotes"].get(index).ok_or_else(|| anyhow!("unknown remote"))?;
            c.url = r["url"].as_str().map(str::to_owned);
            for h in r["headers"].as_array().into_iter().flatten() {
                let Some(k) = h["name"].as_str() else { continue };
                match input(k) {
                    Some(v) => {
                        c.headers.insert(k.to_owned(), fill(h["value"].as_str(), &v));
                    }
                    None if h["isRequired"].as_bool().unwrap_or(false) => bail!("{k} is required"),
                    None => {}
                }
            }
        }
        _ => bail!("unknown install option"),
    }
    all.push(c.clone());
    save(store, "connectors", &all)?;
    Ok(c.public())
}

/// A connector you describe yourself: a command line or an https URL.
pub fn add_custom_connector(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let name =
        b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`name` is required"))?;
    let mut all = connectors(store);
    let map = |v: &Value| -> BTreeMap<String, String> {
        v.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned()))).collect()
    };
    let mut c = Connector {
        id: unique_id(name, &all.iter().map(|c| c.id.clone()).collect::<Vec<_>>()),
        name: name.to_owned(),
        description: b["description"].as_str().unwrap_or_default().to_owned(),
        registry_name: None,
        command: None,
        args: vec![],
        env: map(&b["env"]),
        url: None,
        headers: map(&b["headers"]),
    };
    if let Some(url) = b["url"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        if !url.starts_with("https://") && !url.starts_with("http://localhost") && !url.starts_with("http://127.0.0.1")
        {
            bail!("remote connectors need an https URL");
        }
        c.url = Some(url.to_owned());
    } else {
        let line = b["command"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("give a command or a URL"))?;
        let mut parts = line.split_whitespace().map(str::to_owned);
        c.command = parts.next();
        c.args = parts.collect();
    }
    all.push(c.clone());
    save(store, "connectors", &all)?;
    Ok(c.public())
}

pub fn list_connectors(store: &crate::store::Store) -> Value {
    json!({"items": connectors(store).iter().map(Connector::public).collect::<Vec<_>>()})
}

pub fn remove_connector(store: &crate::store::Store, id: &str) -> Result<()> {
    let mut all = connectors(store);
    all.retain(|c| c.id != id);
    save(store, "connectors", &all)
}

// MARK: skills

static SKILL_CATALOG: Mutex<Option<(Instant, Value)>> = Mutex::new(None);

/// `name` and `description` from a SKILL.md front matter.
fn front_matter(md: &str) -> (Option<String>, Option<String>) {
    let mut name = None;
    let mut description = None;
    let mut lines = md.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().trim_matches('"').to_owned());
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().trim_matches('"').to_owned());
        }
    }
    (name, description)
}

/// Skills published in anthropics/skills (cached for an hour).
pub async fn browse_skills(store: &crate::store::Store) -> Result<Value> {
    let installed: Vec<String> = skills(store).into_iter().map(|s| s.id).collect();
    let cached = SKILL_CATALOG
        .locked()
        .as_ref()
        .filter(|(at, _)| at.elapsed() < Duration::from_secs(3600))
        .map(|(_, v)| v.clone());
    let catalog = if let Some(v) = cached {
        v
    } else {
        let dirs = get_json(&format!("https://api.github.com/repos/{SKILLS_REPO}/contents/skills"))
            .await
            .context("listing skills on GitHub")?;
        let names: Vec<String> = dirs
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["type"] == "dir")
            .filter_map(|d| d["name"].as_str().map(str::to_owned))
            .collect();
        let fetches = names.iter().map(|dir| async move {
            let url = format!("https://raw.githubusercontent.com/{SKILLS_REPO}/main/skills/{dir}/SKILL.md");
            let md = crate::http().get(&url).header("user-agent", UA).timeout(TIMEOUT).send().await.ok()?.text().await.ok()?;
            let (name, description) = front_matter(&md);
            Some(json!({"source": dir, "name": name.unwrap_or_else(|| dir.clone()), "description": description.unwrap_or_default()}))
        });
        let v = Value::Array(futures::future::join_all(fetches).await.into_iter().flatten().collect());
        *SKILL_CATALOG.locked() = Some((Instant::now(), v.clone()));
        v
    };
    let items: Vec<Value> = catalog
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            let mut s = s.clone();
            s["installed"] = json!(installed.contains(&slug(s["source"].as_str().unwrap_or_default())));
            s
        })
        .collect();
    Ok(json!({"items": items}))
}

/// Downloads a skill folder from anthropics/skills (files up to 1 MB, two levels deep).
async fn download_dir(repo_path: &str, dest: &std::path::Path, depth: u8) -> Result<()> {
    let listing = get_json(&format!("https://api.github.com/repos/{SKILLS_REPO}/contents/{repo_path}")).await?;
    tokio::fs::create_dir_all(dest).await?;
    for item in listing.as_array().into_iter().flatten() {
        let name = item["name"].as_str().unwrap_or_default();
        // Names become paths: plain file names only.
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
            continue;
        }
        match item["type"].as_str() {
            Some("file") if item["size"].as_u64().unwrap_or(0) <= 1_000_000 => {
                let Some(url) = item["download_url"].as_str() else { continue };
                let bytes =
                    crate::http().get(url).header("user-agent", UA).timeout(TIMEOUT).send().await?.bytes().await?;
                tokio::fs::write(dest.join(name), &bytes).await?;
            }
            Some("dir") if depth > 0 => {
                Box::pin(download_dir(&format!("{repo_path}/{name}"), &dest.join(name), depth - 1)).await?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub async fn install_skill(store: &crate::store::Store, source: &str) -> Result<Value> {
    let id = slug(source);
    let mut all = skills(store);
    if let Some(s) = all.iter().find(|s| s.id == id) {
        return Ok(s.public());
    }
    let dest = skills_dir().join(&id);
    download_dir(&format!("skills/{source}"), &dest, 2).await.context("downloading the skill")?;
    let md = tokio::fs::read_to_string(dest.join("SKILL.md")).await.context("the skill has no SKILL.md")?;
    let (name, description) = front_matter(&md);
    let s = Skill {
        id,
        name: name.unwrap_or_else(|| source.to_owned()),
        description: description.unwrap_or_default(),
        source: SKILLS_REPO.into(),
    };
    all.push(s.clone());
    save(store, "skills", &all)?;
    Ok(s.public())
}

/// A skill you write yourself: a name, when to use it, and the instructions.
pub fn add_custom_skill(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let name =
        b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`name` is required"))?;
    let description = b["description"].as_str().unwrap_or_default().trim().replace('\n', " ");
    let body = b["instructions"].as_str().unwrap_or_default();
    let mut all = skills(store);
    let s = Skill {
        id: unique_id(name, &all.iter().map(|s| s.id.clone()).collect::<Vec<_>>()),
        name: name.to_owned(),
        description,
        source: "custom".into(),
    };
    std::fs::create_dir_all(s.dir())?;
    std::fs::write(
        s.dir().join("SKILL.md"),
        format!("---\nname: {}\ndescription: {}\n---\n\n{body}\n", s.name, s.description),
    )?;
    all.push(s.clone());
    save(store, "skills", &all)?;
    Ok(s.public())
}

pub fn list_skills(store: &crate::store::Store) -> Value {
    json!({"items": skills(store).iter().map(Skill::public).collect::<Vec<_>>()})
}

pub fn remove_skill(store: &crate::store::Store, id: &str) -> Result<()> {
    let mut all = skills(store);
    if let Some(s) = all.iter().find(|s| s.id == id) {
        let _ = std::fs::remove_dir_all(s.dir());
    }
    all.retain(|s| s.id != id);
    save(store, "skills", &all)
}

/// The skills block of a bot's profile: what each is for and where to read it.
pub fn skills_brief(store: &crate::store::Store, enabled: &[String]) -> Option<String> {
    let lines: Vec<String> = skills(store)
        .iter()
        .filter(|s| enabled.contains(&s.id))
        .map(|s| {
            format!("- {}: {} (read {} before using it)", s.name, s.description, s.dir().join("SKILL.md").display())
        })
        .collect();
    (!lines.is_empty()).then(|| format!("Skills you can use when they fit the task:\n{}", lines.join("\n")))
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
    fn titles_come_from_names() {
        assert_eq!(title_from_name("com.notion/mcp"), "Notion");
        assert_eq!(title_from_name("com.vercel/vercel-mcp"), "Vercel");
        assert_eq!(title_from_name("io.github.microsoft/playwright-mcp"), "Playwright");
    }

    #[test]
    fn header_templates_are_filled() {
        assert_eq!(fill(Some("Bearer {api_key}"), "abc"), "Bearer abc");
        assert_eq!(fill(Some("token"), "abc"), "abc");
        assert_eq!(fill(None, "abc"), "abc");
    }

    #[test]
    fn front_matter_is_read() {
        let md = "---\nname: pdf\ndescription: \"Work with PDFs\"\n---\n# PDF";
        assert_eq!(front_matter(md), (Some("pdf".into()), Some("Work with PDFs".into())));
        assert_eq!(front_matter("# no front matter"), (None, None));
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
        };
        assert_eq!(c.acp(false).unwrap()["env"][0]["name"], "TOKEN");
        assert!(c.public().get("env").is_none(), "secrets never leave the host");
        c.command = None;
        c.url = Some("https://example.com/mcp".into());
        assert!(c.acp(false).is_none(), "remote needs the agent's http capability");
        assert_eq!(c.acp(true).unwrap()["type"], "http");
    }
}
