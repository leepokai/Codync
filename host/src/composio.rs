//! Composio (<https://composio.dev>): hosted sign-in and tools for ~1,000 apps
//! (Gmail, Slack, GitHub, Notion…), as Marketplace connectors.
//!
//! The user pastes a Composio API key once. Connecting an app goes through
//! Composio's hosted auth link (OAuth works from the phone: Composio owns the
//! callback) or, for key-based apps, a form whose fields the toolkit declares.
//! Each connected app shows up as a connector `composio-<toolkit>`; a bot with
//! some turned on gets the built-in `composio` MCP server, limited to those
//! apps, which searches, describes and runs their tools through this host.
//! The key never leaves the computer.

use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

const API: &str = "https://backend.composio.dev/api/v3.1";
const TIMEOUT: Duration = Duration::from_secs(30);
/// Connector ids for Composio apps: `composio-<toolkit slug>`.
pub const PREFIX: &str = "composio-";
const SLOT: &str = "composio";
pub const KEY_URL: &str = "https://platform.composio.dev";

pub const INSTRUCTIONS: &str = "Act in the user's connected apps through Composio. Find a tool with \
`search_tools`, read its parameters with `get_tool`, then run it with `execute`. Tool slugs look like \
GMAIL_SEND_EMAIL. Only the apps listed by `list_apps` are available to you.";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    key: String,
    /// Composio's "user": one per computer.
    user_id: String,
    #[serde(default)]
    connections: Vec<Connection>,
}

/// A connected account, cached so connector listings work offline.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// Composio's connected account id.
    pub id: String,
    pub toolkit: String,
    pub name: String,
    #[serde(default)]
    pub logo: Option<String>,
    /// Composio's status, lowercased (`active`, `initiated`, `failed`, `expired`…).
    pub status: String,
}

impl Connection {
    pub fn active(&self) -> bool {
        self.status == "active"
    }
}

fn load(store: &Store) -> Option<Config> {
    store.kv_get(SLOT).and_then(|s| serde_json::from_str::<Config>(&s).ok()).filter(|c| !c.key.is_empty())
}

fn save(store: &Store, c: &Config) -> Result<()> {
    store.kv_set(SLOT, &serde_json::to_string(c)?)
}

fn config(store: &Store) -> Result<Config> {
    load(store).ok_or_else(|| anyhow!("Set up Composio in Marketplace first."))
}

/// Cached connected apps (possibly stale; `refresh` updates them).
pub fn connections(store: &Store) -> Vec<Connection> {
    load(store).map(|c| c.connections).unwrap_or_default()
}

async fn request(
    key: &str,
    method: reqwest::Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<&Value>,
) -> Result<Value> {
    let url = reqwest::Url::parse_with_params(&format!("{API}{path}"), query)?;
    let mut req = crate::http().request(method.clone(), url).header("x-api-key", key).timeout(TIMEOUT);
    if let Some(body) = body {
        req = req.json(body);
    }
    let res = req.send().await.with_context(|| format!("can't reach Composio ({method} {path})"))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = v["error"]["message"]
            .as_str()
            .or(v["message"].as_str())
            .map_or_else(|| text.chars().take(300).collect(), str::to_owned);
        if status.as_u16() == 401 {
            bail!("Composio didn't accept the API key: {msg}");
        }
        bail!("Composio: {msg} ({status})");
    }
    Ok(v)
}

async fn get(key: &str, path: &str, query: &[(&str, &str)]) -> Result<Value> {
    request(key, reqwest::Method::GET, path, query, None).await
}

