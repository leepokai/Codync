//! Editing an installed connector in place, so its id and every bot's reference to it stay.

use super::command_line::string_args;
use super::{Connector, MAX_CREDENTIAL_BYTES, connectors, update_connectors, verify};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// What went wrong with an edit, in words analytics may report: its message can name the user's paths.
#[derive(Clone, Copy, Debug)]
pub enum EditFailure {
    /// The edit itself: a bad field, or a connector that doesn't exist.
    Invalid,
    /// A connector this method doesn't edit.
    Refused,
    /// The edited connector failed its readiness check.
    WontStart,
    /// Something else saved the connector during the check.
    Conflict,
}

impl EditFailure {
    /// An error that reads as `why` and carries this category.
    pub fn error(self, why: impl std::fmt::Display) -> anyhow::Error {
        EditFailed { failure: self, message: format!("{why:#}") }.into()
    }
}

#[derive(Debug)]
pub struct EditFailed {
    failure: EditFailure,
    message: String,
}

impl EditFailed {
    pub fn category(&self) -> &'static str {
        match self.failure {
            EditFailure::Invalid => "invalid",
            EditFailure::Refused => "refused",
            EditFailure::WontStart => "wont_start",
            EditFailure::Conflict => "conflict",
        }
    }
}

impl std::fmt::Display for EditFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EditFailed {}

/// Edits an installed local connector: omitted fields keep their value, `args` replaces the
/// argument list, and `env` merges, with `null` removing a key. A change to how it runs is checked
/// first and exactly what started is what gets saved, so an error means this edit changed nothing,
/// and overlapping edits never combine. A new name or description starts nothing: `verify` is null.
pub async fn update_custom_connector(store: &crate::store::Store, id: &str, b: &Value, port: u16) -> Result<Value> {
    let before = connectors(store)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| EditFailure::Invalid.error("Unknown connector"))?;
    ensure_custom_local(&before).map_err(|e| EditFailure::Refused.error(e))?;
    let after = edited(&before, b).map_err(|e| EditFailure::Invalid.error(e))?;
    let runs_the_same = after.command == before.command && after.args == before.args && after.env == before.env;
    let check = if runs_the_same {
        None
    } else {
        Some(verify::check_connector(store, &after, port).await.map_err(|e| EditFailure::WontStart.error(e))?)
    };
    let connector = update_connectors(store, |all| {
        let c = all.iter_mut().find(|c| c.id == id).ok_or_else(|| EditFailure::Conflict.error("Unknown connector"))?;
        // The check can take a minute: a save in between means it didn't prove what would be saved now.
        if *c != before {
            let why = format!("{} changed while it was being checked: try again", before.name);
            return Err(EditFailure::Conflict.error(why));
        }
        let public = after.public();
        *c = after;
        Ok(public)
    })?;
    Ok(json!({"connector": connector, "verify": check}))
}

/// Registry installs rebuild their command line from the registry, and a remote connector's
/// URL and OAuth handling are the registry's or the user's to redo, so those are refused here.
fn ensure_custom_local(c: &Connector) -> Result<()> {
    if c.command.is_none() {
        bail!("{} is a remote connector: remove it and add the URL you want", c.name);
    }
    if c.registry_name.is_some() {
        bail!("{} came from the MCP registry: install it again to change how it runs", c.name);
    }
    Ok(())
}

/// `before` with the edit applied. Its `command` is the executable, never split into arguments.
fn edited(before: &Connector, b: &Value) -> Result<Connector> {
    let mut c = before.clone();
    if let Some(name) = text(b, "name")?.filter(|s| !s.is_empty()) {
        c.name = name;
    }
    if let Some(description) = text(b, "description")? {
        c.description = description;
    }
    if let Some(command) = text(b, "command")? {
        if command.is_empty() {
            bail!("give a command");
        }
        c.command = Some(command);
    }
    if let Some(args) = b.get("args").filter(|a| !a.is_null()) {
        c.args = string_args(args)?;
    }
    if let Some(env) = b.get("env") {
        merge_env(&mut c.env, env)?;
    }
    Ok(c)
}

/// A present text field: `None` when it isn't there, an error when it isn't a string.
fn text(b: &Value, key: &str) -> Result<Option<String>> {
    match b.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim().to_owned())),
        Some(_) => bail!("`{key}` is a string"),
    }
}

