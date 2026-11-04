//! Editing an installed connector in place, so its id and every bot's reference to it stay.

use super::command_line::split_command;
use super::{Connector, MAX_CREDENTIAL_BYTES, connectors, update_connectors, verify};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

/// Edits an installed local connector: omitted fields keep their value, `args` replaces the
/// argument list, and `env` merges, with `null` removing a key. Exactly what started is what gets
/// saved, so an error means this edit changed nothing, and overlapping edits never combine.
///
/// Registry installs rebuild their command line from the registry, and a remote connector's
/// URL and OAuth handling are the registry's or the user's to redo, so those are refused here.
pub async fn update_custom_connector(store: &crate::store::Store, b: &Value, port: u16) -> Result<Value> {
    let id = b["id"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`id` is required"))?;
    let before = connectors(store)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
    let mut edited = before.clone();
    apply(&mut edited, b)?;
    let check = verify::check_connector(store, &edited, port).await?;
    let connector = update_connectors(store, |all| {
        let c = all.iter_mut().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
        // The check can take a minute: a save in between means it didn't prove what would be saved now.
        if *c != before {
            bail!("{} changed while it was being checked: try again", before.name);
        }
        let public = edited.public();
        *c = edited;
        Ok(public)
    })?;
    Ok(json!({"connector": connector, "verify": check}))
}

/// Applies an edit, reading every field before writing any, so one bad value changes nothing.
fn apply(c: &mut Connector, b: &Value) -> Result<()> {
    if c.command.is_none() {
        bail!("{} is a remote connector: remove it and add the URL you want", c.name);
    }
    if c.registry_name.is_some() {
        bail!("{} came from the MCP registry: install it again to change how it runs", c.name);
    }
    let name = text(b, "name")?.filter(|s| !s.is_empty());
    let description = text(b, "description")?;
    let command = match (b.get("command"), b.get("args").filter(|a| !a.is_null())) {
        (None, None) => None,
        (given, args) => {
            let line = match given {
                Some(Value::String(line)) => line.clone(),
                None => c.command.clone().unwrap_or_default(),
                Some(_) => bail!("`command` is a string"),
            };
            // Updates treat the executable and its arguments as independent fields.
            let preserved_args = json!(&c.args);
            Some(split_command(&line, Some(args.unwrap_or(&preserved_args)))?)
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
    Ok(())
}

/// A present text field: `None` when it isn't there, an error when it isn't a string.
fn text(b: &Value, key: &str) -> Result<Option<String>> {
    match b.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim().to_owned())),
        Some(_) => bail!("`{key}` is a string"),
    }
}

