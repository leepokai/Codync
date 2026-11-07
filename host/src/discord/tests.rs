use super::*;
use crate::remote::identity::Identity;
use crate::store::Store;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

const CHANNEL: &str = "111111111111111111";
const GUILD: &str = "222222222222222222";
const MINE: &str = "333333333333333333";
const OTHER: &str = "444444444444444444";
const BOT: &str = "4294967296";

struct Fixture {
    hub: Arc<Hub>,
    dir: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A host with one bot bound to [`CHANNEL`] for person [`MINE`], its Discord settings in the vault.
fn fixture() -> Fixture {
    let dir = std::env::temp_dir().join(format!("codync-discord-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Store::open(std::path::Path::new(":memory:")).expect("store");
    let hub = Hub::new(store, "discord-test".into(), Identity::load_or_create(&dir).expect("identity"), "t".into(), 0);
    *hub.store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![7; 32]));
    let bot =
        serde_json::from_value(json!({"id": "b1", "name": "Bot", "backend": "fixture", "command": "x", "cwd": dir}))
            .expect("bot");
    hub.store.save_bot(&bot).expect("saved");
    let config = Config {
        guild_id: GUILD.into(),
        bot_user_id: BOT.into(),
        bot_token: "abc.def.ghi".into(),
        bindings: vec![Binding { channel_id: CHANNEL.into(), bot_id: "b1".into(), allowed_users: vec![MINE.into()] }],
        generation: "g1".into(),
    };
    crate::market::vault::write(&hub.store, SLOT, &serde_json::to_string(&config).expect("json")).expect("vault");
    Fixture { hub, dir }
}

static NEXT: AtomicU64 = AtomicU64::new(1_000);

fn message(user: &str, guild: Option<&str>, text: &str, mentions: &[&str]) -> Value {
    json!({"id": NEXT.fetch_add(1, Ordering::Relaxed).to_string(), "channel_id": CHANNEL,
        "guild_id": guild.map_or(Value::Null, Value::from), "content": text,
        "author": {"id": user, "bot": false}, "mentions": mentions.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()})
}

fn config(hub: &Hub) -> Config {
    load(&hub.store).expect("load").expect("configured")
}

#[test]
fn a_mention_from_an_allowed_person_is_queued() {
    let f = fixture();
    ingress::accept(&f.hub, &config(&f.hub), &message(MINE, Some(GUILD), &format!("<@{BOT}> resúmeme esto"), &[BOT]))
        .expect("accept");
    let pending = f.hub.store.pending_discord().expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, "resúmeme esto");
    assert_eq!(pending[0].bot, "b1");
}

#[test]
fn a_stranger_or_another_bot_is_ignored() {
    let f = fixture();
    let c = config(&f.hub);
    ingress::accept(&f.hub, &c, &message(OTHER, Some(GUILD), &format!("<@{BOT}> hi"), &[BOT])).expect("accept");
    assert!(f.hub.store.pending_discord().expect("pending").is_empty());
    let mut bot = message(MINE, Some(GUILD), &format!("<@{BOT}> hi"), &[BOT]);
    bot["author"]["bot"] = json!(true);
    ingress::accept(&f.hub, &c, &bot).expect("accept");
    assert!(f.hub.store.pending_discord().expect("pending").is_empty());
}

#[test]
fn a_channel_message_answers_after_the_first_one() {
    let f = fixture();
    let c = config(&f.hub);
    ingress::accept(&f.hub, &c, &message(MINE, Some(GUILD), &format!("<@{BOT}> start"), &[BOT])).expect("accept");
    ingress::accept(&f.hub, &c, &message(MINE, Some(GUILD), "and then?", &[])).expect("accept");
    let pending = f.hub.store.pending_discord().expect("pending");
    assert_eq!(pending.len(), 2, "the second one answers the same conversation");
    assert_eq!(pending[1].text, "and then?");
}

#[test]
fn a_direct_message_from_an_allowed_person_counts() {
    let f = fixture();
    ingress::accept(&f.hub, &config(&f.hub), &message(MINE, None, "hola", &[])).expect("accept");
    assert_eq!(f.hub.store.pending_discord().expect("pending").len(), 1);
}

#[test]
fn attachments_are_named_and_other_mentions_are_kept() {
    let f = fixture();
    let mut msg = message(MINE, None, "<@9> mira", &["9"]);
    msg["attachments"] = json!([{"filename": "captura.png"}]);
    ingress::accept(&f.hub, &config(&f.hub), &msg).expect("accept");
    let pending = f.hub.store.pending_discord().expect("pending");
    assert_eq!(pending[0].text, "<@9> mira\n\n[Attached, not downloaded: captura.png]");
}

#[test]
fn ids_and_tokens_have_the_shape_discord_uses() {
    assert!(id_ok("123456789012345678"));
    assert!(!id_ok("") && !id_ok("abc") && !id_ok(&"1".repeat(33)));
    assert!(token_ok("fakeDiscordBotTokenForTests.abc.alsoNotARealToken0123456789"));
    for bad in ["", "abc.def", "short.def.012345678901234567890", "a b.c.d"] {
        assert!(!token_ok(bad), "{bad}");
    }
}

#[test]
fn a_hello_sets_the_heartbeat_and_a_reconnect_is_honoured() {
    assert!(matches!(
        gateway::action(&json!({"op": 10, "d": {"heartbeat_interval": 500}})),
        gateway::Action::Hello(500)
    ));
    assert!(matches!(gateway::action(&json!({"op": 7})), gateway::Action::Reconnect));
    assert!(matches!(gateway::action(&json!({"op": 1})), gateway::Action::Heartbeat));
    assert!(matches!(gateway::action(&json!({"op": 0, "t": "READY"})), gateway::Action::Ignore));
    let identify = gateway::identify(&Config {
        guild_id: GUILD.into(),
        bot_user_id: BOT.into(),
        bot_token: "abc.def.ghi".into(),
        bindings: Vec::new(),
        generation: "g".into(),
    });
    assert_eq!(identify["op"], 2);
    assert_eq!(identify["d"]["token"], "abc.def.ghi");
    assert!(identify["d"]["intents"].as_u64().is_some_and(|i| i & 32768 != 0), "message content");
}

#[test]
fn only_the_gateway_host_is_accepted() {
    let ep = Endpoint::discord();
    assert!(ep.socket_ok("wss://gateway.discord.gg/?v=10"));
    assert!(!ep.socket_ok("wss://example.com/?v=10"));
}
