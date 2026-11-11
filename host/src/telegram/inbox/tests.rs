use super::*;
use crate::LockExt as _;
use crate::remote::identity::Identity;
use crate::store::Store;
use crate::telegram::{Config, TgBot, load, save};
use serde_json::json;
use std::path::PathBuf;

struct Fixture {
    hub: Arc<Hub>,
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("codync-telegram-inbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        *hub.store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![9; 32]));
        save(
            &hub.store,
            &Config {
                bots: vec![TgBot {
                    name: "home".into(),
                    token: "test-token".into(),
                    username: "home_bot".into(),
                    bot_id: "a".into(),
                    allowed: vec![42],
                    memory: String::new(),
                    icon: String::new(),
                    generation: uuid::Uuid::new_v4().to_string(),
                }],
            },
        )
        .unwrap();
        Self { hub, root }
    }

    fn inbox(&self) -> Inbox {
        Inbox::open(&self.hub, "home", worker::fingerprint(&worker::current(&self.hub, "home").unwrap())).unwrap()
    }
}

#[test]
fn hermes_telegram_inbox_is_durable_ordered_and_deduplicates_accepted_updates() {
    let fixture = Fixture::new();
    let inbox = fixture.inbox();
    let first = json!({"update_id":1,"message":{"text":"first"}});
    let second = json!({"update_id":2,"message":{"text":"second"}});
    assert!(inbox.push(&fixture.hub, "home", &first).unwrap());
    assert!(inbox.push(&fixture.hub, "home", &first).unwrap());
    assert!(inbox.push(&fixture.hub, "home", &second).unwrap());
    let recovered = fixture.inbox();
    let pending = recovered.read(&fixture.hub).unwrap();
    assert_eq!(pending, vec![first.clone(), second.clone()]);
    recovered.finish(&fixture.hub, &first).unwrap();
    assert_eq!(recovered.read(&fixture.hub).unwrap(), vec![second]);
    forget(&fixture.hub.store, "home").unwrap();
    assert!(recovered.read(&fixture.hub).unwrap().is_empty());
}

#[test]
fn hermes_telegram_authorization_epochs_discard_old_notes_even_after_reauthorization() {
    let fixture = Fixture::new();
    let old = fixture.inbox();
    old.push(&fixture.hub, "home", &json!({"update_id":1})).unwrap();
    let mut config = load(&fixture.hub.store).unwrap();
    // Same resulting allowed ids, but a revoke/re-pair cycle created a new authorization epoch.
    config.bots[0].generation = uuid::Uuid::new_v4().to_string();
    save(&fixture.hub.store, &config).unwrap();
    let current = fixture.inbox();
    assert_ne!(current.tag, old.tag);
    assert!(current.read(&fixture.hub).unwrap().is_empty());
    assert!(fixture.hub.store.kv_read(&old.key).unwrap().is_none());
    assert!(old.push(&fixture.hub, "home", &json!({"update_id":2})).is_err());
    assert!(fixture.hub.store.kv_read(&old.key).unwrap().is_none(), "old receiver must not recreate its revoked queue");
}