async fn post(key: &str, path: &str, body: &Value) -> Result<Value> {
    request(key, reqwest::Method::POST, path, &[], Some(body)).await
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// Lowercase letters, digits, `_` and `-`: toolkit and tool slugs go into URLs.
fn checked_slug(s: &str) -> Result<&str> {
    let ok = !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok { Ok(s) } else { bail!("`{s}` isn't a Composio slug") }
}

/// `google_calendar` → `Google Calendar`.
fn title_case(s: &str) -> String {
    s.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// MARK: setup

pub fn status(store: &Store) -> Value {
    json!({"configured": load(store).is_some(), "keyUrl": KEY_URL})
}

/// Checks the key with Composio, then keeps it. An empty key forgets Composio.
pub async fn set_key(store: &Store, key: &str) -> Result<Value> {
    let key = key.trim();
    if key.is_empty() {
        store.kv_set(SLOT, "")?;
        return Ok(status(store));
    }
    get(key, "/toolkits", &[("limit", "1")]).await?;
    let mut c = load(store).unwrap_or_default();
    c.key = key.to_owned();
    if c.user_id.is_empty() {
        c.user_id = format!("codync-{}", uuid::Uuid::new_v4());
    }
    save(store, &c)?;
    refresh(store).await?;
    Ok(status(store))
}

/// Re-reads this computer's connected accounts from Composio.
pub async fn refresh(store: &Store) -> Result<Vec<Connection>> {
    let mut c = config(store)?;
    let v = get(&c.key, "/connected_accounts", &[("user_ids", &c.user_id), ("limit", "200")]).await?;
    let mut list: Vec<Connection> = v["items"]
        .as_array()
        .into_iter()
        .flatten()
        // Belt and braces: the filter is a query parameter Composio could ignore.
        .filter(|a| a["user_id"].as_str().is_none_or(|u| u == c.user_id))
        .filter_map(|a| {
            Some(Connection {
                id: text(&a["id"])?,
                toolkit: text(&a["toolkit"]["slug"])?.to_lowercase(),
                name: text(&a["toolkit"]["name"]).unwrap_or_default(),
                logo: text(&a["toolkit"]["logo"]),
                status: text(&a["status"]).unwrap_or_default().to_lowercase(),
            })
        })
        .collect();
    // Names and logos from the toolkit list when the account listing omits them.
    for conn in &mut list {
        if let Some(old) = c.connections.iter().find(|o| o.toolkit == conn.toolkit) {
            if conn.name.is_empty() {
                conn.name.clone_from(&old.name);
            }
            if conn.logo.is_none() {
                conn.logo.clone_from(&old.logo);
            }
        }
        if conn.name.is_empty() {
            conn.name = title_case(&conn.toolkit);
        }
    }
    c.connections.clone_from(&list);
    save(store, &c)?;
    Ok(list)
}

/// Toolkits to connect: the popular ones, or matches for `search`.
pub async fn toolkits(store: &Store, search: &str, cursor: &str) -> Result<Value> {
    let c = config(store)?;
    let mut query = vec![("limit", "30")];
    let search = search.trim();
    if !search.is_empty() {
        query.push(("search", search));
    }
    if !cursor.is_empty() {
        query.push(("cursor", cursor));
    }
    let (v, connected) = tokio::join!(get(&c.key, "/toolkits", &query), refresh(store));
    let v = v?;
    let connected = connected.unwrap_or(c.connections);
    let items: Vec<Value> = v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let slug = text(&t["slug"])?.to_lowercase();
            // Composio's own meta-toolkits aren't apps.
            if slug.starts_with("composio") {
                return None;
            }
            let conn = connected.iter().find(|c| c.toolkit == slug);
            Some(json!({
                "slug": slug,
                "name": text(&t["name"]).unwrap_or_else(|| slug.clone()),
                "description": text(&t["meta"]["description"]).or_else(|| text(&t["description"])),
                "logo": text(&t["meta"]["logo"]).or_else(|| text(&t["logo"])),
                "connection": conn.map(|c| json!({"id": c.id, "status": c.status})),
            }))
        })
        .collect();
    Ok(json!({"items": items, "nextCursor": text(&v["next_cursor"])}))
}

// MARK: connecting

