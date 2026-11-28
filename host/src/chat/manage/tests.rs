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
            json!("Updated lead: name, model"),
            // The audit: the manager's own change is on its timeline too, with what moved and how it was approved.
            json!("Changed by lead (lead) via update_bot (auto): name; model: the agent's default -> opus")
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

/// A model change keeps the bot's saved settings (`hub::patch`), so the check is made with those settings too, not
/// only with the ones the call sends: `validateOnly` can't say valid for a model that refuses them.
#[test]
fn a_model_change_is_checked_with_the_settings_it_keeps() {
    let cfg: BotConfig = serde_json::from_value(json!({"id": "a", "name": "a", "backend": "claude",
        "model": "opus", "modelOptions": {"effort": "high"}}))
    .unwrap();
    let saved = |fields: Value| options_after(fields.as_object().unwrap(), &cfg);
    let kept = json!({"effort": "high"}).as_object().cloned();
    assert_eq!(saved(json!({"model": "sonnet"})), kept, "a model change keeps them, so they are checked");
    assert_eq!(
        saved(json!({"model": "sonnet", "modelOptions": {"effort": "low"}})).and_then(|o| o.get("effort").cloned()),
        Some(json!("low")),
        "the ones the call sends stand"
    );
    assert!(saved(json!({"model": "sonnet", "modelOptions": {}})).is_some_and(|o| o.is_empty()), "an empty map clears");
    assert_eq!(saved(json!({"name": "b"})), None, "a change that touches no model checks no settings");
}

