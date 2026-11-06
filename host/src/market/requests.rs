//! Durable, non-secret connection requests. Only the client submits credentials;
//! tools receive status, never credential values.
use super::connectors;
use crate::{
    agent::bot::Cmd,
    hub::Hub,
    store::{Entry, EntryKind, Lane, now_ms},
};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const INSTRUCTIONS: &str = "Connect external services with search_connectors, list_connectors and request_connection. To update an installed connector's key use request_secret. To sign in to a website or app on the screen, check list_logins, ask with request_login if it's missing, then type it with the computer tool type_login. Requests appear as secure cards in the user's conversation. Never ask for passwords or keys in ordinary chat or tool arguments. After requesting, finish your turn and wait; Codync resumes you after the user completes setup.";
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum RequestKind {
    Connection,
    Secret,
    App,
    Login,
    /// The bot's own agent (Claude Code…) needs signing in; posted by the host, not a tool.
    Agent,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum RequestStatus {
    Pending,
    Ready,
    Cancelled,
}

fn has_status(request: &Value, status: RequestStatus) -> bool {
    serde_json::from_value::<RequestStatus>(request["status"].clone()).ok() == Some(status)
}

pub(super) const MAX_AGE: i64 = 24 * 60 * 60 * 1000;
static MUTATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn tools() -> Value {
    json!([
        {"name":"search_connectors","description":"Find registry connectors by service name.","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}},
        {"name":"list_connectors","description":"List installed connectors without credentials.","inputSchema":{"type":"object","properties":{}}},
        {"name":"request_connection","description":"Invite the user to install or authorize a connector. Supply either registryName from search or connectorId from the installed list.","inputSchema":{"type":"object","properties":{"registryName":{"type":"string"},"connectorId":{"type":"string"},"reason":{"type":"string"}},"required":["reason"]}},
        {"name":"request_app","description":"Invite the user to connect a hosted app through Composio, for example gmail or slack.","inputSchema":{"type":"object","properties":{"toolkit":{"type":"string"},"reason":{"type":"string"}},"required":["toolkit","reason"]}},
        {"name":"list_logins","description":"List saved website and app logins (site and username; never passwords).","inputSchema":{"type":"object","properties":{}}},
        {"name":"request_login","description":"Ask the user to save a login for a website or app, e.g. github.com. They fill it in on a secure card; you never see the password. Then use the computer tool type_login.","inputSchema":{"type":"object","properties":{"site":{"type":"string","description":"The website's domain, or the app's name."},"reason":{"type":"string"}},"required":["site","reason"]}},
        {"name":"request_secret","description":"Ask the user to securely provide a credential for an installed connector. The value goes directly to that connector and is never returned to you.","inputSchema":{"type":"object","properties":{"connectorId":{"type":"string"},"field":{"type":"string"},"location":{"type":"string","enum":["env","header"]},"reason":{"type":"string"}},"required":["connectorId","field","location","reason"]}}
    ])
}

pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    hub.store.bot(bot)?.filter(|b| !b.deleted && !b.config.is_group()).ok_or_else(|| anyhow!("unknown bot"))?;
    match name {
        "search_connectors" => super::browse_connectors(&hub.store, text(args, "query")?, "").await,
        "list_connectors" => super::list_connectors(&hub.store),
        "list_logins" => super::logins::list(&hub.store),
        "request_login" => {
            let site = super::logins::normalize_site(text(args, "site")?);
            if site.is_empty() || site.len() > 200 {
                bail!("Give the website's domain or the app's name");
            }
            let lane = hub.runtime(bot).lane.unwrap_or_else(|| Lane::main(bot));
            let reason: String = text(args, "reason")?.chars().take(400).collect();
            let entry = hub
                .add_entry(
                    &lane,
                    EntryKind::Notice,
                    hub.store.max_turn(bot),
                    &json!({"text":reason,"connectionRequest":{"kind":RequestKind::Login,"status":RequestStatus::Pending,"title":site,"site":site,"botId":bot}}),
                )
                .ok_or_else(|| anyhow!("Could not save login request"))?;
            Ok(json!({"requestId":entry.id,"status":"waitingForUser"}))
        }
        "request_app" => {
            let toolkit = text(args, "toolkit")?.to_ascii_lowercase();
            if toolkit.len() > 100 || !toolkit.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
                bail!("Invalid app identifier");
            }
            let lane = hub.runtime(bot).lane.unwrap_or_else(|| Lane::main(bot));
            let reason: String = text(args, "reason")?.chars().take(400).collect();
            // The card shows the app's own name and logo when Composio lists it.
            let listed = super::composio::toolkits(&hub.store, &toolkit, "")
                .await
                .ok()
                .and_then(|v| v["items"].as_array()?.iter().find(|t| t["slug"] == toolkit.as_str()).cloned());
            let title = listed.as_ref().and_then(|t| t["name"].as_str()).unwrap_or(&toolkit).to_owned();
            let logo = listed.as_ref().map_or(Value::Null, |t| t["logo"].clone());
            let entry = hub.add_entry(&lane,EntryKind::Notice,hub.store.max_turn(bot),&json!({"text":reason,"connectionRequest":{
                "kind":RequestKind::App,"status":RequestStatus::Pending,"title":title,"logo":logo,"botId":bot,"toolkit":toolkit,"connectorId":format!("{}{}",super::composio::PREFIX,toolkit)
            }})).ok_or_else(|| anyhow!("Could not save connection request"))?;
            Ok(json!({"requestId":entry.id,"status":"waitingForUser"}))
        }
        "request_connection" | "request_secret" => {
            let installed = args["connectorId"].as_str().filter(|s| !s.is_empty());
            let (title, registry) = if let Some(id) = installed {
                let c = connectors(&hub.store)?
                    .into_iter()
                    .find(|c| c.id == id)
                    .ok_or_else(|| anyhow!("unknown connector"))?;
                (c.name, c.registry_name)
            } else {
                let registry = text(args, "registryName")?;
                let item = super::connector_info(&hub.store, registry).await?;
                (item["title"].as_str().unwrap_or(registry).to_owned(), Some(registry.to_owned()))
            };
            let secret = name == "request_secret";
            if secret {
                if installed.is_none() {
                    bail!("A secret request needs an installed connector");
                }
                let field = text(args, "field")?;
                if field.len() > 128 || !field.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                    bail!("Invalid credential field");
                }
                if !matches!(text(args, "location")?, "env" | "header") {
                    bail!("Invalid credential destination");
                }
            }
            let lane = hub.runtime(bot).lane.unwrap_or_else(|| Lane::main(bot));
            let request = json!({"kind":if secret {RequestKind::Secret} else {RequestKind::Connection}, "status":RequestStatus::Pending, "title":title,
                "registryName":registry, "connectorId":installed, "botId":bot,
                "field":if secret {args["field"].clone()} else {Value::Null},
                "location":if secret {args["location"].clone()} else {Value::Null}});
            let reason: String = text(args, "reason")?.chars().take(400).collect();
            let entry = hub
                .add_entry(
                    &lane,
                    EntryKind::Notice,
                    hub.store.max_turn(bot),
                    &json!({"text":reason,"connectionRequest":request}),
                )
                .ok_or_else(|| anyhow!("Could not save connection request"))?;
            Ok(json!({"requestId":entry.id,"status":"waitingForUser"}))
        }
        _ => bail!("Unknown connection tool"),
    }
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("{key} is required"))
}

