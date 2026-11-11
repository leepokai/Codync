use super::*;

#[test]
fn hermes_environment_overrides_reuse_rotating_provider_credentials_without_copying_them() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    *store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![7; 32]));
    save_env(&store, "hermes", None, json!({"OPENROUTER_API_KEY":"first-provider-key"}).as_object().unwrap()).unwrap();
    save_env(
        &store,
        "hermes",
        Some("ssh-bot"),
        json!({"TERMINAL_ENV":"ssh","TERMINAL_SSH_HOST":"existing-ssh-alias"}).as_object().unwrap(),
    )
    .unwrap();
    save_env(&store, "hermes", Some("docker-bot"), json!({"TERMINAL_ENV":"docker"}).as_object().unwrap()).unwrap();
    let own = stored_env(&store, &bot_env_slot("hermes", "ssh-bot")).unwrap();
    assert!(!own.contains_key("OPENROUTER_API_KEY"), "the provider credential must remain inherited");
    let names = env_names(&store, "hermes", "ssh-bot").unwrap();
    assert_eq!(names["own"], json!(["TERMINAL_ENV", "TERMINAL_SSH_HOST"]));
    assert_eq!(names["inherited"], json!(["OPENROUTER_API_KEY"]));
    save_env(&store, "hermes", None, json!({"OPENROUTER_API_KEY":"rotated-provider-key"}).as_object().unwrap())
        .unwrap();
    for (bot, backend) in [("ssh-bot", "ssh"), ("docker-bot", "docker")] {
        let environment: BTreeMap<_, _> = launch_env(&store, "hermes", Some(bot)).unwrap().into_iter().collect();
        assert_eq!(environment["OPENROUTER_API_KEY"], "rotated-provider-key");
        assert_eq!(environment["TERMINAL_ENV"], backend);
    }
    save_env(&store, "hermes", Some("ssh-bot"), json!({"OPENROUTER_API_KEY":"explicit-bot-key"}).as_object().unwrap())
        .unwrap();
    assert_eq!(
        env(&store, "hermes", Some("ssh-bot")).unwrap().into_iter().collect::<BTreeMap<_, _>>()["OPENROUTER_API_KEY"],
        "explicit-bot-key"
    );
    save_env(&store, "hermes", Some("ssh-bot"), json!({"OPENROUTER_API_KEY":""}).as_object().unwrap()).unwrap();
    assert_eq!(
        env(&store, "hermes", Some("ssh-bot")).unwrap().into_iter().collect::<BTreeMap<_, _>>()["OPENROUTER_API_KEY"],
        "rotated-provider-key"
    );
}