/// A manager pauses and resumes a bot; `list_team` says where each bot stands.
#[tokio::test]
async fn a_manager_pauses_and_resumes_a_bot() {
    let dir = std::env::temp_dir().join(format!("codync-manage-pause-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let paused = call(&hub, "lead", "update_bot", &json!({"botId": "dev", "paused": true})).await.unwrap();
    assert_eq!(paused["bot"]["paused"], json!(true));
    assert!(hub.store.bot("dev").unwrap().unwrap().config.paused);
    let team = call(&hub, "lead", "list_team", &json!({})).await.unwrap();
    let dev = team["bots"].as_array().unwrap().iter().find(|b| b["id"] == "dev").unwrap();
    assert_eq!(dev["paused"], json!(true));
    assert_eq!(notices(&hub, "lead").last(), Some(&json!("Updated dev: paused")));
    // Only true and false are accepted.
    let bad =
        call(&hub, "lead", "update_bot", &json!({"botId": "dev", "paused": "yes"})).await.unwrap_err().to_string();
    assert!(bad.contains("paused must be true or false"), "{bad}");
    let resumed = call(&hub, "lead", "update_bot", &json!({"botId": "dev", "paused": false})).await.unwrap();
    assert_eq!(resumed["bot"]["paused"], json!(false));
    assert!(!hub.store.bot("dev").unwrap().unwrap().config.paused);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `list_models` says what went wrong for an agent this computer doesn't have, and wants its agent named. Checking
/// ids against a real catalog starts the agent, so like the `modelOptions` checks this is exercised in e2e only.
#[tokio::test]
async fn list_models_wants_its_agent_and_says_what_went_wrong() {
    let dir = std::env::temp_dir().join(format!("codync-manage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let missing = call(&hub, "lead", "list_models", &json!({"agent": "no-such-agent"})).await.unwrap_err().to_string();
    assert!(missing.contains("couldn't list no-such-agent's models"), "{missing}");
    let none = call(&hub, "lead", "list_models", &json!({})).await.unwrap_err().to_string();
    assert!(none.contains("pass agent, or botId for a managed bot's agent"), "{none}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The manage-the-team gate fronts every manage tool, so pausing a bot and changing its model or fallbacks are
/// out of reach for a bot the user has not granted it; nothing is written.
#[tokio::test]
async fn a_bot_without_the_option_cannot_pause_or_remodel() {
    let dir = std::env::temp_dir().join(format!("codync-manage-pause-gate-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let rev_before = hub.store.bot("lead").unwrap().unwrap().rev;
    for args in [
        json!({"botId": "lead", "paused": true}),
        json!({"botId": "lead", "model": "sonnet"}),
        json!({"botId": "lead", "fallbacks": [{"agent": "claude"}]}),
    ] {
        let err = call(&hub, "dev", "update_bot", &args).await.unwrap_err().to_string();
        assert!(err.contains("may not manage the team"), "{args}: {err}");
    }
    let models = call(&hub, "dev", "list_models", &json!({"agent": "claude"})).await.unwrap_err().to_string();
    assert!(models.contains("may not manage the team"), "{models}");
    let lead = hub.store.bot("lead").unwrap().unwrap();
    assert!(!lead.config.paused, "nothing changed");
    assert_eq!(lead.config.model, None, "nothing changed");
    assert!(lead.config.fallbacks.is_empty(), "nothing changed");
    assert_eq!(lead.rev, rev_before, "a refused change writes no revision");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A partial `update_bot` is a patch: what it names changes, what it leaves out keeps its saved value. The
/// audit lands on the target's own timeline, and a `validateOnly` check answers without applying anything.
#[tokio::test]
async fn a_partial_update_preserves_and_a_validate_only_check_changes_nothing() {
    let dir = std::env::temp_dir().join(format!("codync-manage-partial-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    // Pause the bot, then touch a field the patch doesn't share with the pause.
    call(&hub, "lead", "update_bot", &json!({"botId": "dev", "paused": true})).await.unwrap();
    let renamed = call(&hub, "lead", "update_bot", &json!({"botId": "dev", "name": "Dev Renamed"})).await.unwrap();
    assert_eq!(renamed["bot"]["name"], json!("Dev Renamed"));
    assert_eq!(renamed["bot"]["paused"], json!(true), "a name change does not resume the bot");
    // validateOnly answers with the resolved backend and writes nothing: no card, no new revision.
    let before = hub.store.bot("dev").unwrap().unwrap();
    let checked = call(&hub, "lead", "update_bot", &json!({"botId": "dev", "paused": false, "validateOnly": true}))
        .await
        .unwrap();
    assert_eq!(checked["valid"], json!(true));
    assert_eq!(checked["resolvedBackend"], json!("claude"));
    assert_eq!(checked["applied"], json!(false));
    let after = hub.store.bot("dev").unwrap().unwrap();
    assert_eq!(after.rev, before.rev, "a validateOnly check writes nothing");
    assert!(after.config.paused, "the bot is still paused after the check");
    // An invalid value is still an invalid value, alias or not.
    let dry = call(&hub, "lead", "update_bot", &json!({"botId": "dev", "paused": "maybe", "dryRun": true})).await;
    assert!(dry.is_err(), "{dry:?}");
    // The audit trail: the target's timeline says who changed what and how it was approved.
    let timeline: Vec<String> = notices(&hub, "dev").iter().filter_map(|n| n.as_str().map(str::to_owned)).collect();
    assert!(timeline.iter().any(|n| n.contains("Changed by lead (lead) via update_bot")), "{timeline:?}");
    assert!(timeline.iter().any(|n| n.contains("paused: false -> true") && n.contains("(auto)")), "{timeline:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A dry run makes the checks a save makes, so it can't report a change valid that the host would refuse: the lead bot
/// can't be hidden. It writes nothing either way.
#[tokio::test]
async fn a_validate_only_check_refuses_hiding_the_lead() {
    let dir = std::env::temp_dir().join(format!("codync-manage-lead-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    // "lead" is the computer's lead only once its flag says so.
    let mut lead = hub.store.bot("lead").unwrap().unwrap().config;
    lead.lead = true;
    hub.store.save_bot(&lead).unwrap();
    let rev = hub.store.bot("lead").unwrap().unwrap().rev;
    for check in ["validateOnly", "dryRun"] {
        let mut args = json!({"botId": "lead", "hidden": true});
        args[check] = json!(true);
        let checked = call(&hub, "lead", "update_bot", &args).await.unwrap();
        assert_eq!(checked["valid"], json!(false), "{check}");
        assert_eq!(checked["applied"], json!(false), "{check}");
        let reason = checked["reason"].as_str().unwrap();
        assert!(reason.contains("the lead bot can't be hidden"), "{check}: {reason}");
        let stored = hub.store.bot("lead").unwrap().unwrap();
        assert!(!stored.config.hidden, "{check} writes nothing");
        assert_eq!(stored.rev, rev, "{check} writes no revision");
    }
    // The same change for real is refused with that reason, and the bot is still not hidden.
    let refused =
        call(&hub, "lead", "update_bot", &json!({"botId": "lead", "hidden": true})).await.unwrap_err().to_string();
    assert!(refused.contains("the lead bot can't be hidden"), "{refused}");
    assert!(!hub.store.bot("lead").unwrap().unwrap().config.hidden);
    // A check for another bot passes, and still writes nothing.
    let ok =
        call(&hub, "lead", "update_bot", &json!({"botId": "dev", "hidden": true, "validateOnly": true})).await.unwrap();
    assert_eq!((ok["valid"].clone(), ok["applied"].clone()), (json!(true), json!(false)));
    assert!(!hub.store.bot("dev").unwrap().unwrap().config.hidden, "the check does not write");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A manager's model and fallback change keeps the strings on the target's timeline and out of analytics.
/// The event names the fields that moved. It carries no bot name and no model string.
#[test]
fn a_config_change_keeps_names_and_private_models_off_the_analytics_event() {
    let dir = std::env::temp_dir().join(format!("codync-manage-audit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let identity = Identity::load_or_create(&dir).unwrap();
    let hub = Hub::new(store, "audit-test".into(), identity, "test".into(), 0);
    let source: BotConfig = serde_json::from_value(json!({
        "id": "lead", "name": "NightingalePrivate", "backend": "claude", "permission": "auto"
    }))
    .unwrap();
    let before: BotConfig = serde_json::from_value(json!({
        "id": "dev",
        "name": "FoldPrivateTarget",
        "backend": "custom",
        "command": "my-secret-command",
        "model": "secret-fold-model-9f3a",
        "permission": "auto",
        "fallbacks": [{"backend": "claude", "model": "private-fallback-model-7c2e"}]
    }))
    .unwrap();
    let mut after = before.clone();
    after.model = Some("another-secret-model-bb01".into());
    after.fallbacks[0].model = Some("another-private-fallback-aa02".into());
    let mut fields = Map::new();
    fields.insert("model".into(), after.model.clone().into());
    fields.insert("fallbacks".into(), json!(after.fallbacks));
    audit_change(&hub, &source, &before, &after, &fields, 0);

    let notes = notices(&hub, "dev");
    let timeline = notes.iter().filter_map(|n| n.as_str()).collect::<Vec<_>>().join("\n");
    assert!(timeline.contains("secret-fold-model-9f3a"), "the timeline keeps the old model: {timeline}");
    assert!(timeline.contains("another-secret-model-bb01"), "the timeline keeps the new model: {timeline}");
    assert!(timeline.contains("private-fallback-model-7c2e"), "the timeline keeps the old fallback: {timeline}");
    assert!(timeline.contains("another-private-fallback-aa02"), "the timeline keeps the new fallback: {timeline}");
    assert!(timeline.contains("NightingalePrivate"), "the timeline names who changed it: {timeline}");

    let sent = crate::analytics::queued_for_test(&hub);
    let event = sent.iter().find(|e| e["event"] == "bot_config_changed").expect("the change is captured");
    let raw = serde_json::to_string(event).unwrap();
    let private_bits = [
        "NightingalePrivate",
        "FoldPrivateTarget",
        "secret-fold-model-9f3a",
        "another-secret-model-bb01",
        "private-fallback-model-7c2e",
        "another-private-fallback-aa02",
        "my-secret-command",
    ];
    for bit in private_bits {
        assert!(!raw.contains(bit), "analytics carried {bit}: {raw}");
    }
    let fields = event["properties"]["fields"].as_array().expect("fields");
    assert!(fields.iter().any(|f| f == "model") && fields.iter().any(|f| f == "fallbacks"), "{raw}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A routines-only change is a change like any other: the target's own timeline records who made it and what
/// moved. Only the config fields used to reach the audit, so a manager that changed nothing but routines left the
/// audit empty: no line naming it, and no `bot_config_changed` behind the generic notice.
#[tokio::test]
async fn a_manager_routine_change_reaches_the_targets_audit() {
    let dir = std::env::temp_dir().join(format!("codync-manage-routines-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let routine =
        json!({"name": "Scan", "instruction": "Scan the repo", "triggers": [{"type": "interval", "seconds": 1800}]});
    call(&hub, "lead", "update_bot", &json!({"botId": "dev", "routines": [routine]})).await.unwrap();

    assert_eq!(hub.routines.list("dev")["routines"].as_array().unwrap().len(), 1, "the routine was saved");
    let timeline = notices(&hub, "dev").iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n");
    assert!(
        timeline.contains("Changed by lead (lead) via update_bot"),
        "the target's timeline names the manager and the change: {timeline}"
    );
    assert!(timeline.contains("routines"), "the timeline says what moved: {timeline}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// **Manage the team** can be on for any bot, and a manager can create another, so `update_bot` is not only the
/// lead's: the audit records the role the acting bot's own config gives it. The lead is a lead; a bot the lead made
/// is a manager, not a second lead.
#[test]
fn a_change_by_a_manager_that_is_not_the_lead_is_not_recorded_as_one() {
    let dir = std::env::temp_dir().join(format!("codync-manage-actor-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let identity = Identity::load_or_create(&dir).unwrap();
    let hub = Hub::new(store, "audit-test".into(), identity, "test".into(), 0);
    let lead: BotConfig = serde_json::from_value(json!({
        "id": "lead", "name": "Lead", "backend": "claude", "permission": "auto", "lead": true
    }))
    .unwrap();
    let made: BotConfig = serde_json::from_value(json!({
        "id": "made-by-lead", "name": "Owen", "backend": "claude", "permission": "auto", "manageTeam": true
    }))
    .unwrap();
    let before: BotConfig = serde_json::from_value(json!({
        "id": "dev", "name": "Dev", "backend": "claude", "permission": "auto"
    }))
    .unwrap();
    let mut after = before.clone();
    after.paused = true;
    let mut changed = Map::new();
    changed.insert("paused".into(), json!(true));
    audit_change(&hub, &lead, &before, &after, &changed, 0);
    audit_change(&hub, &made, &before, &after, &changed, 0);

    let sent = crate::analytics::queued_for_test(&hub);
    let kinds: Vec<&str> = sent
        .iter()
        .filter(|e| e["event"] == "bot_config_changed")
        .map(|e| e["properties"]["actorKind"].as_str().unwrap_or("missing"))
        .collect();
    assert_eq!(kinds, ["lead", "manager"], "the lead is a lead; a manager it made is not");
    std::fs::remove_dir_all(&dir).unwrap();
}
