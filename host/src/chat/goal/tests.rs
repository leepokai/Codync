use super::*;
use crate::usage::{LoginReading, Provider, Window};

fn hub() -> std::sync::Arc<Hub> {
    let dir = std::env::temp_dir().join(format!("codync-goal-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
    let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
    let hub = Hub::new(store, "h".into(), identity, "t".into(), 0);
    std::fs::remove_dir_all(dir).unwrap();
    hub
}

fn cfg(id: &str, backend: &str) -> BotConfig {
    BotConfig {
        id: id.into(),
        backend: backend.into(),
        ..serde_json::from_value(json!({"id": id, "name": id})).unwrap()
    }
}

fn seed(hub: &Hub) -> Goal {
    let goal = Goal {
        id: uuid::Uuid::new_v4().to_string(),
        text: "Ship notes".into(),
        max_turns: 2,
        max_minutes: 1,
        started_at: now_ms(),
        turns: 0,
        stopped: None,
    };
    save(hub, "b", &goal).unwrap();
    goal
}

#[test]
fn closing_markers_do_not_match_negation_or_quoted_instructions() {
    for text in ["All shipped.\nGOAL_COMPLETE", "目标已完成", "goal met"] {
        assert!(done(text));
    }
    for text in ["not goal complete", "The goal is not met", "> GOAL_COMPLETE", "Say goal met when done."] {
        assert!(!done(text));
    }
    for text in ["NEEDS_USER", "Choose one?", "请确认发布权限。"] {
        assert!(asks_user(text));
    }
    assert!(!asks_user("Continuing."));
}

#[test]
fn invalid_limits_are_rejected_instead_of_silently_changed() {
    assert_eq!(limit(&json!({}), "maxTurns", 20, 200).unwrap(), 20);
    for n in [json!(0), json!(-1), json!(201), json!("2"), json!(null)] {
        assert!(limit(&json!({"maxTurns": n}), "maxTurns", 20, 200).is_err());
    }
}

#[test]
fn corrupt_control_state_never_reports_a_successful_stop_or_starts_work() {
    let hub = hub();
    hub.store.kv_set(&key("b"), "not valid goal JSON").unwrap();
    assert!(stop(&hub, "b").is_err());
    assert!(begin(&hub, &cfg("b", "custom"), "old-goal").is_err());
    assert_eq!(hub.store.kv_read(&key("b")).unwrap().as_deref(), Some("not valid goal JSON"));
}

#[test]
fn stopping_and_replacing_invalidates_queued_steps() {
    let hub = hub();
    let old = seed(&hub);
    let newer = seed(&hub);
    assert!(begin(&hub, &cfg("b", "custom"), &old.id).unwrap().is_none());
    finish(&hub, "b", Some(&old.id), Stopped::Done);
    assert_eq!(get(&hub, "b").unwrap(), newer);
    user_spoke(&hub, "b").unwrap();
    assert!(begin(&hub, &cfg("b", "custom"), &newer.id).unwrap().is_none());
    assert_eq!(get(&hub, "b").unwrap().stopped, Some(Stopped::User));
}

#[test]
fn turn_and_time_limits_prevent_another_step() {
    let hub = hub();
    let goal = seed(&hub);
    assert!(begin(&hub, &cfg("b", "custom"), &goal.id).unwrap().is_some());
    assert!(begin(&hub, &cfg("b", "custom"), &goal.id).unwrap().is_some());
    assert!(begin(&hub, &cfg("b", "custom"), &goal.id).unwrap().is_none());
    assert_eq!(get(&hub, "b").unwrap().stopped, Some(Stopped::Limit));
    let mut expired = seed(&hub);
    expired.started_at -= 61_000;
    save(&hub, "b", &expired).unwrap();
    assert!(begin(&hub, &cfg("b", "custom"), &expired.id).unwrap().is_none());
}

#[test]
fn plan_usage_reserves_headroom() {
    let hub = hub();
    let goal = seed(&hub);
    hub.usage.locked().providers.push(Provider {
        id: "claude",
        name: "Claude",
        registry: None,
        source: "agent",
        updated_at: now_ms(),
        default_login_profile: None,
        login_profiles: Vec::new(),
        windows: vec![Window {
            id: "week".into(),
            label: "Week".into(),
            percent: 80.0,
            resets_at: None,
            resets_text: None,
            text: None,
        }],
    });
    assert!(begin(&hub, &cfg("b", "claude"), &goal.id).unwrap().is_none());
    assert_eq!(get(&hub, "b").unwrap().stopped, Some(Stopped::Usage));
}

#[test]
fn a_goal_checks_the_login_profile_it_will_run_on() {
    let hub = hub();
    crate::agent::login_profiles::tests::insert(&hub.store, &[("work", "claude")]);
    let goal = seed(&hub);
    let window = |percent| Window {
        id: "week".into(),
        label: "Week".into(),
        percent,
        resets_at: None,
        resets_text: None,
        text: None,
    };
    hub.usage.locked().providers.push(Provider {
        id: "claude",
        name: "Claude",
        registry: None,
        source: "agent",
        updated_at: now_ms(),
        default_login_profile: Some("Default".into()),
        login_profiles: vec![LoginReading {
            id: "work".into(),
            name: "Work".into(),
            windows: vec![window(10.0)],
            updated_at: now_ms(),
        }],
        windows: vec![window(90.0)],
    });
    let base: BotConfig = serde_json::from_value(json!({
        "id": "b",
        "name": "B",
        "backend": "custom",
        "fallbacks": [{"backend": "claude", "loginProfile": "work"}]
    }))
    .unwrap();
    let agent = crate::agent::fallback::effective(&base, 1);
    assert_eq!(agent.login_profile.as_deref(), Some("work"));
    assert!(
        begin(&hub, &agent, &goal.id).unwrap().is_some(),
        "the selected fallback's login has headroom even though the default login doesn't"
    );
}

#[test]
fn deleting_a_bot_clears_its_goal_and_blocks_late_saves() {
    let hub = hub();
    let bot = cfg("b", "custom");
    hub.store.save_bot(&bot).unwrap();
    let goal = seed(&hub);
    hub.store.delete_bot("b").unwrap();
    assert!(get(&hub, "b").is_none());
    assert!(!save(&hub, "b", &goal).unwrap());
    assert!(set(&hub, "b", &json!({"goal": "late"})).is_err());
    finish(&hub, "b", Some(&goal.id), Stopped::Done);
    assert!(begin(&hub, &bot, &goal.id).unwrap().is_none());
    assert!(get(&hub, "b").is_none());
}
