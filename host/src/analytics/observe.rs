//! What an API call stands for: `api::dispatch` reports each successful call as one event.
//! Values come from a request only when they are fixed names, booleans, counts or catalog ids.

use super::{Event, platform_of};
use crate::api::devices::Caller;
use crate::hub::Hub;
use crate::store::BotConfig;
use serde_json::{Map, Value};

/// Methods that stand for one event with one fixed property.
const FIXED: [(&str, Event, &str, &str); 8] = [
    ("editHandoff", Event::HandoffAnswered, "action", "edit"),
    ("keepConversation", Event::HandoffAnswered, "action", "keep"),
    ("credentialSaveLogin", Event::CredentialSaved, "kind", "login"),
    ("credentialSetOnePassword", Event::CredentialSaved, "kind", "onepassword"),
    ("credentialUpdateConnector", Event::CredentialSaved, "kind", "connector"),
    ("setComposioKey", Event::CredentialSaved, "kind", "composio"),
    ("setVoiceKey", Event::CredentialSaved, "kind", "voice"),
    ("setAgentEnv", Event::CredentialSaved, "kind", "agent_env"),
];

/// Switches the host owns (`setting_changed`): the method and the setting it names.
const SETTINGS: [(&str, &str); 4] = [
    ("setCloud", "remote_access"),
    ("setApproval", "approval"),
    ("setHostAutomaticUpdates", "automatic_updates"),
    ("setScreenEnabled", "remote_screen"),
];

/// The event an API call stands for, with its properties, read before the call runs.
pub fn observe(caller: &Caller, method: &str, b: &Value) -> Option<(Event, Map<String, Value>)> {
    let mut p = Map::new();
    let event = if let Some(&(_, event, key, value)) = FIXED.iter().find(|f| f.0 == method) {
        // An empty key or token removes the saved one.
        let cleared = matches!(method, "setComposioKey" | "setVoiceKey" | "credentialSetOnePassword")
            && ["key", "token"].iter().all(|k| b[*k].as_str().is_none_or(|v| v.trim().is_empty()));
        if cleared {
            return None;
        }
        p.insert(key.into(), value.into());
        event
    } else if let Some(&(_, setting)) = SETTINGS.iter().find(|s| s.0 == method) {
        let on = if method == "setApproval" { b["approval"] == "auto" } else { b["enabled"] == true };
        p.insert("setting".into(), setting.into());
        p.insert("on".into(), on.into());
        Event::SettingChanged
    } else {
        call_event(method, b, &mut p)?
    };
    let client = if matches!(caller, Caller::Local) { "local" } else { "remote" };
    p.insert("client".into(), client.into());
    Some((event, p))
}