/// Sets `changes`' entries in `env`; `null` removes one. An empty value is a real value here, as it
/// is at install; only the credential form refuses it, because it fills in a secret.
fn merge_env(env: &mut BTreeMap<String, String>, changes: &Value) -> Result<()> {
    for (key, value) in changes.as_object().ok_or_else(|| anyhow!("`env` must be an object"))? {
        if key.is_empty() {
            bail!("`env` names can't be empty");
        }
        match value {
            Value::Null => {
                env.remove(key);
            }
            Value::String(value) if value.len() <= MAX_CREDENTIAL_BYTES => {
                env.insert(key.clone(), value.clone());
            }
            _ => bail!("`env` values are strings, or null to remove one"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{devices::Caller, dispatch};
    use crate::hub::{BotStatus, Hub};
    use crate::market::{requests, save_connectors};
    use crate::store::{BotConfig, Store};
    use std::path::Path;
    use std::time::Duration;

    fn local() -> Connector {
        serde_json::from_value(json!({
            "id": "local", "name": "Local", "description": "kept", "command": "node",
            "args": ["a.js", "--flag"], "env": {"API_KEY": "one", "GONE": "x"},
        }))
        .expect("connector")
    }

    fn store_with(saved: &[Connector]) -> Store {
        let store = Store::open(Path::new(":memory:")).expect("store");
        save_connectors(&store, saved).expect("save");
        store
    }

    fn category(error: &anyhow::Error) -> &'static str {
        error.downcast_ref::<EditFailed>().expect("a categorized failure").category()
    }

    #[test]
    fn an_edit_merges_env_and_leaves_what_it_does_not_name() {
        let edit = json!({"args": ["b.js"], "env": {"API_KEY": "two", "GONE": null, "NEW": "y"}});
        let c = edited(&local(), &edit).expect("edit");
        assert_eq!((c.id.as_str(), c.name.as_str(), c.description.as_str()), ("local", "Local", "kept"));
        assert_eq!((c.command.as_deref(), c.args.as_slice()), (Some("node"), ["b.js".to_owned()].as_slice()));
        assert_eq!((c.env["API_KEY"].as_str(), c.env["NEW"].as_str()), ("two", "y"));
        assert!(!c.env.contains_key("GONE"), "null removes a key");
        let c = edited(&c, &json!({"name": " Renamed ", "description": "new"})).expect("text");
        assert_eq!((c.name.as_str(), c.description.as_str()), ("Renamed", "new"));
    }

    #[test]
    fn a_new_command_keeps_the_arguments_until_they_are_named() {
        let mut c = local();
        for command in ["/opt/node-new/bin/node", "C:\\Program Files\\nodejs\\node.exe"] {
            c = edited(&c, &json!({"command": command})).expect("command only");
            assert_eq!(c.command.as_deref(), Some(command));
            assert_eq!(c.args, ["a.js", "--flag"], "omitting args preserves the server entry point");
        }
        c = edited(&c, &json!({"command": "node", "args": null})).expect("null args");
        assert_eq!((c.command.as_deref(), c.args.as_slice()), (Some("node"), local().args.as_slice()));
        c = edited(&c, &json!({"args": []})).expect("clear arguments");
        assert!(c.args.is_empty(), "an explicit empty list clears the arguments");
    }

    #[tokio::test]
    async fn a_bad_field_changes_nothing() {
        let saved = local();
        let store = store_with(std::slice::from_ref(&saved));
        let oversized = "x".repeat(MAX_CREDENTIAL_BYTES + 1);
        for (b, why) in [
            (json!({"command": " "}), "give a command"),
            (json!({"command": 42}), "`command` is a string"),
            (json!({"args": 42}), "`args` are strings"),
            (json!({"args": [1]}), "`args` are strings"),
            (json!({"name": 7}), "`name` is a string"),
            (json!({"description": []}), "`description` is a string"),
            (json!({"env": {"K": 1}}), "`env` values are strings"),
            (json!({"env": {"K": oversized}}), "`env` values are strings"),
            (json!({"env": {"": "x"}}), "`env` names can't be empty"),
            (json!({"env": []}), "`env` must be an object"),
            (json!({"name": "New", "args": ["b.js"], "env": {"K": 1}}), "`env` values are strings"),
        ] {
            let err = update_custom_connector(&store, "local", &b, 0).await.expect_err(why);
            assert!(err.to_string().contains(why), "{err}");
            assert_eq!(category(&err), "invalid");
            let kept = std::slice::from_ref(&saved);
            assert_eq!(connectors(&store).expect("read"), kept, "{why} left the connector untouched");
        }
    }

    #[tokio::test]
    async fn registry_and_remote_connectors_are_not_edited_in_place() {
        let mut remote = local();
        (remote.id, remote.command, remote.url) = ("remote".into(), None, Some("https://example.com/mcp".into()));
        let mut registry = local();
        (registry.id, registry.registry_name) = ("registry".into(), Some("io.example/server".into()));
        let store = store_with(&[remote, registry]);
        for (id, why) in [("remote", "remote connector"), ("registry", "MCP registry")] {
            let err = update_custom_connector(&store, id, &json!({"command": "node"}), 0).await.expect_err(id);
            assert!(err.to_string().contains(why), "{err}");
            assert_eq!(category(&err), "refused");
        }
    }

    /// A connector whose MCP server really starts, and the path of that server's script.
    fn startable() -> (Connector, String) {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/credential_mcp.py");
        let fixture = fixture.to_str().expect("utf-8 path").to_owned();
        let mut c = local();
        c.command = Some(if cfg!(windows) { "python" } else { "python3" }.to_owned());
        c.args = vec![fixture.clone()];
        c.env = [("API_KEY".to_owned(), "test-private-key".to_owned())].into();
        (c, fixture)
    }

    #[tokio::test]
    async fn overlapping_edits_never_save_a_combination_nobody_checked() {
        let (saved, fixture) = startable();
        let args = json!({"args": ["-u", &fixture]});
        // A new description starts nothing, so it lands while the check on the arguments is running.
        for other in [json!({"description": "edited"}), json!({"env": {"EXTRA": "x"}})] {
            let store = store_with(std::slice::from_ref(&saved));
            let results = tokio::join!(
                update_custom_connector(&store, "local", &args, 0),
                update_custom_connector(&store, "local", &other, 0)
            );
            let (refused, landed) = match results {
                (Ok(_), Err(refused)) => (refused, &args),
                (Err(refused), Ok(_)) => (refused, &other),
                (first, second) => panic!("one edit lands and one is refused: {first:?}, {second:?}"),
            };
            assert!(refused.to_string().contains("try again"), "{refused}");
            assert_eq!(category(&refused), "conflict");
            let expected = edited(&saved, landed).expect("a valid edit");
            assert_eq!(connectors(&store).expect("read"), [expected], "only the checked edit is saved");
        }
    }

    #[tokio::test]
    async fn an_edit_is_saved_only_once_the_connector_starts() {
        let (mut saved, fixture) = startable();
        let fixture = fixture.as_str();
        let store = store_with(std::slice::from_ref(&saved));
        let stored = || connectors(&store).expect("read");

        let err = update_custom_connector(&store, "missing", &json!({}), 0).await.expect_err("unknown");
        assert!(err.to_string().contains("Unknown connector"), "{err}");
        assert_eq!(category(&err), "invalid");
        for bad in [json!({"command": "/nonexistent/codync-connector-test"}), json!({"env": {"API_KEY": "wrong"}})] {
            let err = update_custom_connector(&store, "local", &bad, 0).await.expect_err("it won't start");
            assert_eq!(category(&err), "wont_start");
            assert_eq!(stored(), [saved.clone()], "a failed check leaves the connector as it was");
        }

        let updated =
            update_custom_connector(&store, "local", &json!({"args": ["-u", fixture]}), 0).await.expect("it starts");
        assert_eq!(
            (updated["connector"]["id"].as_str(), updated["verify"]["status"].as_str()),
            (Some("local"), Some("ready"))
        );
        assert!(!updated.to_string().contains("test-private-key"), "the answer carries no credential");
        saved.args = vec!["-u".to_owned(), fixture.to_owned()];
        assert_eq!(stored(), [saved], "one connector, same id, with the edit");
    }

    #[tokio::test]
    async fn a_new_name_is_saved_even_when_the_server_cannot_start() {
        let mut saved = local();
        saved.command = Some("/nonexistent/codync-connector-test".into());
        let store = store_with(std::slice::from_ref(&saved));

        let renamed = update_custom_connector(&store, "local", &json!({"name": "Renamed"}), 0).await.expect("rename");
        assert!(renamed["verify"].is_null(), "nothing was started: {renamed}");
        saved.name = "Renamed".to_owned();
        assert_eq!(connectors(&store).expect("read"), [saved.clone()]);

        for same in [json!({}), json!({"args": saved.args, "env": {"API_KEY": "one"}})] {
            let answer = update_custom_connector(&store, "local", &same, 0).await.expect("no change to how it runs");
            assert!(answer["verify"].is_null(), "{same}: {answer}");
        }
        let err = update_custom_connector(&store, "local", &json!({"env": {"API_KEY": "two"}}), 0).await;
        assert_eq!(category(&err.expect_err("a new key is checked")), "wont_start");
    }

    /// A bot with the connector `on`, in `folder`, with the same agent as `bot`.
    fn bot_using(hub: &Hub, id: &str, folder: &Path, on: &[&str]) -> BotConfig {
        let command = hub.store.bot("bot").expect("read").expect("the fixture's bot").config.command;
        serde_json::from_value(json!({
            "id": id, "name": id, "backend": "fixture", "cwd": folder, "command": command,
            "notify": false, "connectors": on,
        }))
        .expect("bot")
    }

    fn lines(path: &Path) -> usize {
        std::fs::read_to_string(path).map_or(0, |s| s.lines().count())
    }

    /// Runs one turn of `bot`; how many sessions its agent (running in `folder`) has opened by then.
    async fn sessions_after_a_turn(hub: &std::sync::Arc<Hub>, bot: &str, folder: &Path) -> usize {
        let prompts = folder.join("prompts.jsonl");
        let turns = lines(&prompts);
        dispatch(hub, &Caller::Local, "send", json!({"botId": bot, "text": "hello"})).await.expect("send");
        tokio::time::timeout(Duration::from_secs(30), async {
            while lines(&prompts) == turns || hub.runtime(bot).status != BotStatus::Idle {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the turn ended");
        lines(&folder.join("sessions.jsonl"))
    }

    #[tokio::test]
    async fn an_edit_keeps_each_bots_choice_and_restarts_only_the_agents_using_it() {
        // `bot` runs in `dir` with the connector off.
        let (hub, dir) = requests::tests::fixture();
        // No actor, as after a panic: listed ahead of the bot below, which must hear of the edit all the same.
        hub.store.save_bot(&bot_using(&hub, "stopped", &dir, &["fixture"])).expect("save");
        let folder = dir.join("on");
        std::fs::create_dir_all(&folder).expect("folder");
        let created = hub.create_bot(bot_using(&hub, "on", &folder, &["fixture"])).expect("create");
        let on = created["id"].as_str().expect("id");
        let choice = |id: &str| hub.store.bot(id).expect("read").expect("bot").config.connectors;
        assert_eq!(sessions_after_a_turn(&hub, "bot", &dir).await, 1);
        assert_eq!(sessions_after_a_turn(&hub, on, &folder).await, 1);

        let edit = json!({"id": "fixture", "env": {"API_KEY": "test-private-key"}});
        let answer = dispatch(&hub, &Caller::Local, "updateConnector", edit).await.expect("saved, whoever can't hear");
        assert_eq!(answer["verify"]["status"], "ready");
        assert!(connectors(&hub.store).expect("read")[0].env.contains_key("API_KEY"));
        assert_eq!((choice("bot"), choice(on)), (vec![], vec!["fixture".to_owned()]));
        assert_eq!(sessions_after_a_turn(&hub, on, &folder).await, 2, "the bot using it restarted its agent");
        let log = std::fs::read_to_string(folder.join("sessions.jsonl")).expect("log");
        assert!(log.lines().last().expect("a session").contains("session/load"), "{log}");
        assert_eq!(sessions_after_a_turn(&hub, "bot", &dir).await, 1, "a bot with it off isn't touched");

        let renamed = json!({"id": "fixture", "name": "Renamed"});
        dispatch(&hub, &Caller::Local, "updateConnector", renamed).await.expect("rename");
        assert_eq!(sessions_after_a_turn(&hub, on, &folder).await, 2, "a new name restarts nothing");

        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_relative_path_is_checked_where_the_bots_run_it() {
        let (mut saved, fixture) = startable();
        saved.args = vec!["server.py".to_owned()];
        let store = store_with(&[saved]);
        let edit = json!({"args": ["-u", "server.py"]});
        update_custom_connector(&store, "local", &edit, 0).await.expect_err("no server.py in the host's folder");

        let dir = std::env::temp_dir().join(format!("codync-connector-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("folder");
        std::fs::copy(fixture, dir.join("server.py")).expect("copy");
        let bot = json!({"id": "bot", "name": "bot", "backend": "fixture", "cwd": dir, "connectors": ["local"]});
        let bot: BotConfig = serde_json::from_value(bot).expect("bot");
        store.save_bot(&bot).expect("save");
        update_custom_connector(&store, "local", &edit, 0).await.expect("it starts in the bot's folder");
        let _ = std::fs::remove_dir_all(dir);
    }
}
