//! Editing an installed connector in place, so its id and every bot's reference to it stay.

use super::install::split_command;
use super::update_connectors;
use anyhow::{Result, anyhow, bail};
use serde_json::Value;

/// Edits an installed local connector: omitted fields keep their value, `args` replaces the
/// argument list, and `env` merges, with `null` removing a key. Every field is checked before
/// anything is written, so one bad value leaves the connector as it was.
///
/// Registry installs rebuild their command line from the registry, and a remote connector's
/// URL and OAuth handling are the registry's or the user's to redo, so those are refused here.
pub fn update_custom_connector(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let id = b["id"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`id` is required"))?;
    update_connectors(store, |all| {
        let c = all.iter_mut().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
        if c.command.is_none() {
            bail!("{} is a remote connector: remove it and add the URL you want", c.name);
        }
        if c.registry_name.is_some() {
            bail!("{} came from the MCP registry: install it again to change how it runs", c.name);
        }
        let name = text(b, "name")?.filter(|s| !s.is_empty());
        let description = text(b, "description")?;
        let command = match (b.get("command"), b.get("args")) {
            (None, None) => None,
            (given, args) => {
                let line = match given {
                    Some(Value::String(line)) => line.clone(),
                    None => c.command.clone().unwrap_or_default(),
                    Some(_) => bail!("`command` is a string"),
                };
                Some(split_command(&line, args)?)
            }
        };
        let env = b.get("env").map(env_changes).transpose()?;

        if let Some(name) = name {
            name.clone_into(&mut c.name);
        }
        if let Some(description) = description {
            c.description = description;
        }
        if let Some((command, args)) = command {
            (c.command, c.args) = (command, args);
        }
        for (key, value) in env.into_iter().flatten() {
            match value {
                Some(value) => {
                    c.env.insert(key, value);
                }
                None => {
                    c.env.remove(&key);
                }
            }
        }
        Ok(c.public())
    })
}

/// A present text field: `None` when it isn't there, an error when it isn't a string.
fn text(b: &Value, key: &str) -> Result<Option<String>> {
    match b.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim().to_owned())),
        Some(_) => bail!("`{key}` is a string"),
    }
}

/// `env` entries to set, and the ones `null` removes. Values are checked once here, not when
/// a bot starts, so a bad entry fails at the call.
fn env_changes(env: &Value) -> Result<Vec<(String, Option<String>)>> {
    let fields = env.as_object().ok_or_else(|| anyhow!("`env` must be an object"))?;
    let mut changes = Vec::with_capacity(fields.len());
    for (key, value) in fields {
        if key.is_empty() {
            bail!("`env` names can't be empty");
        }
        match value {
            Value::Null => changes.push((key.clone(), None)),
            Value::String(value) if value.len() <= 64 * 1024 => changes.push((key.clone(), Some(value.clone()))),
            _ => bail!("`env` values are strings, or null to remove one"),
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::{Connector, add_custom_connector, connectors, install::install_server};
    use serde_json::json;

    async fn add_local(store: &crate::store::Store, b: Value) -> String {
        let added = add_custom_connector(store, &b).await.expect("add");
        added["id"].as_str().expect("id").to_owned()
    }

    fn only(store: &crate::store::Store) -> Connector {
        connectors(store).expect("read").pop().expect("one connector")
    }

    #[tokio::test]
    async fn updating_a_connector_keeps_its_id_and_merges_env() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        let id = add_local(
            &store,
            json!({"name": "Local", "command": "node old.js", "env": {"API_KEY": "one", "GONE": "x"}}),
        )
        .await;
        let updated = update_custom_connector(
            &store,
            &json!({"id": id, "command": "node", "args": ["new.js"], "env": {"API_KEY": "two", "GONE": null, "NEW": "y"}}),
        )
        .expect("update");
        assert_eq!((updated["id"].as_str(), updated["name"].as_str()), (Some(id.as_str()), Some("Local")));
        assert_eq!(connectors(&store).expect("read").len(), 1, "an update adds nothing");
        let c = only(&store);
        assert_eq!(c.id, id, "bots keep pointing at the same connector");
        assert_eq!((c.command.as_deref(), c.args.as_slice()), (Some("node"), ["new.js".to_owned()].as_slice()));
        assert_eq!(c.env["API_KEY"], "two");
        assert_eq!(c.env["NEW"], "y");
        assert!(!c.env.contains_key("GONE"), "null removes a key");
    }

    #[tokio::test]
    async fn an_update_leaves_what_it_does_not_name() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        let id = add_local(
            &store,
            json!({"name": "Local", "description": "kept", "command": "node", "args": ["a.js", "--flag"]}),
        )
        .await;
        update_custom_connector(&store, &json!({"id": id, "args": ["b.js"]})).expect("args only");
        let c = only(&store);
        assert_eq!((c.name.as_str(), c.description.as_str()), ("Local", "kept"));
        assert_eq!((c.command.as_deref(), c.args.as_slice()), (Some("node"), ["b.js".to_owned()].as_slice()));
        // With no args at all the command is a command line, split like it is when adding one.
        update_custom_connector(&store, &json!({"id": id, "command": "node c.js --quiet"})).expect("command line");
        let c = only(&store);
        assert_eq!(
            (c.command.as_deref(), c.args.as_slice()),
            (Some("node"), ["c.js".to_owned(), "--quiet".to_owned()].as_slice())
        );
    }

    #[tokio::test]
    async fn a_bad_field_changes_nothing() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        let id = add_local(&store, json!({"name": "Local", "command": "node", "args": ["a.js"]})).await;
        for (b, why) in [
            (json!({"id": "missing"}), "Unknown connector"),
            (json!({"id": id, "command": " "}), "give a command"),
            (json!({"id": id, "command": 42}), "`command` is a string"),
            (json!({"id": id, "args": 42}), "`args` are strings"),
            (json!({"id": id, "args": [1]}), "`args` are strings"),
            (json!({"id": id, "name": 7}), "`name` is a string"),
            (json!({"id": id, "description": []}), "`description` is a string"),
            (json!({"id": id, "env": {"K": 1}}), "`env` values are strings"),
            (json!({"id": id, "env": []}), "`env` must be an object"),
        ] {
            let err = update_custom_connector(&store, &b).expect_err(why).to_string();
            assert!(err.contains(why), "{err}");
        }
        let c = only(&store);
        assert_eq!(
            (c.command.as_deref(), c.args.as_slice()),
            (Some("node"), ["a.js".to_owned()].as_slice()),
            "the connector is untouched"
        );
    }

    #[tokio::test]
    async fn registry_and_remote_connectors_are_not_edited_in_place() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        let remote = add_local(&store, json!({"name": "Remote", "url": "https://example.com/mcp"})).await;
        let err = update_custom_connector(&store, &json!({"id": remote, "command": "node"})).expect_err("remote");
        assert!(err.to_string().contains("remote connector"), "{err}");
        let server = json!({"name": "io.example/server",
                            "packages": [{"registryType": "npm", "identifier": "server", "version": "1.0.0"}]});
        let c = install_server(&store, &server, "io.example/server", "package:0", &json!({})).await.expect("install");
        let err = update_custom_connector(&store, &json!({"id": c["id"], "command": "node"})).expect_err("registry");
        assert!(err.to_string().contains("MCP registry"), "{err}");
    }
}