/// `env` entries to set, and the ones `null` removes. An empty value is a real value here, as it
/// is at install; only the credential form refuses it, because it fills in a secret.
fn env_changes(env: &Value) -> Result<Vec<(String, Option<String>)>> {
    let fields = env.as_object().ok_or_else(|| anyhow!("`env` must be an object"))?;
    let mut changes = Vec::with_capacity(fields.len());
    for (key, value) in fields {
        if key.is_empty() {
            bail!("`env` names can't be empty");
        }
        match value {
            Value::Null => changes.push((key.clone(), None)),
            Value::String(value) if value.len() <= MAX_CREDENTIAL_BYTES => {
                changes.push((key.clone(), Some(value.clone())));
            }
            _ => bail!("`env` values are strings, or null to remove one"),
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::save_connectors;

    fn local() -> Connector {
        serde_json::from_value(json!({
            "id": "local", "name": "Local", "description": "kept", "command": "node",
            "args": ["a.js", "--flag"], "env": {"API_KEY": "one", "GONE": "x"},
        }))
        .expect("connector")
    }

    #[test]
    fn an_edit_merges_env_and_leaves_what_it_does_not_name() {
        let mut c = local();
        apply(&mut c, &json!({"args": ["b.js"], "env": {"API_KEY": "two", "GONE": null, "NEW": "y"}})).expect("edit");
        assert_eq!((c.id.as_str(), c.name.as_str(), c.description.as_str()), ("local", "Local", "kept"));
        assert_eq!((c.command.as_deref(), c.args.as_slice()), (Some("node"), ["b.js".to_owned()].as_slice()));
        assert_eq!((c.env["API_KEY"].as_str(), c.env["NEW"].as_str()), ("two", "y"));
        assert!(!c.env.contains_key("GONE"), "null removes a key");
        apply(&mut c, &json!({"name": " Renamed ", "description": "new"})).expect("text");
        assert_eq!((c.name.as_str(), c.description.as_str()), ("Renamed", "new"));
    }

    #[test]
    fn a_new_command_keeps_the_arguments_until_they_are_named() {
        let mut c = local();
        for command in ["/opt/node-new/bin/node", "C:\\Program Files\\nodejs\\node.exe"] {
            apply(&mut c, &json!({"command": command})).expect("command only");
            assert_eq!(c.command.as_deref(), Some(command));
            assert_eq!(c.args, ["a.js", "--flag"], "omitting args preserves the server entry point");
        }
        apply(&mut c, &json!({"command": "node", "args": null})).expect("null args");
        assert_eq!((c.command.as_deref(), c.args.as_slice()), (Some("node"), local().args.as_slice()));
        apply(&mut c, &json!({"args": []})).expect("clear arguments");
        assert!(c.args.is_empty(), "an explicit empty list clears the arguments");
    }

    #[test]
    fn a_bad_field_changes_nothing() {
        let before = local();
        for (b, why) in [
            (json!({"command": " "}), "give a command"),
            (json!({"command": 42}), "`command` is a string"),
            (json!({"args": 42}), "`args` are strings"),
            (json!({"args": [1]}), "`args` are strings"),
            (json!({"name": 7}), "`name` is a string"),
            (json!({"description": []}), "`description` is a string"),
            (json!({"env": {"K": 1}}), "`env` values are strings"),
            (json!({"env": []}), "`env` must be an object"),
            (json!({"name": "New", "args": ["b.js"], "env": {"K": 1}}), "`env` values are strings"),
        ] {
            let mut c = before.clone();
            let err = apply(&mut c, &b).expect_err(why).to_string();
            assert!(err.contains(why), "{err}");
            assert_eq!(c, before, "{why} left the connector untouched");
        }
    }

    #[test]
    fn registry_and_remote_connectors_are_not_edited_in_place() {
        let mut remote = local();
        (remote.command, remote.url) = (None, Some("https://example.com/mcp".into()));
        let err = apply(&mut remote, &json!({"command": "node"})).expect_err("remote");
        assert!(err.to_string().contains("remote connector"), "{err}");
        let mut registry = local();
        registry.registry_name = Some("io.example/server".into());
        let err = apply(&mut registry, &json!({"command": "node"})).expect_err("registry");
        assert!(err.to_string().contains("MCP registry"), "{err}");
    }

    /// A connector whose MCP server really starts, and the path of that server's script.
    fn startable() -> (Connector, String) {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/credential_mcp.py");
        let fixture = fixture.to_str().expect("utf-8 path").to_owned();
        let mut c = local();
        c.command = Some(if cfg!(windows) { "python" } else { "python3" }.to_owned());
        c.args = vec![fixture.clone()];
        c.env = [("API_KEY".to_owned(), "test-private-key".to_owned())].into();
        (c, fixture)
    }

    #[tokio::test]
    async fn overlapping_edits_never_save_a_combination_nobody_checked() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        let (saved, fixture) = startable();
        save_connectors(&store, std::slice::from_ref(&saved)).expect("save");

        // Each reads the saved connector before either check ends, as two edits a minute apart can.
        let description = json!({"id": "local", "description": "edited"});
        let args = json!({"id": "local", "args": ["-u", &fixture]});
        let results =
            tokio::join!(update_custom_connector(&store, &description, 0), update_custom_connector(&store, &args, 0));

        let mut expected = saved;
        match results {
            (Ok(_), Err(refused)) => {
                assert!(refused.to_string().contains("try again"), "{refused}");
                expected.description = "edited".to_owned();
            }
            (Err(refused), Ok(_)) => {
                assert!(refused.to_string().contains("try again"), "{refused}");
                expected.args = vec!["-u".to_owned(), fixture];
            }
            (first, second) => panic!("one edit lands and one is refused: {first:?}, {second:?}"),
        }
        assert_eq!(connectors(&store).expect("read"), [expected], "only the checked edit is saved");
    }

    #[tokio::test]
    async fn an_edit_is_saved_only_once_the_connector_starts() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
        let (mut saved, fixture) = startable();
        let fixture = fixture.as_str();
        save_connectors(&store, std::slice::from_ref(&saved)).expect("save");
        let stored = || connectors(&store).expect("read");

        let err = update_custom_connector(&store, &json!({"id": "missing"}), 0).await.expect_err("unknown");
        assert!(err.to_string().contains("Unknown connector"), "{err}");
        for bad in [
            json!({"id": "local", "command": "/nonexistent/codync-connector-test"}),
            json!({"id": "local", "env": {"API_KEY": "wrong"}}),
        ] {
            update_custom_connector(&store, &bad, 0).await.expect_err("it won't start");
            assert_eq!(stored(), [saved.clone()], "a failed check leaves the connector as it was");
        }

        let updated = update_custom_connector(&store, &json!({"id": " local ", "args": ["-u", fixture]}), 0)
            .await
            .expect("it starts");
        assert_eq!(
            (updated["connector"]["id"].as_str(), updated["verify"]["status"].as_str()),
            (Some("local"), Some("ready"))
        );
        assert!(!updated.to_string().contains("test-private-key"), "the answer carries no credential");
        saved.args = vec!["-u".to_owned(), fixture.to_owned()];
        assert_eq!(stored(), [saved], "one connector, same id, with the edit");
    }
}
