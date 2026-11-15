use super::*;
use std::path::Path;

/// Adds signed-out account profiles `(id, agent)` to `store`.
pub(crate) fn insert(store: &Store, accounts: &[(&str, &str)]) {
    change(store, |all| {
        all.extend(accounts.iter().map(|(id, backend)| Profile {
            account: Account {
                id: (*id).to_owned(),
                agent: (*backend).to_owned(),
                label: None,
                email: None,
                number: 0,
                signed_in: false,
            },
            kind: Kind::Account,
            vars: Vec::new(),
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
    serde_json::from_value(json!({"id": "b", "name": "B", "backend": backend, "loginProfile": account})).expect("bot")
}

#[test]
fn an_account_points_its_own_agent_at_its_folder_and_no_other() {
    let store = store_with(&[("a1", "claude")]);
    let env = env(&store, "claude", Some("a1")).expect("env").0;
    let dir = crate::service::data_dir().join("login-profiles").join("claude").join("a1");
    assert_eq!(env[0], ("CLAUDE_CONFIG_DIR".to_owned(), dir.to_string_lossy().into_owned()));
    assert_eq!(env[1], (PROFILE_VAR.to_owned(), "a1".to_owned()));
    // The registry id of the same agent is the same agent.
    assert_eq!(super::env(&store, "claude-acp", Some("a1")).expect("env").0, env);
    // A login key the host itself was started with is taken away from the process (`shell::apply_env`).
    let taken: Vec<&str> =
        env[2..].iter().filter(|(_, value)| value.is_empty()).map(|(name, _)| name.as_str()).collect();
    assert_eq!(taken, login_keys("claude"));
    assert_eq!(super::env(&store, "claude", None).expect("own login").0, Vec::new());
    assert_eq!(super::env(&store, "codex", Some("a1")).expect("a fallback's agent").0, Vec::new());
    assert!(super::env(&store, "claude", Some("gone")).is_err(), "never the wrong login in silence");
    let on_profile = bot("claude", Some("a1"));
    assert_eq!(of_bot(&store, "claude", &on_profile), Some("a1"));
    assert_eq!(of_bot(&store, "codex", &on_profile), None, "on a fallback it's the computer's own login");
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
fn a_profile_being_removed_is_gone_for_a_bot_that_would_be_set_to_it() {
    let store = store_with(&[("leaving", "claude")]);
    let removing = Removing::on("leaving");
    assert!(settle(&store, &mut bot("claude", Some("leaving"))).is_err());
    drop(removing);
    settle(&store, &mut bot("claude", Some("leaving"))).expect("a removal that was refused leaves it usable");
}

#[tokio::test]
async fn a_refused_removal_leaves_the_profile_usable() {
    let dir = std::env::temp_dir().join(format!("codync-login-profiles-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("folder");
    let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("identity");
    let hub = Hub::new(store_with(&[("kept", "claude")]), "login-profiles-test".into(), identity, "test".into(), 0);
    let category = |refused: anyhow::Error| refused.downcast_ref::<Refused>().map(|refused| refused.category);

    // An agent still runs on it: refused once it's marked, and the mark goes with the refusal.
    let running = Running::on("kept");
    assert_eq!(category(remove(&hub, "kept").await.expect_err("in use")), Some("in_use"));
    drop(running);
    settle(&hub.store, &mut bot("claude", Some("kept"))).expect("still there for a bot");

    // A removal already under way: a second one finds it gone, and leaves the first one's mark.
    let removing = Removing::on("kept");
    assert!(remove(&hub, "kept").await.is_err());
    assert!(settle(&hub.store, &mut bot("claude", Some("kept"))).is_err(), "the first removal still holds it");
    drop(removing);

    // A bot set to it: refused, and it stays.
    let mut cfg = bot("claude", Some("kept"));
    cfg.cwd = dir.to_string_lossy().into_owned();
    hub.store.save_bot(&cfg).expect("saved");
    assert_eq!(category(remove(&hub, "kept").await.expect_err("a bot uses it")), Some("in_use"));
    find(&hub.store, "kept").expect("not removed");
}

#[tokio::test]
async fn an_api_key_profile_takes_the_agents_other_login_keys_away() {
    let store = unlocked(&[]);
    let added = add(&store, "claude", Kind::ApiKey).await.expect("added");
    let id = added["id"].as_str().expect("id");
    let value = |vars: &[(String, String)], name: &str| -> Vec<String> {
        vars.iter().filter(|(key, _)| key == name).map(|(_, value)| value.clone()).collect()
    };

    let typed = json!({"MADE_UP_KEY": "x"});
    keys::set(&store, id, typed.as_object().expect("vars"), &[]).await.expect("saved");
    let vars = env(&store, "claude", Some(id)).expect("env").0;
    assert_ne!(login_keys("claude"), &[] as &[&str]);
    for key in login_keys("claude") {
        assert_eq!(value(&vars, key), [""], "{key}: not the host's own");
    }

    // A login key the profile sets itself keeps its value, once.
    let own = login_keys("claude")[0];
    let typed = json!({own: "made-up"});
    keys::set(&store, id, typed.as_object().expect("vars"), &[]).await.expect("saved");
    let vars = env(&store, "claude", Some(id)).expect("env").0;
    assert_eq!(value(&vars, own), ["made-up"]);
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
    assert_eq!(own.login_profile.as_deref(), Some("a1"));
    let mut moved = bot("codex", Some("a1"));
    settle(&store, &mut moved).expect("settled");
    assert_eq!(moved.login_profile, None, "moved to another agent");
    let mut custom = bot("claude", Some("a1"));
    custom.command = Some("my-claude --acp".into());
    settle(&store, &mut custom).expect("settled");
    assert_eq!(custom.login_profile, None, "its own command decides its login");
    assert!(settle(&store, &mut bot("claude", Some("gone"))).is_err());
}

#[test]
fn agents_that_can_have_accounts_list_theirs_by_who_they_sign_in() {
    let store = store_with(&[("a1", "claude"), ("a2", "claude")]);
    change(&store, |all| {
        all[0].account.email = Some("work@example.com".into());
        all[0].account.signed_in = true;
        Ok(())
    })
    .expect("signed in");
    let mut agents = vec![json!({"id": "claude"}), json!({"id": "codex"}), json!({"id": "cursor"})];
    annotate(&store, &mut agents);
    assert_eq!(
        agents[0]["loginProfiles"],
        json!([
            {"id": "a1", "kind": "account", "name": "work@example.com", "label": null,
                "email": "work@example.com", "signedIn": true, "vars": []},
            {"id": "a2", "kind": "account", "name": null, "label": null, "email": null, "signedIn": false, "vars": []},
        ])
    );
    assert_eq!(agents[1]["loginProfiles"], json!([]), "none yet");
    assert_eq!(agents[0]["loginKeys"][0], "ANTHROPIC_API_KEY", "where its API key usually goes");
    // An agent that keeps one sign-in can still have API key profiles.
    assert_eq!(agents[2]["loginProfiles"], json!([]));
    assert_eq!(agents[2]["loginProfileSignIn"], Value::Null);
    assert_eq!(agents[2]["loginKeys"], json!([]));
}

#[test]
fn the_user_names_an_account_and_the_computers_own_login() {
    let store = store_with(&[("a1", "claude")]);
    rename(&store, "claude", Some("a1"), "  Work ").expect("named");
    rename(&store, "claude-acp", None, "Personal").expect("the computer's own, by its registry id too");
    let mut agents = vec![json!({"id": "claude"}), json!({"id": "codex"})];
    annotate(&store, &mut agents);
    assert_eq!(agents[0]["loginProfiles"][0]["name"], "Work");
    assert_eq!(agents[0]["defaultLoginProfile"]["label"], "Personal");
    assert_eq!(agents[0]["loginProfileSignIn"], "terminal");
    assert_eq!(agents[1]["defaultLoginProfile"]["label"], Value::Null, "another agent's own login");
    rename(&store, "claude", Some("a1"), "").expect("an empty name clears it");
    rename(&store, "claude", None, " ").expect("cleared");
    annotate(&store, &mut agents);
    assert_eq!(
        (&agents[0]["loginProfiles"][0]["name"], &agents[0]["defaultLoginProfile"]["label"]),
        (&Value::Null, &Value::Null)
    );
    rename(&store, "cursor", None, "Team").expect("every agent's default login can be named");
    assert!(rename(&store, "claude", Some("gone"), "x").is_err());
}

/// A store whose vault is open, as after the user unlocked it.
fn unlocked(profiles: &[(&str, &str)]) -> Store {
    let store = store_with(profiles);
    *store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![7; 32]));
    store
}

#[tokio::test]
async fn an_api_key_profile_sets_its_variables_and_never_shows_their_values() {
    let store = unlocked(&[]);
    let added = add(&store, "cursor", Kind::ApiKey).await.expect("any agent can have one");
    let id = added["id"].as_str().expect("id").to_owned();
    assert_eq!((&added["kind"], &added["signedIn"]), (&json!("apiKey"), &json!(false)), "no key yet");
    assert!(add(&store, "cursor", Kind::Account).await.is_err(), "it keeps one sign-in of its own");

    let typed = json!({"MY_KEY": " secret ", "MY_BASE_URL": "https://example.com"});
    let set = keys::set(&store, &id, typed.as_object().expect("vars"), &[]).await.expect("saved");
    assert_eq!(set.vars, ["MY_BASE_URL", "MY_KEY"]);
    assert!(set.account.signed_in, "a key can't be tried: having one counts");
    assert!(!set.status().to_string().contains("secret"), "names only");
    let env = env(&store, "cursor", Some(&id)).expect("env").0;
    assert!(env.contains(&("MY_KEY".into(), "secret".into())) && env.contains(&(PROFILE_VAR.into(), id.clone())));
    assert!(reads_vault(&store, "cursor", Some(&id)) && !reads_vault(&store, "cursor", None));
    assert_eq!(super::env(&store, "claude", Some(&id)).expect("another agent's").0, Vec::new());
    assert!(login_command(&store, &id).await.is_err(), "its variables never show in a sign-in terminal");
    assert_eq!(vars(&store, "cursor", Some(&id)), Some(set.vars.clone()));

    // A refused prompt outranks having a key, until new keys come.
    note_signed_in(&store, &id, false);
    assert!(!check(&store, &id).await.expect("checked").account.signed_in);
    let cleared = json!({"MY_BASE_URL": ""});
    let set = keys::set(&store, &id, cleared.as_object().expect("vars"), &[]).await.expect("saved");
    assert_eq!(set.vars, ["MY_KEY"], "an empty value removes it");
    assert!(check(&store, &id).await.expect("checked").account.signed_in);

    for bad in [json!({"PATH": "/x"}), json!({"CODYNC_HOME": "/x"}), json!({"MY KEY": "x"})] {
        assert!(keys::set(&store, &id, bad.as_object().expect("vars"), &[]).await.is_err(), "{bad}");
    }
    assert!(keys::set(&store, &id, &serde_json::Map::new(), &["CODYNC_NOT_SET_ANYWHERE".into()]).await.is_err());
}

#[test]
fn an_api_key_profile_is_forgotten_while_the_vault_is_locked() {
    let store = unlocked(&[]);
    crate::market::vault::write(&store, "login-profile:k1", r#"{"MY_KEY":"secret"}"#).expect("saved");
    *store.secret_key.locked() = None;
    keys::forget(&store, "k1").expect("nothing is read to delete it");
    assert!(!crate::market::vault::saved(&store, "login-profile:k1").expect("looked"));
}

#[test]
fn a_fallback_keeps_only_an_account_of_its_own_agent() {
    let store = store_with(&[("a1", "claude"), ("c1", "codex")]);
    let mut cfg: BotConfig = serde_json::from_value(json!({"id": "b", "name": "B", "backend": "claude", "fallbacks": [
        {"backend": "claude", "loginProfile": "a1"},
        {"backend": "codex", "loginProfile": "a1"},
        {"backend": "codex", "loginProfile": "c1", "command": "my-codex --acp"},
    ]}))
    .expect("bot");
    settle(&store, &mut cfg).expect("settled");
    let accounts: Vec<_> = cfg.fallbacks.iter().map(|f| f.login_profile.as_deref()).collect();
    assert_eq!(accounts, [Some("a1"), None, None], "another agent's account and a custom command's go");
    cfg.fallbacks[0].login_profile = Some("gone".into());
    assert!(settle(&store, &mut cfg).is_err());
}

#[test]
fn an_account_is_in_use_by_a_bot_on_it_and_by_one_that_falls_back_to_it() {
    let store = store_with(&[("a1", "claude"), ("a2", "claude")]);
    let save = |cfg: Value| store.save_bot(&serde_json::from_value(cfg).expect("bot")).expect("saved");
    save(json!({"id": "own", "name": "Own", "backend": "claude", "loginProfile": "a1"}));
    save(
        json!({"id": "step", "name": "Step", "backend": "claude", "fallbacks": [{"backend": "claude", "loginProfile": "a2"}]}),
    );
    assert_eq!(users(&store, "a1").expect("bots"), ["Own"]);
    assert_eq!(users(&store, "a2").expect("bots"), ["Step"], "removing it would leave a fallback no save accepts");
    assert_eq!(users(&store, "gone").expect("bots"), Vec::<String>::new());
}