/// Starts connecting `toolkit`: a hosted link to open, or the fields a key-based app needs.
pub async fn connect(store: &Store, toolkit: &str) -> Result<Value> {
    let c = config(store)?;
    let slug = checked_slug(toolkit)?.to_lowercase();
    let detail = get(&c.key, &format!("/toolkits/{slug}"), &[]).await?;
    let managed = detail["composio_managed_auth_schemes"].as_array().is_some_and(|a| !a.is_empty());
    if !managed && let Some((mode, fields)) = key_fields(&detail) {
        return Ok(json!({"status": "needsFields", "mode": mode, "fields": fields}));
    }
    let auth_config = managed_auth_config(&c.key, &slug).await?;
    let v =
        post(&c.key, "/connected_accounts/link", &json!({"auth_config_id": auth_config, "user_id": c.user_id})).await?;
    let url = text(&v["redirect_url"]).ok_or_else(|| anyhow!("Composio didn't return a sign-in link"))?;
    Ok(json!({"status": "redirect", "url": url, "connection": text(&v["connected_account_id"])}))
}

/// The first non-OAuth scheme a toolkit declares, with its input fields.
fn key_fields(detail: &Value) -> Option<(String, Vec<Value>)> {
    detail["auth_config_details"].as_array()?.iter().find_map(|d| {
        let mode = text(&d["mode"])?.to_uppercase();
        if mode.starts_with("OAUTH") {
            return None;
        }
        let init = &d["fields"]["connected_account_initiation"];
        let mut fields: Vec<Value> = [("required", true), ("optional", false)]
            .iter()
            .flat_map(|(k, required)| {
                init[*k].as_array().into_iter().flatten().filter_map(move |f| {
                    let name = text(&f["name"])?;
                    let secret = f["is_secret"].as_bool().unwrap_or_else(|| {
                        let n = name.to_lowercase();
                        ["key", "token", "secret", "password"].iter().any(|h| n.contains(h))
                    });
                    Some(json!({
                        "name": name,
                        "label": text(&f["display_name"]).unwrap_or_else(|| title_case(&name)),
                        "description": text(&f["description"]),
                        "secret": secret,
                        "required": *required || f["required"].as_bool().unwrap_or(false),
                    }))
                })
            })
            .collect();
        if fields.is_empty() {
            fields.push(json!({"name": "generic_api_key", "label": "API key", "secret": true, "required": true}));
        }
        Some((mode, fields))
    })
}

/// A Composio-managed auth config for `slug`, made on first use.
async fn managed_auth_config(key: &str, slug: &str) -> Result<String> {
    let list = get(key, "/auth_configs", &[("toolkit_slug", slug), ("limit", "100")]).await?;
    let existing = list["items"].as_array().into_iter().flatten().find(|a| {
        a["is_composio_managed"] == true
            && text(&a["status"]).is_none_or(|s| s.eq_ignore_ascii_case("enabled") || s.eq_ignore_ascii_case("active"))
    });
    if let Some(id) = existing.and_then(|a| text(&a["id"])) {
        return Ok(id);
    }
    let v = post(
        key,
        "/auth_configs",
        &json!({"toolkit": {"slug": slug}, "auth_config": {"type": "use_composio_managed_auth", "name": format!("Codync {slug}")}}),
    )
    .await?;
    text(&v["auth_config"]["id"])
        .or_else(|| text(&v["id"]))
        .ok_or_else(|| anyhow!("Composio didn't return an auth config"))
}

