//! Installing connectors: registry install options and inputs, pasted configs, custom
//! servers, listing and removal.

use super::registry::{listing, registry_server};
use super::{Connector, add_connector, composio, connectors, oauth, unique_id, update_connectors};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The ways a registry server can run here, each with what the user must fill in.
pub(super) fn install_options(server: &Value) -> Vec<Value> {
    let mut out = vec![];
    for (i, p) in server["packages"].as_array().into_iter().flatten().enumerate() {
        let kind = p["registryType"].as_str().unwrap_or_default();
        if !matches!(kind, "npm" | "pypi" | "oci" | "nuget")
            || p["transport"]["type"].as_str().is_some_and(|t| t != "stdio")
        {
            continue;
        }
        let env = p["environmentVariables"].as_array().into_iter().flatten().map(|e| {
            json!({"name": e["name"], "description": e["description"], "secret": e["isSecret"].as_bool().unwrap_or(false),
                   "required": e["isRequired"].as_bool().unwrap_or(false), "default": e["default"]})
        });
        let args = ["runtimeArguments", "packageArguments"]
            .iter()
            .flat_map(|k| p[*k].as_array().into_iter().flatten())
            .filter_map(|a| {
                let key = argument_input(a)?;
                Some(json!({"name": key, "description": a["description"], "secret": a["isSecret"].as_bool().unwrap_or(false),
                            "required": a["isRequired"].as_bool().unwrap_or(false), "default": a["default"],
                            "placeholder": a["valueHint"]}))
            });
        let inputs: Vec<Value> = env.chain(args).collect();
        out.push(json!({"id": format!("package:{i}"), "kind": kind, "label": format!("{kind} · {}", p["identifier"].as_str().unwrap_or_default()), "inputs": inputs}));
    }
    for (i, r) in server["remotes"].as_array().into_iter().flatten().enumerate() {
        if !matches!(r["type"].as_str(), Some("streamable-http" | "sse")) || r["url"].as_str().is_none() {
            continue;
        }
        // `{name}` parts of the URL are the user's to fill in.
        let variables = r["variables"].as_object().into_iter().flatten().map(|(k, v)| {
            json!({"name": k, "description": v["description"], "secret": v["isSecret"].as_bool().unwrap_or(false),
                   "required": v["isRequired"].as_bool().unwrap_or(false), "default": v["default"]})
        });
        let headers = r["headers"].as_array().into_iter().flatten().map(|h| {
            json!({"name": h["name"], "description": h["description"], "secret": h["isSecret"].as_bool().unwrap_or(true),
                   "required": h["isRequired"].as_bool().unwrap_or(false), "placeholder": h["value"]})
        });
        let inputs: Vec<Value> = variables.chain(headers).collect();
        out.push(json!({"id": format!("remote:{i}"), "kind": "remote", "label": "Hosted", "inputs": inputs}));
    }
    // Some servers list a remote per region; one of each kind is enough.
    let mut kinds = std::collections::HashSet::new();
    out.retain(|o| kinds.insert(o["kind"].as_str().unwrap_or_default().to_owned()));
    out
}

