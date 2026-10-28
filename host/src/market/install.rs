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
        let args = ["runtimeArguments", "packageArguments"].iter().flat_map(|k| super::arguments::inputs(&p[*k]));
        let inputs = unique_inputs(env.chain(args));
        out.push(json!({"id": format!("package:{i}"), "kind": kind, "label": format!("{kind} · {}", p["identifier"].as_str().unwrap_or_default()), "inputs": inputs}));
    }
    for (i, r) in server["remotes"].as_array().into_iter().flatten().enumerate() {
        if !matches!(r["type"].as_str(), Some("streamable-http" | "sse")) || r["url"].as_str().is_none() {
            continue;
        }
        // `{name}` parts of the URL are the user's to fill in.
        let variables =
            r["variables"].as_object().into_iter().flatten().filter(|(_, v)| !v["value"].is_string()).map(|(k, v)| {
                json!({"name": k, "description": v["description"], "secret": v["isSecret"].as_bool().unwrap_or(false),
                   "required": v["isRequired"].as_bool().unwrap_or(false), "default": v["default"]})
            });
        let headers = r["headers"].as_array().into_iter().flatten().map(|h| {
            json!({"name": h["name"], "description": h["description"], "secret": h["isSecret"].as_bool().unwrap_or(true),
                   "required": h["isRequired"].as_bool().unwrap_or(false), "placeholder": h["value"]})
        });
        let inputs = unique_inputs(variables.chain(headers));
        out.push(json!({"id": format!("remote:{i}"), "kind": "remote", "label": "Hosted", "inputs": inputs}));
    }
    // Some servers list a remote per region; one of each kind is enough.
    let mut kinds = std::collections::HashSet::new();
    out.retain(|o| kinds.insert(o["kind"].as_str().unwrap_or_default().to_owned()));
    out
}

/// The inputs an install asks for, one per name: a sheet keys its fields by name.
fn unique_inputs(inputs: impl Iterator<Item = Value>) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    for input in inputs {
        if let Some(existing) = out.iter_mut().find(|existing| existing["name"] == input["name"]) {
            for flag in ["secret", "required"] {
                existing[flag] = json!(existing[flag] == true || input[flag] == true);
            }
        } else {
            out.push(input);
        }
    }
    out
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

/// The command that runs a registry package.
fn package_command(p: &Value) -> Option<&'static str> {
    match p["registryType"].as_str()? {
        "npm" => Some("npx"),
        "pypi" => Some("uvx"),
        "oci" => Some("docker"),
        "nuget" => Some("dnx"),
        _ => None,
    }
}

/// Installs a registry server with the user's inputs (`{NAME: value}`).
pub async fn install_connector(store: &crate::store::Store, name: &str, option: &str, inputs: &Value) -> Result<Value> {
    install_server(store, &registry_server(name).await?, name, option, inputs).await
}

/// A server that's installed already stays as it is when the call brings no values and runs it the
/// same way (Add again after its sign-in or check failed); otherwise it's rebuilt with the new ones.
async fn install_server(
    store: &crate::store::Store,
    server: &Value,
    name: &str,
    option: &str,
    inputs: &Value,
) -> Result<Value> {
    let (kind, index) = option.split_once(':').ok_or_else(|| anyhow!("unknown install option"))?;
    let index: usize = index.parse()?;
    let all = connectors(store)?;
    let typed = inputs.as_object().is_some_and(|m| m.values().any(|v| v.as_str().is_some_and(|s| !s.is_empty())));
    let command = if kind == "package" { package_command(&server["packages"][index]) } else { None };
    let stored =
        all.iter().find(|c| c.registry_name.as_deref() == Some(name)).filter(|c| c.command.as_deref() == command);
    // A connector installed before its arguments could name credentials kept a literal
    // template and no value: adding it again rebuilds it, keeping the values it saved.
    let rebuild = kind == "package"
        && stored.is_some_and(|c| {
            c.argument_templates.is_empty()
                && super::arguments::has_legacy_templates(&c.args, &server["packages"][index])
        });
    if let Some(c) = stored
        && !typed
        && !rebuild
    {
        return Ok(c.public());
    }
    let kept = if rebuild { stored.map(|c| c.env.clone()).unwrap_or_default() } else { BTreeMap::new() };
    let input =
        |k: &str| inputs[k].as_str().filter(|v| !v.is_empty()).map(str::to_owned).or_else(|| kept.get(k).cloned());
    let title =
        listing(server, &all).and_then(|l| l["title"].as_str().map(str::to_owned)).unwrap_or_else(|| name.into());
    let mut c = Connector {
        id: unique_id(&title, &all.iter().map(|c| c.id.clone()).collect::<Vec<_>>()),
        name: title,
        description: server["description"].as_str().unwrap_or_default().to_owned(),
        registry_name: Some(name.to_owned()),
        command: None,
        args: vec![],
        argument_templates: BTreeMap::new(),
        env: BTreeMap::new(),
        url: None,
        headers: BTreeMap::new(),
        oauth: None,
    };
    match kind {
        "package" => {
            let p = server["packages"].get(index).ok_or_else(|| anyhow!("unknown package"))?;
            configure_package(&mut c, p, &input)?;
        }
        "remote" => {
            let r = server["remotes"].get(index).ok_or_else(|| anyhow!("unknown remote"))?;
            c.url = Some(super::arguments::fill_url(r["url"].as_str().unwrap_or_default(), &r["variables"], &input)?);
            for h in r["headers"].as_array().into_iter().flatten() {
                let Some(k) = h["name"].as_str() else { continue };
                match input(k) {
                    Some(v) => {
                        c.headers.insert(k.to_owned(), super::arguments::fill_header(h["value"].as_str(), &v));
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

/// Builds a package command without I/O, keeping credential templates separate from their values.
fn configure_package(c: &mut Connector, p: &Value, input: &dyn Fn(&str) -> Option<String>) -> Result<()> {
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
    let runtime = super::arguments::words(&p["runtimeArguments"], &input)?;
    let package = super::arguments::words(&p["packageArguments"], &input)?;
    let mut credentials = runtime.values;
    credentials.extend(package.values);
    let command = package_command(p).ok_or_else(|| anyhow!("this package type isn't supported"))?;
    let mut args = match command {
        "npx" => vec!["-y".to_owned()],
        "docker" => {
            let mut args = vec!["run".to_owned()];
            for flag in ["-i", "--rm"] {
                let supplied = p["runtimeArguments"].as_array().into_iter().flatten().any(|a| {
                    a["type"] == "named" && a["name"] == flag && a["value"].is_null() && a["valueHint"].is_null()
                });
                if !supplied {
                    args.push(flag.into());
                }
            }
            // Only declared environment fields are present here; template values are added below.
            for k in c.env.keys() {
                args.extend(["-e".into(), k.clone()]);
            }
            args
        }
        _ => vec![],
    };
    // Registry arguments retain their order, including repeated flags and positionals.
    let offset = args.len();
    c.argument_templates.extend(runtime.templates.into_iter().map(|(index, variables)| (index + offset, variables)));
    args.extend(runtime.words.into_iter().flatten());
    args.push(reference);
    if p["registryType"] == "nuget" {
        args.push("--yes".into());
    }
    let offset = args.len();
    c.argument_templates.extend(package.templates.into_iter().map(|(index, variables)| (index + offset, variables)));
    for words in package.words {
        args.extend(words);
    }
    c.command = Some(command.into());
    c.args = args;
    c.env.extend(credentials);
    Ok(())
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
        argument_templates: BTreeMap::new(),
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
#[path = "install_tests.rs"]
mod tests;