/// Connects a key-based app with the fields `connect` asked for.
pub async fn connect_with_fields(
    store: &Store,
    toolkit: &str,
    mode: &str,
    fields: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let c = config(store)?;
    let slug = checked_slug(toolkit)?.to_lowercase();
    let mode = checked_slug(mode)?.to_uppercase();
    let values: serde_json::Map<String, Value> = fields
        .iter()
        .filter(|(_, v)| v.as_str().is_some_and(|s| !s.trim().is_empty()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if values.is_empty() {
        bail!("Fill in the fields first.");
    }
    let config = post(
        &c.key,
        "/auth_configs",
        &json!({"toolkit": {"slug": slug}, "auth_config": {"type": "use_custom_auth", "authScheme": mode, "name": format!("Codync {slug}")}}),
    )
    .await?;
    let config = text(&config["auth_config"]["id"])
        .or_else(|| text(&config["id"]))
        .ok_or_else(|| anyhow!("Composio didn't return an auth config"))?;
    let v = post(
        &c.key,
        "/connected_accounts",
        &json!({"auth_config": {"id": config}, "connection": {"user_id": c.user_id, "state": {"authScheme": mode, "val": values}}}),
    )
    .await?;
    let id = text(&v["id"]).ok_or_else(|| anyhow!("Composio didn't return the connection"))?;
    connection(store, &id).await
}

/// One connection's state (polled while the user signs in).
pub async fn connection(store: &Store, id: &str) -> Result<Value> {
    let c = config(store)?;
    let v = get(&c.key, &format!("/connected_accounts/{}", checked_slug(id)?), &[]).await?;
    let status = text(&v["status"]).unwrap_or_default().to_lowercase();
    if status == "active" {
        refresh(store).await?;
    }
    Ok(json!({"id": id, "status": status, "toolkit": text(&v["toolkit"]["slug"])}))
}

/// Disconnects the app behind connector `composio-<toolkit>` (every account of it).
pub async fn disconnect(store: &Store, toolkit: &str) -> Result<()> {
    let c = config(store)?;
    for conn in refresh(store).await?.iter().filter(|x| x.toolkit == toolkit) {
        request(
            &c.key,
            reqwest::Method::DELETE,
            &format!("/connected_accounts/{}", checked_slug(&conn.id)?),
            &[],
            None,
        )
        .await?;
    }
    refresh(store).await?;
    Ok(())
}

// MARK: tools for bots

/// MCP tools of the built-in `composio` server.
pub fn tools() -> Value {
    json!([
        {
            "name": "list_apps",
            "description": "The connected apps you may use.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "search_tools",
            "description": "Find tools in the connected apps by what they do, e.g. \"send an email\" or \"create an issue\".",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "What you want to do."},
                    "app": {"type": "string", "description": "Limit to one app (its slug from list_apps)."},
                },
                "required": ["query"],
            },
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "get_tool",
            "description": "A tool's description and input parameters (JSON Schema).",
            "inputSchema": {"type": "object", "properties": {"slug": {"type": "string"}}, "required": ["slug"]},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "execute",
            "description": "Run a tool with arguments that match its input parameters.",
            "inputSchema": {
                "type": "object",
                "properties": {"slug": {"type": "string"}, "arguments": {"type": "object"}},
                "required": ["slug", "arguments"],
            },
        },
    ])
}

/// Toolkits bot `bot` may use: its `composio-*` connectors that are connected.
fn allowed(store: &Store, bot: &str) -> Result<Vec<Connection>> {
    let row = store.bot(bot)?.ok_or_else(|| anyhow!("unknown bot"))?;
    let on: Vec<&str> = row.config.connectors.iter().filter_map(|id| id.strip_prefix(PREFIX)).collect();
    Ok(connections(store).into_iter().filter(|c| c.active() && on.contains(&c.toolkit.as_str())).collect())
}

/// Toolkits a bot has turned on, for its MCP server list (none: no server).
pub fn enabled_for(store: &Store, connectors: &[String]) -> bool {
    let conns = connections(store);
    connectors
        .iter()
        .filter_map(|id| id.strip_prefix(PREFIX))
        .any(|t| conns.iter().any(|c| c.active() && c.toolkit == t))
}

