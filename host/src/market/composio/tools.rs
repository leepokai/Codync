//! Composio tools of the built-in `composio` MCP server: what a bot sees
//! (`tools`, `INSTRUCTIONS`) and what its calls do, limited to the apps it has on.

use super::{Connection, PREFIX, checked_slug, config, connections, get, post, text};
use crate::store::Store;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

pub const INSTRUCTIONS: &str = "Act in the user's connected apps through Composio. Find a tool with \
`search_tools`, read its parameters with `get_tool`, then run it with `execute`. Tool slugs look like \
GMAIL_SEND_EMAIL. Only the apps listed by `list_apps` are available to you. An app can have several \
accounts (e.g. several Gmail inboxes): `list_apps` gives each its `account` id; pass it to `execute` to pick \
one. To read or search across apps with several accounts, run the read tool once per account. Anything that \
writes (sending, replying, creating, deleting) runs once, on the account the user meant; if that isn't clear, \
ask which one.";

// MARK: tools for bots

/// MCP tools of the built-in `composio` server.
pub fn tools() -> Value {
    json!([
        {
            "name": "list_apps",
            "description": "The connected apps you may use, one entry per connected account (`account` id).",
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
                "properties": {
                    "slug": {"type": "string"},
                    "arguments": {"type": "object"},
                    "account": {"type": "string", "description": "Which connected account to use (its `account` id from list_apps). Default: the first one."},
                },
                "required": ["slug", "arguments"],
            },
        },
    ])
}

/// Toolkits bot `bot` may use: its `composio-*` connectors that are connected.
fn allowed(store: &Store, bot: &str) -> Result<Vec<Connection>> {
    let row = store.bot(bot)?.ok_or_else(|| anyhow!("unknown bot"))?;
    let on: Vec<&str> = row.config.connectors.iter().filter_map(|id| id.strip_prefix(PREFIX)).collect();
    Ok(connections(store)?.into_iter().filter(|c| c.active() && on.contains(&c.toolkit.as_str())).collect())
}

/// Toolkits a bot has turned on, for its MCP server list (none: no server).
pub fn enabled_for(store: &Store, connectors: &[String]) -> Result<bool> {
    if !connectors.iter().any(|s| s.starts_with(PREFIX)) {
        return Ok(false);
    }
    let conns = connections(store)?;
    Ok(connectors
        .iter()
        .filter_map(|id| id.strip_prefix(PREFIX))
        .any(|t| conns.iter().any(|c| c.active() && c.toolkit == t)))
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
        "list_apps" => Ok(json!(
            apps.iter().map(|a| json!({"slug": a.toolkit, "name": a.name, "account": a.id})).collect::<Vec<_>>()
        )),
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
                        "description": t["description"].as_str().map(|d| crate::agent::acp::truncate(d, 300)),
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
            let app = pick(&apps, &toolkit, args["account"].as_str()).map_err(|e| anyhow!("{slug}: {e}"))?;
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

/// The connection a tool runs on: the given `account` of `toolkit`, else its first connection.
fn pick<'a>(apps: &'a [Connection], toolkit: &str, account: Option<&str>) -> Result<&'a Connection> {
    let of_toolkit = |a: &&Connection| a.toolkit.eq_ignore_ascii_case(toolkit);
    match account.filter(|s| !s.is_empty()) {
        Some(id) => apps.iter().filter(of_toolkit).find(|a| a.id == id).ok_or_else(|| {
            anyhow!("account `{id}` isn't a connected {toolkit} account this bot can use (see list_apps)")
        }),
        None => apps.iter().find(of_toolkit).ok_or_else(|| anyhow!("belongs to an app this bot can't use")),
    }
}

async fn tool(key: &str, slug: &str) -> Result<Value> {
    get(key, &format!("/tools/{slug}"), &[("toolkit_versions", "latest")]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(id: &str, toolkit: &str) -> Connection {
        Connection { id: id.into(), toolkit: toolkit.into(), name: String::new(), logo: None, status: "active".into() }
    }

    #[test]
    fn pick_chooses_the_requested_account() {
        let apps = [conn("ca_1", "gmail"), conn("ca_2", "gmail"), conn("ca_3", "slack")];
        assert_eq!(pick(&apps, "gmail", None).unwrap().id, "ca_1");
        assert_eq!(pick(&apps, "gmail", Some("")).unwrap().id, "ca_1");
        assert_eq!(pick(&apps, "GMAIL", Some("ca_2")).unwrap().id, "ca_2");
        assert!(pick(&apps, "gmail", Some("ca_3")).is_err(), "an account of another app");
        assert!(pick(&apps, "gmail", Some("ca_9")).is_err(), "an unknown account");
        assert!(pick(&apps, "notion", None).is_err(), "an app the bot can't use");
    }
}