fn pending(hub: &Hub, id: &str, cancelling: bool) -> Result<Entry> {
    let e = hub.store.entry(id).ok_or_else(|| anyhow!("Unknown connection request"))?;
    if !has_status(&e.data["connectionRequest"], RequestStatus::Pending) {
        bail!("This request is already completed or cancelled");
    }
    // An agent sign-in card doesn't expire; each message it holds does (`agent_sign_in`).
    let agent = serde_json::from_value(e.data["connectionRequest"]["kind"].clone()).ok() == Some(RequestKind::Agent);
    if !cancelling && !agent && now_ms() - e.created_at > MAX_AGE {
        bail!("This request expired. Ask the bot for a new request.");
    }
    let bot = text(&e.data["connectionRequest"], "botId")?;
    hub.store.bot(bot)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("The requesting bot was removed"))?;
    Ok(e)
}

pub async fn finish(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let _guard = MUTATION.lock().await;
    let id = text(args, "entryId")?;
    if let Some(entry) = hub.store.entry(id)
        && has_status(&entry.data["connectionRequest"], RequestStatus::Ready)
    {
        return Ok(json!({"entry":entry}));
    }
    let e = pending(hub, id, args["cancel"] == true)?;
    let request = &e.data["connectionRequest"];
    let kind: RequestKind = serde_json::from_value(request["kind"].clone())?;
    if args["cancel"] == true {
        let mut data = e.data.clone();
        data["connectionRequest"]["status"] = serde_json::to_value(RequestStatus::Cancelled)?;
        return Ok(json!({"entry":hub.set_entry(id, &data).ok_or_else(|| anyhow!("Could not cancel request"))?}));
    }
    if kind == RequestKind::Agent {
        let backend = text(request, "backend")?;
        if crate::agent::auth::check(&hub.store, backend).await?["signedIn"] != true {
            bail!("{} isn't signed in yet.", request["title"].as_str().unwrap_or("The agent"));
        }
        super::agent_sign_in::agent_signed_in(hub, backend)?;
        return Ok(json!({"entry": hub.store.entry(id)}));
    }
    if kind == RequestKind::Login {
        let login = super::logins::save(
            &hub.store,
            text(request, "site")?,
            args["username"].as_str().unwrap_or_default(),
            text(args, "value")?,
        )?;
        let note = format!(
            "[Login for {} saved as `{}`{}. Type it with the computer tool type_login; the password is never shown to you. Continue the task.]",
            login.site,
            login.id,
            if login.username.is_empty() { String::new() } else { format!(" (username {})", login.username) }
        );
        return complete(hub, &e, id, note);
    }
    let connector_id = request["connectorId"]
        .as_str()
        .or(args["connectorId"].as_str())
        .ok_or_else(|| anyhow!("Choose a connector"))?;
    if kind != RequestKind::App {
        super::update_connectors(&hub.store, |all| {
            let c = all.iter_mut().find(|c| c.id == connector_id).ok_or_else(|| anyhow!("Unknown connector"))?;
            if request["connectorId"].is_null() && request["registryName"].as_str() != c.registry_name.as_deref() {
                bail!("This connector does not match the request");
            }
            if kind == RequestKind::Secret {
                let value = text(args, "value")?;
                if value.len() > 64 * 1024 {
                    bail!("Credential is too large");
                }
                let field = text(request, "field")?.to_owned();
                match text(request, "location")? {
                    "env" if c.command.is_some() => {
                        c.env.insert(field, value.to_owned());
                    }
                    "header" if c.url.is_some() => {
                        c.headers.insert(field, value.to_owned());
                    }
                    _ => bail!("Credential destination does not match this connector"),
                }
            }
            Ok(())
        })?;
    }
    super::verify::verify(&hub.store, connector_id, hub.port).await?;
    let bot = text(request, "botId")?;
    let row = hub.store.bot(bot)?.ok_or_else(|| anyhow!("Unknown bot"))?;
    let mut enabled = row.config.connectors;
    if !enabled.iter().any(|s| s == connector_id) {
        enabled.push(connector_id.to_owned());
    }
    hub.update_bot(&json!({"id":bot,"connectors":enabled}))?;
    hub.send_cmd(bot, Cmd::RefreshTools)?;
    let note = format!(
        "[Connection setup completed: {}. Credentials were submitted securely and are not in this conversation. Continue the task.]",
        request["title"].as_str().unwrap_or("Connector")
    );
    complete(hub, &e, id, note)
}