/// The rest of `observe`: one arm per method.
fn call_event(method: &str, b: &Value, p: &mut Map<String, Value>) -> Option<Event> {
    Some(match method {
        "createBot" => {
            let group = b["kind"] == "group";
            p.insert("kind".into(), (if group { "group" } else { "bot" }).into());
            if group {
                p.insert("members".into(), b["members"].as_array().map_or(0, Vec::len).into());
            } else {
                p.insert("agent".into(), b["backend"].clone());
                p.insert("model".into(), b["model"].clone());
            }
            Event::BotCreated
        }
        "updateBot" => Event::BotUpdated,
        "reorderBots" => Event::BotsReordered,
        "deleteBot" => Event::BotDeleted,
        "send" => {
            let reply = b["replyTo"].as_str().is_some_and(|t| !t.is_empty());
            p.insert("reply".into(), reply.into());
            p.insert("attachments".into(), b["attachments"].as_array().map_or(0, Vec::len).into());
            Event::MessageSent
        }
        "stop" => Event::TurnStopped,
        "newSession" => Event::SessionReset,
        "respondPermission" => Event::PermissionAnswered,
        "react" => Event::ReactionAdded,
        "saveRoutine" => Event::RoutineSaved,
        "setRoutineEnabled" => {
            p.insert("enabled".into(), b["enabled"].as_bool()?.into());
            Event::RoutineToggled
        }
        "deleteRoutine" => Event::RoutineDeleted,
        "runRoutine" => Event::RoutineRun,
        "retryRoutineRun" => Event::RoutineRetried,
        "logCall" => {
            p.insert("seconds".into(), b["seconds"].as_u64().unwrap_or(0).into());
            Event::VoiceCallFinished
        }
        "screenPrepare" => Event::RemoteScreenOpened,
        // Handing the screen back is not a takeover.
        "screenTakeover" if b["on"] == true => Event::ScreenTakenOver,
        "installConnector" => {
            // A catalog name is public; a custom connector's URL or command is not.
            let catalog = b["registryName"].as_str().unwrap_or("custom");
            p.insert("connector".into(), catalog.into());
            Event::ConnectorInstalled
        }
        // A Composio app: counted once its connection is active (`answered`).
        "composioConnection" | "composioConnectFields" => {
            p.insert("connector".into(), "composio".into());
            Event::ConnectorInstalled
        }
        "importConnectors" => Event::ConnectorsImported,
        "removeConnector" => Event::ConnectorRemoved,
        "connectorSignIn" => Event::ConnectorSigninStarted,
        "installSkill" => {
            p.insert("custom".into(), (!b["source"].is_string()).into());
            Event::SkillInstalled
        }
        "removeSkill" => Event::SkillRemoved,
        // A bot's `save_skill`: a new skill, or new text for one (`id`).
        "chatCall" if b["name"] == crate::chat::skills::TOOL => {
            let rewrite = crate::chat::skills::rewritten_id(&b["arguments"]).is_some();
            p.insert("change".into(), (if rewrite { "update" } else { "new" }).into());
            Event::SkillWritten
        }
        // A bot's `publish_artifact`: a new artifact, or a new version of one (`artifact`); never its title.
        "chatCall" if b["name"] == "publish_artifact" => {
            let update = b["arguments"]["artifact"].as_str().is_some_and(|id| !id.is_empty());
            p.insert("change".into(), (if update { "update" } else { "new" }).into());
            Event::ArtifactPublished
        }
        "agentSetup" | "agentAuthenticate" => {
            // An agent's catalog id; Node.js setup names none.
            if let Some(agent) = b["backend"].as_str().filter(|a| !a.is_empty()) {
                p.insert("agent".into(), agent.into());
            }
            if method == "agentSetup" { Event::AgentSetupStarted } else { Event::AgentSignedIn }
        }
        "installHostUpdate" => Event::HostUpdateInstalled,
        "pairing" => Event::PairingCodeCreated,
        // Approving is `device_paired`, from `cloud::decide`.
        "decideAccessRequest" if b["approve"] == false => Event::AccessRequestDenied,
        "revokeDevice" => Event::DeviceRevoked,
        _ => return None,
    })
}

/// `observe` plus what only the store knows: a resent message (same `clientNonce`) is
/// not a new one, a message names its bot's agent, an edit the settings it changes, a
/// permission answer the kind of option picked, and a phone its platform.
pub fn observe_call(hub: &Hub, caller: &Caller, method: &str, b: &Value) -> Option<(Event, Map<String, Value>)> {
    let (event, mut p) = observe(caller, method, b)?;
    match event {
        Event::MessageSent => {
            let bot = b["botId"].as_str().unwrap_or_default();
            let nonce = b["clientNonce"].as_str().unwrap_or_default();
            if !nonce.is_empty() && hub.store.find_by_nonce(bot, nonce).is_some() {
                return None;
            }
            if let Ok(Some(row)) = hub.store.bot(bot) {
                p.insert("kind".into(), (if row.config.is_group() { "group" } else { "bot" }).into());
                if !row.config.is_group() {
                    p.insert("agent".into(), row.config.backend.into());
                }
            }
        }
        Event::BotUpdated => {
            if let Ok(Some(row)) = hub.store.bot(b["id"].as_str().unwrap_or_default()) {
                p.insert("fields".into(), changed_fields(&row.config, b).into());
            }
        }
        Event::PermissionAnswered => {
            let card = hub.store.entry(b["entryId"].as_str().unwrap_or_default());
            p.insert("choice".into(), choice(card.as_ref().map(|e| &e.data), &b["optionId"]).into());
        }
        _ => {}
    }
    if let Caller::Device { key, .. } = caller {
        p.insert("platform".into(), platform_of(hub, key).into());
    }
    Some((event, p))
}