/// A tool call from a bot's `composio` MCP server.
pub async fn call(store: &Store, bot: &str, name: &str, args: &Value) -> Result<Value> {
    let c = config(store)?;
    let apps = allowed(store, bot)?;
    if apps.is_empty() {
        bail!("No connected apps are turned on for this bot.");
    }
    let app_of = |toolkit: &str| apps.iter().find(|a| a.toolkit.eq_ignore_ascii_case(toolkit));
    match name {
        "list_apps" => Ok(json!(apps.iter().map(|a| json!({"slug": a.toolkit, "name": a.name})).collect::<Vec<_>>())),
        "search_tools" => {
            let query = args["query"].as_str().unwrap_or_default();
            let targets: Vec<&Connection> = match args["app"].as_str().filter(|s| !s.is_empty()) {
                Some(app) => {
                    vec![app_of(app).ok_or_else(|| anyhow!("`{app}` isn't one of your apps (see list_apps)"))?]
                }
                None => apps.iter().collect(),
            };
            let mut found = vec![];
            for app in targets.iter().take(8) {
                let v =
                    get(&c.key, "/tools", &[("toolkit_slug", &app.toolkit), ("query", query), ("limit", "8")]).await?;
                for t in v["items"].as_array().into_iter().flatten() {
                    found.push(json!({
                        "slug": t["slug"], "name": t["name"], "app": app.toolkit,
                        "description": t["description"].as_str().map(|d| crate::acp::truncate(d, 300)),
                    }));
                }
            }
            Ok(json!(found))
        }
        "get_tool" => {
            let slug = checked_slug(args["slug"].as_str().unwrap_or_default())?;
            let t = tool(&c.key, slug).await?;
            let toolkit = text(&t["toolkit"]["slug"]).unwrap_or_default();
            app_of(&toolkit).ok_or_else(|| anyhow!("{slug} belongs to an app this bot can't use"))?;
            Ok(
                json!({"slug": t["slug"], "name": t["name"], "description": t["description"], "input_parameters": t["input_parameters"]}),
            )
        }
        "execute" => {
            let slug = checked_slug(args["slug"].as_str().unwrap_or_default())?;
            let t = tool(&c.key, slug).await?;
            let toolkit = text(&t["toolkit"]["slug"]).unwrap_or_default();
            let app = app_of(&toolkit).ok_or_else(|| anyhow!("{slug} belongs to an app this bot can't use"))?;
            let arguments = args.get("arguments").filter(|a| a.is_object()).cloned().unwrap_or_else(|| json!({}));
            let v = post(
                &c.key,
                &format!("/tools/execute/{slug}"),
                &json!({"user_id": c.user_id, "connected_account_id": app.id, "arguments": arguments}),
            )
            .await?;
            if v["successful"] == false {
                bail!("{}", text(&v["error"]).unwrap_or_else(|| "the tool failed".into()));
            }
            Ok(v["data"].clone())
        }
        other => bail!("unknown tool {other}"),
    }
}

async fn tool(key: &str, slug: &str) -> Result<Value> {
    get(key, &format!("/tools/{slug}"), &[("toolkit_versions", "latest")]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_key_based_fields() {
        let detail = json!({
            "composio_managed_auth_schemes": [],
            "auth_config_details": [
                {"mode": "OAUTH2", "fields": {}},
                {"mode": "API_KEY", "fields": {"connected_account_initiation": {
                    "required": [{"name": "api_key", "display_name": "API Key"}],
                    "optional": [{"name": "subdomain", "description": "Your workspace"}],
                }}},
            ],
        });
        let (mode, fields) = key_fields(&detail).unwrap();
        assert_eq!(mode, "API_KEY");
        assert_eq!(fields[0]["name"], "api_key");
        assert_eq!(fields[0]["secret"], true);
        assert_eq!(fields[0]["required"], true);
        assert_eq!(fields[1]["label"], "Subdomain");
        assert_eq!(fields[1]["secret"], false);
        assert_eq!(fields[1]["required"], false);
    }

    #[test]
    fn slugs_stay_out_of_paths() {
        assert!(checked_slug("GMAIL_SEND_EMAIL").is_ok());
        assert!(checked_slug("ca_abc-123").is_ok());
        assert!(checked_slug("../auth_configs").is_err());
        assert!(checked_slug("a/b").is_err());
        assert!(checked_slug("").is_err());
    }
}
