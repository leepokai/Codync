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
        setting("setAssistants", json!({"enabled": true})),
        Some((Event::SettingChanged, json!("cloud_assistants"), json!(true)))
    );
    assert_eq!(
        props("networkJoin", &json!({"invite": "codync-network:…", "name": "laptop"})),
        Some((Event::NetworkJoined, json!({"via": "invite", "client": "local"})))
    );
    let saved = props("setCloudUrl", &json!({"url": "https://relay.example.com"}));
    assert_eq!(
        saved.as_ref().map(|(e, p)| (*e, p["setting"].clone(), p["on"].clone())),
        Some((Event::SettingChanged, json!("remote_access_url"), json!(true)))
    );
    assert!(!saved.unwrap().1.to_string().contains("relay.example.com"), "the URL is not a property");
    assert_eq!(setting("setCloudUrl", json!({"url": "  "})).map(|s| s.2), Some(json!(false)));

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
    assert_eq!(props("revokeDevice", &json!({"key": "k"})), Some((Event::DeviceRevoked, json!({"client": "local"}))));
    assert_eq!(props("installHostUpdate", &json!({"force": true})).map(|(e, _)| e), Some(Event::HostUpdateInstalled));
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
        observe(&Caller::Local, "chatCall", &json!({"botId": "b", "name": "publish_artifact", "arguments": arguments}))
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
    assert_eq!((p["change"].clone(), p["card"].clone()), (json!("update"), json!(false)), "a routine's has no card");
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
    let stored: BotConfig =
        serde_json::from_value(json!({"id": "b1", "name": "Ada", "backend": "codex", "cwd": "/tmp", "model": "gpt-5"}))
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
fn sections_count_without_their_names() {
    assert_eq!(
        props("renameSection", &json!({"from": "Work", "to": "Play"})),
        Some((Event::SectionRenamed, json!({"removed": false, "client": "local"})))
    );
    let removed = props("renameSection", &json!({"from": "Work", "to": null}));
    assert_eq!(removed.map(|(_, p)| p["removed"].clone()), Some(json!(true)));

    let stored: BotConfig = serde_json::from_value(
        json!({"id": "b1", "name": "Ada", "backend": "codex", "cwd": "/tmp", "section": "Work"}),
    )
    .unwrap();
    assert_eq!(changed_fields(&stored, &json!({"id": "b1", "section": null})), ["section"]);
    assert_eq!(changed_fields(&stored, &json!({"id": "b1", "section": "Work"})), Vec::<String>::new());
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
    let (_, p) = observe_call(&hub, &Caller::Local, "updateBot", &json!({"id": "b1", "permission": "auto"})).unwrap();
    assert_eq!(p["fields"], json!(["permission"]));

    let card = json!({"status": "pending", "options": [{"optionId": "o1", "name": "Allow", "kind": "allow_always"}]});
    let e =
        hub.store.insert_entry(&crate::store::Lane::main("b1"), crate::store::EntryKind::Permission, 1, &card).unwrap();
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

#[test]
fn a_failed_network_call_reports_what_it_was_and_others_nothing() {
    let failed_call = |method: &str, b: &Value| {
        let (event, p) = observe(&Caller::Local, method, b)?;
        failed(event, p).map(|(e, p)| (e, Value::Object(p)))
    };
    assert_eq!(
        failed_call("networkJoin", &json!({"invite": "codync-network:…", "name": "laptop"})),
        Some((Event::NetworkFailed, json!({"via": "invite", "client": "local", "stage": "action"})))
    );
    assert_eq!(
        failed_call("networkRemove", &json!({"computer": "desk"})),
        Some((Event::NetworkFailed, json!({"action": "remove", "client": "local", "stage": "action"})))
    );
    assert_eq!(failed_call("setRoutineEnabled", &json!({"botId": "b", "id": "r", "enabled": false})), None);
}