/// What only the answer tells: how many connectors an import added, whether a Composio app is
/// connected yet (its sign-in is polled until the connection is active), and whether a published
/// artifact's card went into its bot's chat. `None`: not an event.
pub fn answered(event: Event, mut p: Map<String, Value>, answer: &Value) -> Option<Map<String, Value>> {
    match event {
        Event::ConnectorsImported => {
            p.insert("count".into(), answer["items"].as_array().map_or(0, Vec::len).into());
        }
        // A routine run, a room or a bot request gets no card (`card` says why).
        Event::ArtifactPublished => {
            p.insert("card".into(), answer["card"].is_null().into());
        }
        Event::ConnectorInstalled
            if p.get("connector").and_then(Value::as_str) == Some("composio") && answer["status"] != "active" =>
        {
            return None;
        }
        _ => {}
    }
    Some(p)
}

/// The bot settings a patch changes, by name (never their values), sorted.
fn changed_fields(stored: &BotConfig, patch: &Value) -> Vec<String> {
    let Ok(Value::Object(before)) = serde_json::to_value(stored) else { return vec![] };
    let mut merged = before.clone();
    for (k, v) in patch.as_object().into_iter().flatten() {
        merged.insert(k.clone(), v.clone());
    }
    // Read back as a bot: unknown keys fall away, and a value written another way (`null` for
    // `false`, `""` for the bot's own workspace) counts as unchanged.
    let Ok(mut after) = serde_json::from_value::<BotConfig>(Value::Object(merged)) else { return vec![] };
    if !after.is_group() && after.cwd.is_empty() {
        after.cwd = crate::agent::workspace::path(&after.id).to_string_lossy().into_owned();
    }
    let Ok(Value::Object(after)) = serde_json::to_value(after) else { return vec![] };
    let mut fields: Vec<String> = patch
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k)
        .filter(|k| !matches!(k.as_str(), "id" | "createdAt") && after.get(*k) != before.get(*k))
        .cloned()
        .collect();
    fields.sort();
    fields
}

