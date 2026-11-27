use super::*;
use crate::remote::identity::Identity;
use crate::store::{Permission, Store};
use std::sync::atomic::{AtomicBool, Ordering};

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

#[test]
fn delete_bot_is_a_destructive_tool_with_an_explicit_card() {
    let tools = tools();
    let delete = tools.as_array().unwrap().iter().find(|t| t["name"] == "delete_bot").unwrap();
    assert_eq!(delete["annotations"]["readOnlyHint"], false);
    assert_eq!(delete["annotations"]["destructiveHint"], true);
    assert_eq!(delete["inputSchema"]["required"], json!(["botId"]));
    assert_eq!(delete["inputSchema"]["properties"].as_object().unwrap().len(), 1);
    assert!(!INSTRUCTIONS.contains("Deleting bots is the user's"));
    assert!(INSTRUCTIONS.contains("every delete needs the user's Allow on an approval card"));
}

#[tokio::test]
async fn a_manager_refuses_unsafe_delete_targets_before_any_card() {
    let dir = std::env::temp_dir().join(format!("codync-manage-delete-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let bot = |cfg: Value| {
        let id = cfg["id"].as_str().unwrap().to_owned();
        hub.store.save_bot(&serde_json::from_value::<BotConfig>(cfg).unwrap()).unwrap();
        id
    };
    let group = bot(
        json!({"id": "group", "name": "Group", "kind": "group", "backend": "claude", "cwd": dir.to_string_lossy()}),
    );
    let thread = bot(
        json!({"id": "thread-1", "name": "Thread", "backend": "claude", "cwd": dir.to_string_lossy(), "threadOf": "lead"}),
    );
    let plain =
        bot(json!({"id": "plain", "name": "Box", "backend": "claude", "cwd": dir.to_string_lossy(), "plain": true}));
    let lead = bot(
        json!({"id": "other-lead", "name": "Lead", "backend": "claude", "cwd": dir.to_string_lossy(), "lead": true}),
    );

    let refused = |actor: &'static str, args: Value| {
        let hub = hub.clone();
        async move { call(&hub, actor, "delete_bot", &args).await.unwrap_err().to_string() }
    };
    assert!(refused("dev", json!({"botId": "dev"})).await.contains("may not manage the team"));
    assert!(refused("lead", json!({})).await.contains("botId is required"));
    assert!(refused("lead", json!({"botId": "gone"})).await.contains("unknown bot"));
    assert!(refused("lead", json!({"botId": "lead"})).await.contains("delete yourself"));
    assert!(refused("lead", json!({"botId": group})).await.contains("unknown bot"));
    assert!(refused("lead", json!({"botId": thread})).await.contains("thread isn't a bot"));
    assert!(refused("lead", json!({"botId": plain})).await.contains("box for threads without a bot"));
    assert!(refused("lead", json!({"botId": lead})).await.contains("current lead can't be deleted"));
    hub.store.delete_bot("dev").unwrap();
    assert!(refused("lead", json!({"botId": "dev"})).await.contains("unknown bot"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn delete_bot_stays_under_a_network_ceiling() {
    let dir = std::env::temp_dir().join(format!("codync-manage-ceiling-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    hub.ceilings.set("lead", codync_network::Access::Ask);
    let refused = hub.ceilings.check_tool("lead", "delete_bot").unwrap_err().to_string();
    assert!(refused.contains("delete_bot isn't available in this turn"), "{refused}");
    hub.ceilings.set("lead", codync_network::Access::Allow);
    assert!(hub.ceilings.check_tool("lead", "delete_bot").is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The user can pick the target as lead while a manager's delete card is up. Allowing the card then
/// must not delete the new lead: `delete_bot_if` rechecks lead under the same `emit_lock` `make_lead`
/// uses, right before the store delete.
#[tokio::test]
async fn a_target_made_lead_while_the_card_waits_is_not_deleted() {
    let dir = std::env::temp_dir().join(format!("codync-manage-lead-race-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    // Stand in for the manager's actor: the approval card command arrives here instead of driving a real turn.
    let mut commands = hub.attach_bot_for_test("lead");
    let call_hub = hub.clone();
    let task = tokio::spawn(async move { call(&call_hub, "lead", "delete_bot", &json!({"botId": "dev"})).await });

    let reply = loop {
        let cmd = tokio::time::timeout(Duration::from_secs(5), commands.recv()).await.unwrap().unwrap();
        if let Cmd::Approve { title, reply, .. } = cmd {
            assert!(title.contains("Delete bot dev"), "{title}");
            break reply;
        }
    };
    // The user makes the deletion target the lead while the card is still unanswered.
    crate::chat::lead::set(&hub, "dev").unwrap();
    assert!(hub.store.bot("dev").unwrap().unwrap().config.lead);
    reply.send(true).unwrap();

    let err = task.await.unwrap().unwrap_err().to_string();
    assert!(err.contains("current lead can't be deleted"), "{err}");
    let row = hub.store.bot("dev").unwrap().unwrap();
    assert!(!row.deleted && row.config.lead, "the new lead is still here");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A finished thread cleared while its owner is being deleted must not abort the rest of the cleanup:
/// the owner and its other threads all go, even though one thread vanished mid-delete.
#[tokio::test]
async fn a_thread_cleared_mid_delete_does_not_abort_the_owner_cleanup() {
    let dir = std::env::temp_dir().join(format!("codync-manage-delete-thread-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let save = |id: &str, thread_of: Option<&str>| {
        let mut v = json!({"id": id, "name": id, "backend": "claude", "cwd": dir.to_string_lossy()});
        if let Some(owner) = thread_of {
            v["threadOf"] = json!(owner);
        }
        hub.store.save_bot(&serde_json::from_value::<BotConfig>(v).unwrap()).unwrap();
    };
    save("parent", None);
    save("thread-a", Some("parent"));
    save("thread-b", Some("parent"));

    // The check runs under emit_lock before the thread recursion: clearing one thread here is the
    // same as a concurrent cleanup between the listing and the recursion.
    let cleared = hub.clone();
    hub.delete_bot_if("parent", |_| {
        cleared.store.delete_bot("thread-a")?;
        Ok(())
    })
    .unwrap();

    assert!(hub.store.bot("parent").unwrap().unwrap().deleted, "the owner was deleted");
    assert!(hub.store.bot("thread-a").unwrap().unwrap().deleted, "the cleared thread is gone");
    assert!(hub.store.bot("thread-b").unwrap().unwrap().deleted, "the other thread still gets cleaned");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Two bots with the same name show their ids on the delete card, so the user can tell which one
/// Allow will remove.
#[tokio::test]
async fn a_delete_card_names_the_bot_id_so_twins_can_be_told_apart() {
    let dir = std::env::temp_dir().join(format!("codync-manage-delete-twins-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    for id in ["twin-a", "twin-b"] {
        let cfg: BotConfig = serde_json::from_value(json!({
            "id": id, "name": "Twin", "backend": "claude", "cwd": dir.to_string_lossy()
        }))
        .unwrap();
        hub.store.save_bot(&cfg).unwrap();
    }

    let mut commands = hub.attach_bot_for_test("lead");
    for id in ["twin-a", "twin-b"] {
        let call_hub = hub.clone();
        let target = id.to_owned();
        let task = tokio::spawn(async move { call(&call_hub, "lead", "delete_bot", &json!({"botId": target})).await });
        let reply = loop {
            let cmd = tokio::time::timeout(Duration::from_secs(5), commands.recv()).await.unwrap().unwrap();
            if let Cmd::Approve { detail, reply, .. } = cmd {
                assert!(detail.contains(id), "{detail}");
                assert!(detail.contains("Name: Twin"), "{detail}");
                break reply;
            }
        };
        // Deny, so both twins remain for the next card.
        reply.send(false).unwrap();
        assert!(task.await.unwrap().is_err(), "a deny leaves the bot alone");
    }
    assert!(!hub.store.bot("twin-a").unwrap().unwrap().deleted);
    assert!(!hub.store.bot("twin-b").unwrap().unwrap().deleted);
    drop(commands);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The delete marks its target under `emit_lock` before the unlocked cleanup. A lead pick that
/// lands in that window must be refused: a lead in the store with its runtime already torn down is
/// worse than a refused pick.
#[tokio::test]
async fn a_lead_pick_during_the_cleanup_window_is_refused() {
    let dir = std::env::temp_dir().join(format!("codync-manage-delete-lead-window-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    let (entered_tx, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
    let signalled = AtomicBool::new(false);
    let delete_hub = hub.clone();
    let task = tokio::task::spawn_blocking(move || {
        delete_hub.delete_bot_if("dev", |_| {
            // First check (under emit_lock): the target is about to be marked. Wake the picker;
            // it blocks on emit_lock until the mark is in place, then runs in the cleanup window.
            if !signalled.swap(true, Ordering::SeqCst) {
                let _ = entered_tx.send(());
            }
            Ok(())
        })
    });
    entered_rx.recv().await.unwrap();

    // Whoever wins `emit_lock` after the signal, the lead pick can never succeed: it either sees the
    // delete mark and is refused, or the delete has already finished and the bot is gone. Assert the
    // invariant, not which error string the race produced.
    let err = crate::chat::lead::set(&hub, "dev").unwrap_err().to_string();
    assert!(err.contains("being deleted") || err.contains("unknown bot"), "the lead pick was refused: {err}");
    assert!(!hub.store.bot("dev").unwrap().unwrap().config.lead, "no half-promoted lead in the store");

    task.await.unwrap().unwrap();
    let row = hub.store.bot("dev").unwrap().unwrap();
    assert!(row.deleted && !row.config.lead, "the delete still completed cleanly");
    assert!(!hub.is_deleting("dev"), "the mark is cleared after the delete");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Deterministic check of the guard itself: a marked bot is refused as lead, and is selectable again
/// once the mark clears. No delete thread, so no race over which error the pick gets.
#[tokio::test]
async fn a_marked_bot_cannot_be_made_lead_then_can_again() {
    let dir = std::env::temp_dir().join(format!("codync-manage-lead-mark-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let hub = hub(&dir);
    hub.mark_deleting_for_test("dev", true);
    let err = crate::chat::lead::set(&hub, "dev").unwrap_err().to_string();
    assert!(err.contains("being deleted"), "{err}");
    assert!(!hub.store.bot("dev").unwrap().unwrap().config.lead);

    hub.mark_deleting_for_test("dev", false);
    crate::chat::lead::set(&hub, "dev").unwrap();
    assert!(hub.store.bot("dev").unwrap().unwrap().config.lead, "pickable once the mark clears");
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