/// The input a registry argument asks the user for: a positional's hint, or a named
/// option's name. None when the registry fixes it (a set value, or a bare flag).
fn argument_input(a: &Value) -> Option<String> {
    if a["value"].is_string() {
        return None;
    }
    let text = |k: &str| a[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    match a["type"].as_str() {
        Some("named") => text("name").filter(|_| a["valueHint"].is_string() || a["default"].is_string()),
        _ => text("valueHint").or_else(|| text("name")).or_else(|| Some("value".into())),
    }
}

/// A registry argument list as command-line words, taking values from the user's inputs.
fn arguments(list: &Value, input: &dyn Fn(&str) -> Option<String>) -> Result<Vec<String>> {
    let mut out = vec![];
    for a in list.as_array().into_iter().flatten() {
        let named = a["type"] == "named";
        let name = a["name"].as_str().unwrap_or_default();
        let value = match argument_input(a) {
            None => a["value"].as_str().map(str::to_owned),
            Some(key) => match input(&key).or_else(|| a["default"].as_str().map(str::to_owned)) {
                Some(v) => Some(v),
                None if a["isRequired"].as_bool().unwrap_or(false) => bail!("{key} is required"),
                None => continue,
            },
        };
        if named && !name.is_empty() {
            out.push(name.to_owned());
        }
        out.extend(value);
    }
    Ok(out)
}

/// Puts the user's values into a URL's `{name}` parts, or their defaults.
fn fill_url(url: &str, variables: &Value, input: &dyn Fn(&str) -> Option<String>) -> Result<String> {
    let mut url = url.to_owned();
    for (k, v) in variables.as_object().into_iter().flatten() {
        let value =
            input(k).or_else(|| v["default"].as_str().map(str::to_owned)).ok_or_else(|| anyhow!("{k} is required"))?;
        url = url.replace(&format!("{{{k}}}"), &value);
    }
    if url.contains('{') {
        bail!("this server's URL has parts the registry doesn't describe; add it as a custom connector");
    }
    Ok(url)
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

/// The exact package reference passed to the runtime. OCI embeds its version in
/// the identifier; server metadata cannot pin an otherwise floating image.
pub(super) fn package_reference(p: &Value) -> Result<String> {
    let id = p["identifier"].as_str().ok_or_else(|| anyhow!("package has no identifier"))?;
    let unpinned = || anyhow!("{id} has no pinned version in the MCP Registry, so it can't be installed safely");
    let pinned = |v: &str| !v.is_empty() && v != "latest" && !v.contains(['^', '~', '*', ' ']);
    if p["registryType"] == "oci" {
        let valid = if let Some((_, digest)) = id.split_once('@') {
            digest
                .strip_prefix("sha256:")
                .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        } else {
            // A registry port is not an image tag.
            id.rsplit('/').next().and_then(|name| name.rsplit_once(':')).is_some_and(|(_, tag)| {
                pinned(tag)
                    && tag.len() <= 128
                    && tag.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                    && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            })
        };
        if !valid {
            return Err(unpinned());
        }
        return Ok(id.to_owned());
    }
    let version = p["version"].as_str().filter(|v| pinned(v)).ok_or_else(unpinned)?;
    Ok(if p["registryType"] == "pypi" { format!("{id}=={version}") } else { format!("{id}@{version}") })
}

/// Installs a registry server with the user's inputs (`{NAME: value}`).
pub async fn install_connector(store: &crate::store::Store, name: &str, option: &str, inputs: &Value) -> Result<Value> {
    let server = registry_server(name).await?;
    let input = |k: &str| inputs[k].as_str().filter(|v| !v.is_empty()).map(str::to_owned);
    let (kind, index) = option.split_once(':').ok_or_else(|| anyhow!("unknown install option"))?;
    let index: usize = index.parse()?;
    let all = connectors(store)?;
    if let Some(c) = all.iter().find(|c| c.registry_name.as_deref() == Some(name)) {
        return Ok(c.public());
    }
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
        oauth: None,
    };
    match kind {
        "package" => {
            let p = server["packages"].get(index).ok_or_else(|| anyhow!("unknown package"))?;
            let reference = package_reference(p)?;
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
            let runtime = arguments(&p["runtimeArguments"], &input)?;
            let package = arguments(&p["packageArguments"], &input)?;
            let (command, mut args) = match p["registryType"].as_str() {
                Some("npm") => ("npx", vec!["-y".to_owned()]),
                Some("pypi") => ("uvx", vec![]),
                Some("oci") => {
                    let mut args = vec!["run".to_owned(), "-i".into(), "--rm".into()];
                    for k in c.env.keys() {
                        args.extend(["-e".into(), k.clone()]);
                    }
                    ("docker", args)
                }
                Some("nuget") => ("dnx", vec![]),
                _ => bail!("this package type isn't supported"),
            };
            // Runtime arguments come before the package; skip flags we already pass.
            for a in runtime {
                if !a.starts_with('-') || !args.contains(&a) {
                    args.push(a);
                }
            }
            args.push(reference);
            if p["registryType"] == "nuget" {
                args.push("--yes".into());
            }
            args.extend(package);
            c.command = Some(command.into());
            c.args = args;
        }
        "remote" => {
            let r = server["remotes"].get(index).ok_or_else(|| anyhow!("unknown remote"))?;
            c.url = Some(fill_url(r["url"].as_str().unwrap_or_default(), &r["variables"], &input)?);
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
    if let Some(url) = &c.url
        && oauth::required(url, &c.headers).await
    {
        c.oauth = Some(oauth::OAuth::default());
    }
    add_connector(store, c)
}

/// A connector you describe yourself: a command line or an https URL.
pub async fn add_custom_connector(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let name =
        b["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`name` is required"))?;
    let all = connectors(store)?;
    let c = custom(name, b, &all).await?;
    add_connector(store, c)
}

/// Adds every server in a pasted MCP config: the `mcpServers` / `servers` JSON that
/// Claude, Cursor, VS Code and most READMEs use, or one server's own entry.
pub async fn import_connectors(store: &crate::store::Store, config: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(config.trim()).context("that isn't JSON")?;
    let servers = config_servers(&v);
    if servers.is_empty() {
        bail!("no MCP servers in that config");
    }
    let all = connectors(store)?;
    let mut pending = vec![];
    for (name, entry) in servers {
        pending.push(custom(&name, &entry, &all).await.with_context(|| name.clone())?);
    }
    update_connectors(store, |all| {
        let mut added = vec![];
        for mut c in pending {
            c.id = unique_id(&c.name, &all.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
            added.push(c.public());
            all.push(c);
        }
        Ok(json!({"items":added}))
    })
}

/// The named server entries in a pasted config.
fn config_servers(v: &Value) -> Vec<(String, Value)> {
    match ["mcpServers", "servers", "context_servers"].iter().find_map(|k| v[*k].as_object()) {
        Some(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        None if v["command"].is_string() || v["url"].is_string() => {
            vec![(v["name"].as_str().unwrap_or("Custom").to_owned(), v.clone())]
        }
        // `{"github": {"command": …}}`: the map without its wrapper.
        None => v
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, e)| e.is_object())
            .map(|(k, e)| (k.clone(), e.clone()))
            .collect(),
    }
}

/// A connector from `{command, args?, env?}` or `{url, headers?}`. A command without
/// `args` is split like a shell would (quotes work).
async fn custom(name: &str, b: &Value, all: &[Connector]) -> Result<Connector> {
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
        oauth: None,
    };
    let url =
        ["url", "serverUrl", "httpUrl"].iter().find_map(|k| b[*k].as_str()).map(str::trim).filter(|s| !s.is_empty());
    if let Some(url) = url {
        let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"].iter().any(|p| url.starts_with(p));
        if !url.starts_with("https://") && !local {
            bail!("remote connectors need an https URL");
        }
        if oauth::required(url, &c.headers).await {
            c.oauth = Some(oauth::OAuth::default());
        }
        c.url = Some(url.to_owned());
        return Ok(c);
    }
    let line = b["command"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("give a command or a URL"))?;
    let mut words = match b["args"].as_array() {
        Some(args) => {
            std::iter::once(line.to_owned()).chain(args.iter().filter_map(|a| a.as_str().map(str::to_owned))).collect()
        }
        None => shlex::split(line).ok_or_else(|| anyhow!("the command has an unclosed quote"))?,
    }
    .into_iter();
    c.command = words.next();
    c.args = words.collect();
    Ok(c)
}

/// Installed MCP servers, then the apps connected through Composio.
pub fn list_connectors(store: &crate::store::Store) -> Result<Value> {
    let mut items: Vec<Value> = connectors(store)?.iter().map(Connector::public).collect();
    items.extend(composio::connections(store)?.iter().filter(|c| c.active()).map(|c| {
        json!({
            "id": format!("{}{}", composio::PREFIX, c.toolkit),
            "name": c.name,
            "description": format!("{} through Composio", c.name),
            "registryName": null,
            "kind": "composio",
            "command": null,
            "url": null,
            "keys": [],
            "logo": c.logo,
        })
    }));
    Ok(json!({"items": items}))
}

pub fn remove_connector(store: &crate::store::Store, id: &str) -> Result<()> {
    update_connectors(store, |all| {
        all.retain(|c| c.id != id);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oci_references_do_not_need_a_separate_package_version() {
        for id in [
            "ghcr.io/github/github-mcp-server:2.0.2".to_owned(),
            "localhost:5000/team/server:2.0.2".to_owned(),
            "server:v2.0.2-rc.1".to_owned(),
            format!("ghcr.io/team/server@sha256:{}", "a".repeat(64)),
            format!("ghcr.io/team/server:latest@sha256:{}", "b".repeat(64)),
        ] {
            let mut package = json!({"registryType": "oci", "identifier": id});
            assert_eq!(package_reference(&package).unwrap(), id);
            // Older registry entries also supply a separate package version.
            package["version"] = json!("0.20.0");
            assert_eq!(package_reference(&package).unwrap(), id);
        }
    }

    #[test]
    fn package_metadata_cannot_pin_a_floating_or_invalid_oci_reference() {
        for id in [
            "ghcr.io/github/github-mcp-server",
            "localhost:5000/team/server",
            "server:latest",
            "server:",
            "server:^2.0.2",
            "server:2.*",
            "server:2.0.2 invalid",
            "server:2.0.2\n",
            "server@sha256:",
            "server@sha256:not-a-digest",
            "server:2.0.2@sha256:invalid",
        ] {
            for version in [Value::Null, json!("2.0.2")] {
                let package = json!({"registryType": "oci", "identifier": id, "version": version});
                assert!(package_reference(&package).is_err(), "accepted {package}");
            }
        }
    }

    #[test]
    fn other_registries_still_require_their_own_package_version() {
        for (kind, expected) in [("npm", "server@2.0.2"), ("pypi", "server==2.0.2"), ("nuget", "server@2.0.2")] {
            let mut package = json!({"registryType": kind, "identifier": "server", "version": "2.0.2"});
            assert_eq!(package_reference(&package).unwrap(), expected);
            for version in [Value::Null, json!(""), json!("latest"), json!("^2.0.2"), json!("~2.0.2"), json!("*")] {
                package["version"] = version;
                assert!(package_reference(&package).is_err(), "accepted {package}");
            }
        }
    }

    #[test]
    fn registry_arguments_become_words() {
        let list = json!([
            {"type": "named", "name": "--rm"},
            {"type": "named", "name": "--db", "valueHint": "path", "isRequired": true},
            {"type": "named", "name": "--mode", "value": "ro"},
            {"type": "positional", "valueHint": "source", "isRequired": false},
        ]);
        let input = |k: &str| (k == "--db").then(|| "/tmp/a.db".to_owned());
        assert_eq!(arguments(&list, &input).unwrap(), ["--rm", "--db", "/tmp/a.db", "--mode", "ro"]);
        assert!(arguments(&list, &|_: &str| None).is_err(), "--db is required");
        assert_eq!(argument_input(&list[0]), None);
        assert_eq!(argument_input(&list[3]).as_deref(), Some("source"));
    }

    #[test]
    fn url_templates_fill_in() {
        let vars = json!({"agent_id": {"isRequired": true}, "region": {"default": "us"}});
        let input = |k: &str| (k == "agent_id").then(|| "a1".to_owned());
        assert_eq!(fill_url("https://x/{region}/{agent_id}/mcp", &vars, &input).unwrap(), "https://x/us/a1/mcp");
        assert!(fill_url("https://x/{agent_id}", &vars, &|_: &str| None).is_err());
        assert!(fill_url("https://x/{other}", &json!({}), &|_: &str| None).is_err());
    }

    #[test]
    fn pasted_configs_list_their_servers() {
        let claude = json!({"mcpServers": {"github": {"command": "npx", "args": ["-y", "x"]}, "linear": {"url": "https://l/mcp"}}});
        assert_eq!(config_servers(&claude).len(), 2);
        let vscode = json!({"servers": {"a": {"type": "sse", "url": "https://a/sse"}}});
        assert_eq!(config_servers(&vscode)[0].0, "a");
        let one = json!({"command": "uvx", "args": ["y"]});
        assert_eq!(config_servers(&one)[0].0, "Custom");
        let bare = json!({"fs": {"command": "npx"}});
        assert_eq!(config_servers(&bare)[0].0, "fs");
    }

    #[test]
    fn header_templates_are_filled() {
        assert_eq!(fill(Some("Bearer {api_key}"), "abc"), "Bearer abc");
        assert_eq!(fill(Some("token"), "abc"), "abc");
        assert_eq!(fill(None, "abc"), "abc");
    }
}