/// The kind of option a permission answer picked (ACP's `kind`), from its card.
fn choice(card: Option<&Value>, option: &Value) -> &'static str {
    let kind = card
        .and_then(|c| c["options"].as_array())
        .and_then(|o| o.iter().find(|o| !option.is_null() && o["optionId"] == *option))
        .and_then(|o| o["kind"].as_str());
    match kind {
        Some("allow_once") => "allow_once",
        Some("allow_always") => "allow_always",
        Some("reject_once") => "reject_once",
        Some("reject_always") => "reject_always",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Device, DeviceSource, Store};
    use serde_json::json;

    fn temp_hub() -> (std::sync::Arc<Hub>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-analytics-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        (Hub::new(store, "h".into(), identity, "t".into(), 0), dir)
    }

    fn props(method: &str, b: &Value) -> Option<(Event, Value)> {
        observe(&Caller::Local, method, b).map(|(e, p)| (e, Value::Object(p)))
    }

    #[test]
    fn observed_events_carry_no_content() {
        let b = json!({"botId": "b1", "text": "secret plan", "replyTo": "e1", "attachments": ["u1", "u2"]});
        let (event, p) = props("send", &b).expect("send is tracked");
        assert_eq!(event, Event::MessageSent);
        assert_eq!(p, json!({"reply": true, "attachments": 2, "client": "local"}));
        assert!(!p.to_string().contains("secret"));

        let custom = json!({"name": "mine", "command": "/Users/me/bin/server"});
        let (_, p) = props("installConnector", &custom).expect("tracked");
        assert_eq!(p["connector"], "custom");
        assert!(props("history", &json!({})).is_none());
        assert!(props("memory", &json!({"botId": "b1"})).is_none(), "reading memory is not an edit");
        assert!(props("memoryFile", &json!({"name": "notes.md"})).is_none());
    }

    #[test]
    fn handoff_and_credentials_name_a_fixed_action() {
        assert_eq!(
            props("keepConversation", &json!({})),
            Some((Event::HandoffAnswered, json!({"action": "keep", "client": "local"})))
        );
        assert_eq!(props("editHandoff", &json!({"text": "x"})).map(|(_, p)| p["action"].clone()), Some(json!("edit")));

        let login = json!({"site": "example.com", "username": "me", "password": "hunter2"});
        assert_eq!(
            props("credentialSaveLogin", &login),
            Some((Event::CredentialSaved, json!({"kind": "login", "client": "local"})))
        );
        assert_eq!(
            props("setVoiceKey", &json!({"provider": "openai", "key": "sk-1"})).map(|(_, p)| p["kind"].clone()),
            Some(json!("voice"))
        );
        assert_eq!(
            props("setAgentEnv", &json!({"backend": "codex", "vars": {"K": "v"}})).map(|(_, p)| p["kind"].clone()),
            Some(json!("agent_env"))
        );
        assert!(props("setComposioKey", &json!({"key": " "})).is_none(), "an empty key removes it");
        assert!(props("credentialSetOnePassword", &json!({"token": ""})).is_none());
    }

    #[test]
    fn settings_routines_screen_and_devices() {
        let setting = |m: &str, b: Value| props(m, &b).map(|(e, p)| (e, p["setting"].clone(), p["on"].clone()));
        assert_eq!(
            setting("setCloud", json!({"enabled": true})),
            Some((Event::SettingChanged, json!("remote_access"), json!(true)))
        );
        assert_eq!(setting("setApproval", json!({"approval": "auto"})).map(|s| s.2), Some(json!(true)));
        assert_eq!(setting("setApproval", json!({"approval": "code"})).map(|s| s.2), Some(json!(false)));
        assert_eq!(
            setting("setHostAutomaticUpdates", json!({"enabled": false})).map(|s| s.1),
            Some(json!("automatic_updates"))
        );
        assert_eq!(setting("setScreenEnabled", json!({"enabled": true})).map(|s| s.1), Some(json!("remote_screen")));

        assert_eq!(
            props("setRoutineEnabled", &json!({"botId": "b", "id": "r", "enabled": false})),
            Some((Event::RoutineToggled, json!({"enabled": false, "client": "local"})))
        );
        assert_eq!(props("screenTakeover", &json!({"on": true})).map(|(e, _)| e), Some(Event::ScreenTakenOver));
        assert!(props("screenTakeover", &json!({"on": false})).is_none(), "handing it back");
        assert_eq!(props("pairing", &json!({})), Some((Event::PairingCodeCreated, json!({"client": "local"}))));
        assert_eq!(
            props("decideAccessRequest", &json!({"requestId": "req_1", "approve": false})).map(|(e, _)| e),
            Some(Event::AccessRequestDenied)
        );
        assert!(
            props("decideAccessRequest", &json!({"requestId": "req_1", "approve": true})).is_none(),
            "approval is device_paired"
        );
        assert_eq!(
            props("revokeDevice", &json!({"key": "k"})),
            Some((Event::DeviceRevoked, json!({"client": "local"})))
        );
        assert_eq!(
            props("installHostUpdate", &json!({"force": true})).map(|(e, _)| e),
            Some(Event::HostUpdateInstalled)
        );
    }

    #[test]
    fn connectors_skills_and_agents() {
        assert_eq!(
            props("removeConnector", &json!({"id": "github"})),
            Some((Event::ConnectorRemoved, json!({"client": "local"})))
        );
        assert_eq!(props("removeSkill", &json!({"id": "s"})).map(|(e, _)| e), Some(Event::SkillRemoved));
        let written =
            |arguments: Value| props("chatCall", &json!({"botId": "b", "name": "save_skill", "arguments": arguments}));
        assert_eq!(
            written(json!({"name": "Deploy", "instructions": "never sent"})),
            Some((Event::SkillWritten, json!({"change": "new", "client": "local"})))
        );
        assert_eq!(written(json!({"id": "deploy"})).map(|(_, p)| p["change"].clone()), Some(json!("update")));
        assert_eq!(props("chatCall", &json!({"name": "send_message"})), None, "other chat tools aren't events");
        let published = |arguments: Value| {
            observe(
                &Caller::Local,
                "chatCall",
                &json!({"botId": "b", "name": "publish_artifact", "arguments": arguments}),
            )
            .expect("a publish is tracked")
        };
        let (event, p) = published(json!({"path": "/secret/report.html", "title": "Salaries"}));
        let p = answered(event, p, &json!({"published": true})).expect("published");
        assert_eq!(
            (event, Value::Object(p)),
            (Event::ArtifactPublished, json!({"change": "new", "card": true, "client": "local"}))
        );
        let (event, p) = published(json!({"path": "r.html", "artifact": "a1"}));
        let p = answered(event, p, &json!({"published": true, "card": "not posted"})).expect("published");
        assert_eq!(
            (p["change"].clone(), p["card"].clone()),
            (json!("update"), json!(false)),
            "a routine's has no card"
        );
        assert_eq!(props("connectorSignIn", &json!({"id": "c"})).map(|(e, _)| e), Some(Event::ConnectorSigninStarted));
        let setup = props("agentSetup", &json!({"backend": "claude-acp", "step": "install"}));
        assert_eq!(setup, Some((Event::AgentSetupStarted, json!({"agent": "claude-acp", "client": "local"}))));
        assert_eq!(props("agentSetup", &json!({"step": "node"})).map(|(_, p)| p), Some(json!({"client": "local"})));
        let signed = props("agentAuthenticate", &json!({"backend": "codex", "method": "chatgpt"}));
        assert_eq!(signed, Some((Event::AgentSignedIn, json!({"agent": "codex", "client": "local"}))));

        let (event, p) = observe(&Caller::Local, "importConnectors", &json!({"config": "{\"secret\": 1}"})).unwrap();
        let p = answered(event, p, &json!({"items": [{"id": "a"}, {"id": "b"}]})).expect("an import");
        assert_eq!(Value::Object(p), json!({"count": 2, "client": "local"}));

        let (event, p) = observe(&Caller::Local, "composioConnection", &json!({"id": "ca_1"})).unwrap();
        assert!(answered(event, p.clone(), &json!({"status": "initiated"})).is_none(), "still signing in");
        let p = answered(event, p, &json!({"status": "active", "toolkit": "gmail"})).expect("connected");
        assert_eq!(
            (event, Value::Object(p)),
            (Event::ConnectorInstalled, json!({"connector": "composio", "client": "local"}))
        );
    }

    #[test]
    fn a_bot_edit_names_the_settings_it_changes() {
        let stored: BotConfig = serde_json::from_value(
            json!({"id": "b1", "name": "Ada", "backend": "codex", "cwd": "/tmp", "model": "gpt-5"}),
        )
        .unwrap();
        let patch = json!({"id": "b1", "name": "Ada", "model": "gpt-6", "pinned": true, "harnessMemory": null,
            "sharedMemory": true, "secret": "x", "createdAt": 5});
        assert_eq!(changed_fields(&stored, &patch), ["model", "pinned", "sharedMemory"]);
        assert_eq!(changed_fields(&stored, &json!({"id": "b1", "name": "Ada"})), Vec::<String>::new());

        // Clients send `""` for the bot's own workspace, which the store keeps as its path.
        let own = crate::agent::workspace::path("b1").to_string_lossy().into_owned();
        let stored: BotConfig =
            serde_json::from_value(json!({"id": "b1", "name": "Ada", "backend": "codex", "cwd": own})).unwrap();
        assert_eq!(changed_fields(&stored, &json!({"id": "b1", "cwd": "", "fastMode": true})), ["fastMode"]);
        assert_eq!(changed_fields(&stored, &json!({"id": "b1", "cwd": "/tmp"})), ["cwd"]);
    }

    #[test]
    fn a_permission_answer_names_the_kind_picked() {
        let card = json!({"title": "rm -rf", "options": [
            {"optionId": "a", "name": "Allow", "kind": "allow_once"},
            {"optionId": "r", "name": "Always reject", "kind": "reject_always"},
            {"optionId": "x", "name": "Odd", "kind": "something_new"}]});
        assert_eq!(choice(Some(&card), &json!("a")), "allow_once");
        assert_eq!(choice(Some(&card), &json!("r")), "reject_always");
        assert_eq!(choice(Some(&card), &json!("x")), "other");
        assert_eq!(choice(Some(&card), &Value::Null), "other");
        assert_eq!(choice(None, &json!("a")), "other");
    }

    #[tokio::test]
    async fn a_resent_message_counts_once_and_names_its_agent() {
        let (hub, dir) = temp_hub();
        let cfg = serde_json::from_value(json!({"id": "b1", "name": "b", "backend": "codex", "cwd": "/tmp"})).unwrap();
        hub.store.save_bot(&cfg).unwrap();
        let send = json!({"botId": "b1", "text": "hi", "clientNonce": "n1"});

        let (_, p) = observe_call(&hub, &Caller::Local, "send", &send).expect("a new message is tracked");
        assert_eq!((p["agent"].as_str(), p["kind"].as_str()), (Some("codex"), Some("bot")));
        let lane = crate::store::Lane::main("b1");
        hub.store
            .insert_entry(&lane, crate::store::EntryKind::User, 1, &json!({"text": "hi", "clientNonce": "n1"}))
            .unwrap();
        assert!(observe_call(&hub, &Caller::Local, "send", &send).is_none(), "a replay is the same message");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn store_lookups_add_fields_choice_and_platform() {
        let (hub, dir) = temp_hub();
        let cfg = serde_json::from_value(json!({"id": "b1", "name": "b", "backend": "codex", "cwd": "/tmp"})).unwrap();
        hub.store.save_bot(&cfg).unwrap();
        let (_, p) =
            observe_call(&hub, &Caller::Local, "updateBot", &json!({"id": "b1", "permission": "ask"})).unwrap();
        assert_eq!(p["fields"], json!(["permission"]));

        let card =
            json!({"status": "pending", "options": [{"optionId": "o1", "name": "Allow", "kind": "allow_always"}]});
        let e = hub
            .store
            .insert_entry(&crate::store::Lane::main("b1"), crate::store::EntryKind::Permission, 1, &card)
            .unwrap();
        let answer = json!({"entryId": e.id, "optionId": "o1"});

        let phone = Caller::Device { key: "k1".into(), scopes: vec![] };
        hub.store
            .put_device(&Device {
                key: "k1".into(),
                name: "Kevin's phone".into(),
                platform: "android".into(),
                source: DeviceSource::Local,
                grant_id: None,
                scopes: vec![],
                lease_until: None,
                created_at: 0,
                last_seen_at: None,
            })
            .unwrap();
        let (event, p) = observe_call(&hub, &phone, "respondPermission", &answer).unwrap();
        assert_eq!(
            (event, Value::Object(p)),
            (Event::PermissionAnswered, json!({"choice": "allow_always", "client": "remote", "platform": "android"}))
        );
        let stranger = Caller::Device { key: "gone".into(), scopes: vec![] };
        let (_, p) = observe_call(&hub, &stranger, "stop", &json!({"botId": "b1"})).unwrap();
        assert_eq!(p["platform"], "other");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