/// Marks a request ready and resumes its bot once with `note`.
fn complete(hub: &Arc<Hub>, e: &Entry, id: &str, note: String) -> Result<Value> {
    let bot = text(&e.data["connectionRequest"], "botId")?.to_owned();
    let mut data = e.data.clone();
    data["connectionRequest"]["status"] = serde_json::to_value(RequestStatus::Ready)?;
    let entry = hub.set_entry(id, &data).ok_or_else(|| anyhow!("Could not complete request"))?;
    let lane = Lane::main(&e.bot_id);
    let (ack, added) = hub.add_entry_once(
        &format!("connection-ack-{id}"),
        &lane,
        EntryKind::User,
        0,
        // Written for the user, not by them: memory doesn't take it as their words.
        &json!({"text":note,"status":"queued","from":"host"}),
    )?;
    if added {
        hub.send_cmd(&bot, Cmd::Send { lane, entry_id: ack.id, text: note })?;
    }
    Ok(json!({"entry":entry}))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::store::{BotConfig, Store};
    use std::path::{Path, PathBuf};

    pub(in crate::market) fn fixture() -> (Arc<Hub>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-credentials-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(Path::new(":memory:")).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        let cfg: BotConfig = serde_json::from_value(json!({"id":"bot","name":"bot","backend":"fixture","cwd":dir,"command":crate::shell::python(&agent),"notify":false})).unwrap();
        store.save_bot(&cfg).unwrap();
        let python = if cfg!(windows) { "python" } else { "python3" };
        let c: super::super::Connector = serde_json::from_value(json!({"id":"fixture","name":"Fixture","command":python,"args":[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/credential_mcp.py")]})).unwrap();
        super::super::save_connectors(&store, &[c]).unwrap();
        let hub = Hub::new(
            store,
            "test".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "test".into(),
            19222,
        );
        hub.start().unwrap();
        (hub, dir)
    }

    #[tokio::test]
    async fn secure_submission_enables_connector_and_resumes_only_once() {
        let (hub, dir) = fixture();
        let r = call(
            &hub,
            "bot",
            "request_secret",
            &json!({"connectorId":"fixture","field":"API_KEY","location":"env","reason":"Connect the test service"}),
        )
        .await
        .unwrap();
        let id = r["requestId"].as_str().unwrap();
        let submitted = json!({"entryId":id,"value":"test-private-key"});
        let first = finish(&hub, &submitted).await.unwrap();
        assert_eq!(first["entry"]["data"]["connectionRequest"]["status"], "ready");
        assert!(!first.to_string().contains("test-private-key"));
        assert!(hub.store.bot("bot").unwrap().unwrap().config.connectors.contains(&"fixture".to_owned()));
        assert!(!hub.store.kv_read("connectors").unwrap().unwrap().contains("test-private-key"));
        finish(&hub, &submitted).await.unwrap();
        let history = hub.store.history("bot", i64::MAX, 100).unwrap();
        assert_eq!(history.iter().filter(|e| e.id == format!("connection-ack-{id}")).count(), 1);
        assert!(!serde_json::to_string(&history).unwrap().contains("test-private-key"));
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn remote_control_devices_cannot_retrieve_credentials_or_forge_requests() {
        let caller =
            crate::api::devices::Caller::Device { key: "test".into(), scopes: vec![crate::store::Scope::Control] };
        for method in ["connectorRuntime", "connectorTarget", "connectorCall"] {
            assert!(crate::api::devices::permit(&caller, method).is_err());
        }
        assert!(crate::api::devices::permit(&caller, "connectorRequestFinish").is_ok());
    }

    #[tokio::test]
    async fn saved_login_resumes_the_bot_without_its_password() {
        let (hub, dir) = fixture();
        let r = call(
            &hub,
            "bot",
            "request_login",
            &json!({"site":"https://www.GitHub.com/login","reason":"Sign in to open the PR"}),
        )
        .await
        .unwrap();
        let id = r["requestId"].as_str().unwrap();
        let done = finish(&hub, &json!({"entryId":id,"username":"kevin","value":"hunter2-private"})).await.unwrap();
        assert_eq!(done["entry"]["data"]["connectionRequest"]["status"], "ready");
        let listed = call(&hub, "bot", "list_logins", &json!({})).await.unwrap();
        assert_eq!(listed["items"][0]["site"], "github.com");
        assert_eq!(listed["items"][0]["username"], "kevin");
        assert!(!listed.to_string().contains("hunter2-private"));
        assert!(!hub.store.kv_read("logins").unwrap().unwrap().contains("hunter2-private"));
        let history = hub.store.history("bot", i64::MAX, 100).unwrap();
        let history = serde_json::to_string(&history).unwrap();
        assert!(history.contains("type_login") && !history.contains("hunter2-private"));
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn cancellation_rejects_later_secret_submission() {
        let (hub, dir) = fixture();
        let r = call(
            &hub,
            "bot",
            "request_secret",
            &json!({"connectorId":"fixture","field":"API_KEY","location":"env","reason":"Test"}),
        )
        .await
        .unwrap();
        let id = &r["requestId"];
        finish(&hub, &json!({"entryId":id,"cancel":true})).await.unwrap();
        assert!(finish(&hub, &json!({"entryId":id,"value":"test-private-key"})).await.is_err());
        assert!(connectors(&hub.store).unwrap()[0].env.is_empty());
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn failed_verification_does_not_enable_connector_or_resume() {
        let (hub, dir) = fixture();
        let r = call(
            &hub,
            "bot",
            "request_secret",
            &json!({"connectorId":"fixture","field":"API_KEY","location":"env","reason":"Test"}),
        )
        .await
        .unwrap();
        let id = r["requestId"].as_str().unwrap();
        assert!(finish(&hub, &json!({"entryId":id,"value":"wrong"})).await.is_err());
        assert_eq!(hub.store.entry(id).unwrap().data["connectionRequest"]["status"], "pending");
        assert_eq!(hub.store.bot("bot").unwrap().unwrap().config.connectors.len(), 0);
        assert!(hub.store.entry(&format!("connection-ack-{id}")).is_none());
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
