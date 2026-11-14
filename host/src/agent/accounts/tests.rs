use super::*;
use std::path::Path;

/// Adds signed-out accounts `(id, agent)` to `store`.
pub(crate) fn insert(store: &Store, accounts: &[(&str, &str)]) {
    change(store, |all| {
        all.extend(accounts.iter().map(|(id, backend)| Account {
            id: (*id).to_owned(),
            agent: (*backend).to_owned(),
            label: None,
            email: None,
            number: 0,
            signed_in: false,
        }));
        Ok(())
    })
    .expect("saved");
}

fn store_with(accounts: &[(&str, &str)]) -> Store {
    let store = Store::open(Path::new(":memory:")).expect("store");
    insert(&store, accounts);
    store
}

fn bot(backend: &str, account: Option<&str>) -> BotConfig {
    serde_json::from_value(json!({"id": "b", "name": "B", "backend": backend, "account": account})).expect("bot")
}

#[test]
fn an_account_points_its_own_agent_at_its_folder_and_no_other() {
    let store = store_with(&[("a1", "claude")]);
    let env = env(&store, "claude", Some("a1")).expect("env");
    let dir = crate::service::data_dir().join("agent-accounts").join("claude").join("a1");
    assert_eq!(env[0], ("CLAUDE_CONFIG_DIR".to_owned(), dir.to_string_lossy().into_owned()));
    assert_eq!(env[1], (ACCOUNT_VAR.to_owned(), "a1".to_owned()));
    // The registry id of the same agent is the same agent.
    assert_eq!(super::env(&store, "claude-acp", Some("a1")).expect("env").len(), 2);
    assert_eq!(super::env(&store, "claude", None).expect("own login"), Vec::new());
    assert_eq!(super::env(&store, "codex", Some("a1")).expect("a fallback's agent"), Vec::new());
    assert!(super::env(&store, "claude", Some("gone")).is_err(), "never the wrong login in silence");
    let on_account = bot("claude", Some("a1"));
    assert_eq!(of_bot(&store, "claude", &on_account), Some("a1"));
    assert_eq!(of_bot(&store, "codex", &on_account), None, "on a fallback it's the computer's own login");
}

#[test]
fn an_account_counts_as_running_for_as_long_as_a_process_holds_it() {
    let running = |id: &str| RUNNING.locked().iter().filter(|r| *r == id).count();
    let (first, second) = (Running::on("held"), Running::on("held"));
    drop(first);
    assert_eq!(running("held"), 1, "another process is still on it");
    drop(second);
    assert_eq!(running("held"), 0);
}

#[test]
fn an_id_that_would_leave_the_accounts_folder_is_refused() {
    let store = store_with(&[("../../x", "claude"), ("", "codex")]);
    assert!(env(&store, "claude", Some("../../x")).is_err());
    assert!(env(&store, "codex", Some("")).is_err());
}

#[test]
fn a_bot_keeps_only_an_account_of_its_own_agent() {
    let store = store_with(&[("a1", "claude")]);
    let mut own = bot("claude", Some("a1"));
    settle(&store, &mut own).expect("settled");
    assert_eq!(own.account.as_deref(), Some("a1"));
    let mut moved = bot("codex", Some("a1"));
    settle(&store, &mut moved).expect("settled");
    assert_eq!(moved.account, None, "moved to another agent");
    let mut custom = bot("claude", Some("a1"));
    custom.command = Some("my-claude --acp".into());
    settle(&store, &mut custom).expect("settled");
    assert_eq!(custom.account, None, "its own command decides its login");
    assert!(settle(&store, &mut bot("claude", Some("gone"))).is_err());
}

#[test]
fn agents_that_can_have_accounts_list_theirs_by_who_they_sign_in() {
    let store = store_with(&[("a1", "claude"), ("a2", "claude")]);
    change(&store, |all| {
        all[0].email = Some("work@example.com".into());
        all[0].signed_in = true;
        Ok(())
    })
    .expect("signed in");
    let mut agents = vec![json!({"id": "claude"}), json!({"id": "codex"}), json!({"id": "cursor"})];
    annotate(&store, &mut agents);
    assert_eq!(
        agents[0]["accounts"],
        json!([
            {"id": "a1", "name": "work@example.com", "label": null, "email": "work@example.com", "signedIn": true},
            {"id": "a2", "name": null, "label": null, "email": null, "signedIn": false},
        ])
    );
    assert_eq!(agents[1]["accounts"], json!([]), "none yet, but it can have them");
    assert!(agents[2].get("accounts").is_none());
}

#[test]
fn the_user_names_an_account_and_the_computers_own_login() {
    let store = store_with(&[("a1", "claude")]);
    rename(&store, "claude", Some("a1"), "  Work ").expect("named");
    rename(&store, "claude-acp", None, "Personal").expect("the computer's own, by its registry id too");
    let mut agents = vec![json!({"id": "claude"}), json!({"id": "codex"})];
    annotate(&store, &mut agents);
    assert_eq!(agents[0]["accounts"][0]["name"], "Work");
    assert_eq!(agents[0]["ownAccount"]["label"], "Personal");
    assert_eq!(agents[0]["accountSignIn"], "terminal");
    assert_eq!(agents[1]["ownAccount"]["label"], Value::Null, "another agent's own login");
    rename(&store, "claude", Some("a1"), "").expect("an empty name clears it");
    rename(&store, "claude", None, " ").expect("cleared");
    annotate(&store, &mut agents);
    assert_eq!((&agents[0]["accounts"][0]["name"], &agents[0]["ownAccount"]["label"]), (&Value::Null, &Value::Null));
    assert!(rename(&store, "cursor", None, "x").is_err(), "an agent with one login");
    assert!(rename(&store, "claude", Some("gone"), "x").is_err());
}

#[test]
fn a_fallback_keeps_only_an_account_of_its_own_agent() {
    let store = store_with(&[("a1", "claude"), ("c1", "codex")]);
    let mut cfg: BotConfig = serde_json::from_value(json!({"id": "b", "name": "B", "backend": "claude", "fallbacks": [
        {"backend": "claude", "account": "a1"},
        {"backend": "codex", "account": "a1"},
        {"backend": "codex", "account": "c1", "command": "my-codex --acp"},
    ]}))
    .expect("bot");
    settle(&store, &mut cfg).expect("settled");
    let accounts: Vec<_> = cfg.fallbacks.iter().map(|f| f.account.as_deref()).collect();
    assert_eq!(accounts, [Some("a1"), None, None], "another agent's account and a custom command's go");
    cfg.fallbacks[0].account = Some("gone".into());
    assert!(settle(&store, &mut cfg).is_err());
}

#[test]
fn an_account_is_in_use_by_a_bot_on_it_and_by_one_that_falls_back_to_it() {
    let store = store_with(&[("a1", "claude"), ("a2", "claude")]);
    let save = |cfg: Value| store.save_bot(&serde_json::from_value(cfg).expect("bot")).expect("saved");
    save(json!({"id": "own", "name": "Own", "backend": "claude", "account": "a1"}));
    save(
        json!({"id": "step", "name": "Step", "backend": "claude", "fallbacks": [{"backend": "claude", "account": "a2"}]}),
    );
    assert_eq!(users(&store, "a1").expect("bots"), ["Own"]);
    assert_eq!(users(&store, "a2").expect("bots"), ["Step"], "removing it would leave a fallback no save accepts");
    assert_eq!(users(&store, "gone").expect("bots"), Vec::<String>::new());
}
