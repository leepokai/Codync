use super::*;
use crate::remote::identity::Identity;
use crate::store::{Permission, Store};

fn hub(dir: &std::path::Path) -> Arc<Hub> {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    for (id, manage) in [("lead", true), ("dev", false)] {
        let cfg: BotConfig = serde_json::from_value(json!({"id": id, "name": id, "backend": "claude",
            "cwd": dir.to_string_lossy(), "manageTeam": manage, "connectors": ["github"], "skills": [],
            "permission": "auto", "mode": "plan"}))
        .unwrap();
        store.save_bot(&cfg).unwrap();
    }
    let hub = Hub::new(store, "manage-test".into(), Identity::load_or_create(dir).unwrap(), "test".into(), 0);
    hub.start().unwrap();
    hub
}

fn notices(hub: &Hub, bot: &str) -> Vec<Value> {
    hub.store.history(bot, i64::MAX, 20).unwrap().into_iter().map(|e| e.data["text"].clone()).collect()
}

#[tokio::test]
async fn a_manager_creates_and_changes_bots_that_record_who_made_them() {
    let dir = std::env::temp_dir().join(format!("codync-manage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let team = call(&hub, "lead", "list_team", &json!({})).await.unwrap();
    let team: Vec<_> = team["bots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["id"].clone(), b["you"].clone(), b["manageTeam"].clone()))
        .collect();
    assert_eq!(team.len(), 2);
    assert!(team.contains(&(json!("lead"), json!(true), json!(true))));
    assert!(team.contains(&(json!("dev"), json!(false), json!(false))));

    let routine =
        json!({"name": "Scan", "instruction": "Scan the repo", "triggers": [{"type": "interval", "seconds": 1800}]});
    let args = json!({"name": " Security Analyst ", "instructions": "Reviews code.", "agent": "claude",
        "avatarColor": "red", "routines": [routine]});
    let made = call(&hub, "lead", "create_bot", &args).await.unwrap();
    let id = made["bot"]["id"].as_str().unwrap();
    let cfg = hub.store.bot(id).unwrap().unwrap().config;
    assert_eq!(
        (cfg.name.as_str(), cfg.description.as_str(), cfg.avatar_color.as_str()),
        ("Security Analyst", "Reviews code.", "red")
    );
    // It manages the team like the bot that made it, and asks no less.
    assert_eq!((cfg.created_by.as_deref(), cfg.permission, cfg.manage_team), (Some("lead"), Permission::Auto, true));
    assert_eq!(cfg.mode.as_deref(), Some("plan"), "in the same mode");
    // Another agent may not have that mode: no bot is made on it.
    let other = json!({"name": "Reviewer", "agent": "codex"});
    let refused = call(&hub, "lead", "create_bot", &other).await.unwrap_err().to_string();
    assert!(refused.contains("plan mode, which Codex may not have"), "{refused}");
    // And reaches only the connectors and skills it does, not every installed one.
    assert_eq!((cfg.connectors, cfg.skills), (vec!["github".to_owned()], vec![]));
    assert_eq!(made["routines"][0]["triggerDescriptions"][0], "Every 30 minutes");

    call(&hub, "lead", "update_bot", &json!({"botId": "dev", "instructions": "Writes code.", "hidden": true}))
        .await
        .unwrap();
    let dev = hub.store.bot("dev").unwrap().unwrap().config;
    assert_eq!((dev.description.as_str(), dev.hidden), ("Writes code.", true));
    // A manager changes its own setup too.
    call(&hub, "lead", "update_bot", &json!({"botId": "lead", "name": "Boss", "model": "opus"})).await.unwrap();
    let lead = hub.store.bot("lead").unwrap().unwrap().config;
    assert_eq!((lead.name.as_str(), lead.model.as_deref(), lead.auto_name), ("Boss", Some("opus"), false));
    assert_eq!(
        notices(&hub, "lead"),
        [
            json!("Created bot Security Analyst"),
            json!("Updated dev: hidden, instructions"),
            json!("Updated lead: name, model")
        ]
    );

    // Who made a bot is the host's record: a client's save can't rewrite it.
    hub.update_bot(&json!({"id": id, "createdBy": "dev"})).unwrap();
    assert_eq!(hub.store.bot(id).unwrap().unwrap().config.created_by.as_deref(), Some("lead"));

    let refused = |bot: &'static str, name: &'static str, args: Value| {
        let hub = hub.clone();
        async move { call(&hub, bot, name, &args).await.unwrap_err().to_string() }
    };
    assert!(refused("dev", "create_bot", json!({"name": "x"})).await.contains("may not manage the team"));
    assert!(refused("lead", "create_bot", json!({"name": "x", "avatarColor": "plaid"})).await.contains("avatarColor"));
    assert!(refused("lead", "create_bot", json!({"name": "x", "folder": "projects/x"})).await.contains("absolute"));
    let no_trigger = json!({"name": "x", "routines": [{"name": "r", "instruction": "i", "triggers": []}]});
    assert!(refused("lead", "create_bot", no_trigger).await.contains("triggers"));
    // Routines are checked as `save_routine` checks them, for the bot they're for.
    let lead_routine = hub.routines.save(&hub, "lead", &routine).unwrap()["routine"]["id"].clone();
    let other = json!({"botId": "dev", "routines": [{"id": lead_routine, "name": "r", "instruction": "i",
        "triggers": [{"type": "interval", "seconds": 1800}]}]});
    assert!(refused("lead", "update_bot", other).await.contains("routine not found"));
    let no_time = json!({"botId": "dev", "routines": [{"name": "r", "instruction": "i", "timeoutSeconds": 0,
        "triggers": [{"type": "interval", "seconds": 1800}]}]});
    assert!(refused("lead", "update_bot", no_time).await.contains("timeoutSeconds"));
    // A manager's own fallbacks are the user's once they turned that off, also through update_bot.
    hub.update_bot(&json!({"id": "lead", "selfFallbacks": false})).unwrap();
    let own = json!({"botId": "lead", "fallbacks": [{"agent": "codex"}]});
    assert!(refused("lead", "update_bot", own).await.contains("turned off setting your own fallbacks"));
    // A change too long for its card to show in full never reaches the card: under a network rule's ceiling,
    // which asks whatever the bot's Approvals, and under Ask.
    let long = json!({"name": "x", "agent": "claude", "instructions": "a".repeat(MAX_INSTRUCTIONS_CHARS)});
    let lead = hub.store.bot("lead").unwrap().unwrap().config;
    assert!(hub.ceilings.approves_by_itself(&lead));
    hub.ceilings.set("lead", codync_network::Access::Auto);
    assert!(!hub.ceilings.approves_by_itself(&lead));
    assert!(refused("lead", "create_bot", long.clone()).await.contains("too long for its approval card"));
    hub.ceilings.set("lead", codync_network::Access::Allow);
    hub.update_bot(&json!({"id": "lead", "permission": "ask"})).unwrap();
    assert!(refused("lead", "create_bot", long).await.contains("too long for its approval card"));
    assert_eq!(hub.store.bots().unwrap().len(), 3, "a refused change creates nothing");
    assert_eq!(hub.routines.list("dev")["routines"], json!([]));
    // Bots that bots made stop at MAX_CREATED, so managers making managers can't multiply without end.
    for n in 1..MAX_CREATED {
        let cfg: BotConfig =
            serde_json::from_value(json!({"id": format!("made{n}"), "name": "made", "backend": "claude",
            "cwd": dir.to_string_lossy(), "createdBy": "lead"}))
            .unwrap();
        hub.store.save_bot(&cfg).unwrap();
    }
    assert!(refused("lead", "create_bot", json!({"name": "x"})).await.contains("already created"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_manager_lists_creates_and_changes_local_groups() {
    let dir = std::env::temp_dir().join(format!("codync-manage-{}-groups", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let plain: BotConfig = serde_json::from_value(json!({"id": "plain", "name": "Plain", "plain": true,
        "hidden": true, "backend": "custom", "command": "unused"}))
    .unwrap();
    let nested: BotConfig = serde_json::from_value(json!({"id": "nested", "name": "Nested", "kind": "group",
        "members": ["dev"]}))
    .unwrap();
    let thread: BotConfig = serde_json::from_value(json!({"id": "thread-1", "name": "Thread", "backend": "claude",
        "cwd": dir.to_string_lossy(), "threadOf": "dev"}))
    .unwrap();
    let old: BotConfig = serde_json::from_value(json!({"id": "old", "name": "Old", "backend": "claude",
        "cwd": dir.to_string_lossy()}))
    .unwrap();
    for cfg in [plain, nested, thread, old] {
        hub.store.save_bot(&cfg).unwrap();
    }
    hub.store.delete_bot("old").unwrap();

    let groups = call(&hub, "lead", "list_groups", &json!({})).await.unwrap();
    assert_eq!(groups["groups"][0]["name"], "Nested");
    assert_eq!(groups["groups"][0]["members"][0]["name"], "dev", "metadata only, never a chat");

    let refused = |bot: &'static str, name: &'static str, args: Value| {
        let hub = hub.clone();
        async move { call(&hub, bot, name, &args).await.unwrap_err().to_string() }
    };
    assert!(refused("dev", "list_groups", json!({})).await.contains("may not manage the team"));
    assert!(
        refused("lead", "create_group", json!({"name": "G", "members": ["nested"]}))
            .await
            .contains("group can't be a member")
    );
    assert!(
        refused("lead", "create_group", json!({"name": "G", "members": ["plain"]}))
            .await
            .contains("unknown or unavailable")
    );
    assert!(
        refused("lead", "create_group", json!({"name": "G", "members": ["thread-1"]})).await.contains("not a thread")
    );
    assert!(
        refused("lead", "create_group", json!({"name": "G", "members": ["remote@mac"]}))
            .await
            .contains("unknown or unavailable")
    );
    assert!(
        refused("lead", "create_group", json!({"name": "G", "members": ["missing"]}))
            .await
            .contains("unknown or unavailable")
    );
    assert!(
        refused("lead", "create_group", json!({"name": "G", "members": ["old"]}))
            .await
            .contains("unknown or unavailable")
    );
    assert!(
        refused("lead", "create_group", json!({"name": "G", "members": ["lead", "lead"]}))
            .await
            .contains("same bot twice")
    );
    assert!(refused("lead", "create_group", json!({"name": "G", "members": []})).await.contains("at least one"));

    let made = call(&hub, "lead", "create_group", &json!({"name": "Crew", "topic": "Ship it", "members": ["lead"]}))
        .await
        .unwrap();
    let id = made["group"]["id"].as_str().unwrap().to_owned();
    assert_eq!(made["group"]["topic"], "Ship it");
    assert_eq!(made["group"]["members"][0]["name"], "lead");
    assert_eq!(hub.store.bot(&id).unwrap().unwrap().config.created_by.as_deref(), Some("lead"));

    let changed = call(&hub, "lead", "update_group", &json!({"groupId": id, "name": "Squad", "addMembers": ["dev"]}))
        .await
        .unwrap();
    assert_eq!(changed["group"]["name"], "Squad", "a name change lands");
    assert_eq!(changed["group"]["members"][0]["id"], "lead");
    assert_eq!(changed["group"]["members"][1]["id"], "dev", "addMembers appends");
    let removed = call(&hub, "lead", "update_group", &json!({"groupId": id, "removeMembers": ["dev"]})).await.unwrap();
    assert_eq!(removed["group"]["members"].as_array().unwrap().len(), 1, "removeMembers takes one out");
    assert!(
        refused("lead", "update_group", json!({"groupId": id, "members": ["lead"]}))
            .await
            .contains("nothing to change")
    );
    assert!(
        refused("lead", "update_group", json!({"groupId": "unknown", "name": "X"})).await.contains("unknown group")
    );
    assert!(refused("lead", "update_group", json!({"groupId": "dev", "name": "X"})).await.contains("unknown group"));
    assert!(
        refused("lead", "update_group", json!({"groupId": id, "members": ["lead"], "addMembers": ["dev"]}))
            .await
            .contains("not both")
    );
    assert_eq!(
        notices(&hub, "lead"),
        [json!("Created group Crew"), json!("Updated Crew: name, members"), json!("Updated Squad: members")]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_card_lists_the_routines_before_the_instructions() {
    let fields = setup_patch(&json!({"name": "Scout", "instructions": "Watches the repo."}), &[]).unwrap();
    assert_eq!(
        describe(
            &Store::open(std::path::Path::new(":memory:")).unwrap(),
            &fields,
            &["• Scan (Every 30 minutes): Scan it".into()]
        ),
        "Name: Scout\nRoutines:\n• Scan (Every 30 minutes): Scan it\nInstructions:\nWatches the repo."
    );
}

/// #395: a manager sets a bot's model settings (effort…) the way it sets its model; a card shows them. Checking a
/// value against the agent's model list (`acp::models::check_options`) starts the agent, so the calls here set them empty
/// and the model they'd be checked against is picked by `model_after`.
#[tokio::test]
async fn a_manager_sets_a_bots_reasoning_effort() {
    let dir = std::env::temp_dir().join(format!("codync-manage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    hub.update_bot(&json!({"id": "dev", "modelOptions": {"effort": "low"}})).unwrap();
    call(&hub, "lead", "update_bot", &json!({"botId": "dev", "modelOptions": {}})).await.unwrap();
    assert!(hub.store.bot("dev").unwrap().unwrap().config.model_options.is_empty(), "back to the model's defaults");
    let number = json!({"botId": "dev", "modelOptions": {"effort": 1}});
    let refused = call(&hub, "lead", "update_bot", &number).await.unwrap_err().to_string();
    assert!(refused.contains("modelOptions must map"), "{refused}");
    let fields =
        setup_patch(&json!({"model": "opus", "modelOptions": {"effort": "high", "fast": "true"}}), &[]).unwrap();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    assert_eq!(describe(&store, &fields, &[]), "Model: opus\nModel settings: effort high, fast true");
    assert_eq!(model_after(&fields, Some("sonnet")), Some("opus"), "checked against the new model");
    let options_only = setup_patch(&json!({"modelOptions": {"effort": "high"}}), &[]).unwrap();
    assert_eq!(model_after(&options_only, Some("sonnet")), Some("sonnet"), "against the bot's own");
    let default = setup_patch(&json!({"model": "", "modelOptions": {"effort": "high"}}), &[]).unwrap();
    assert_eq!(model_after(&default, Some("sonnet")), None, "against the agent's default");
    std::fs::remove_dir_all(&dir).unwrap();
}
