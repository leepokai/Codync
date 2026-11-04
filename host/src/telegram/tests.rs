//! Telegram against a fake Bot API and real scripted agents: pairing, the allowed list, commands that cost no turn,
//! streaming replies, long replies as a file, and approvals answered from inline buttons (the first answer wins).
//! Nothing here proves Telegram's real behavior; `docs/guides/telegram-manual-checklist.md` is for that.

use super::*;
use crate::LockExt as _;
use crate::egress::Policy;
use crate::remote::identity::Identity;
use crate::store::Store;
use axum::{
    body::Bytes,
    extract::{Path, State},
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

const TOKEN: &str = "123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw_";
const HOME: i64 = 42;

/// An ACP agent that answers "answered", or with 5000 letters when asked for a "bigreply".
const AGENT: &str = r#"
import json, sys
def send(m): print(json.dumps({"jsonrpc": "2.0", **m}), flush=True)
for line in sys.stdin:
    m = json.loads(line)
    method = m.get("method")
    if method == "initialize":
        send({"id": m["id"], "result": {"protocolVersion": 1, "agentCapabilities": {"loadSession": False}}})
    elif method == "session/new":
        send({"id": m["id"], "result": {"sessionId": "s"}})
    elif method == "session/prompt":
        text = " ".join(b.get("text", "") for b in m["params"]["prompt"])
        reply = "x" * 5000 if "bigreply" in text else "answered"
        send({"method": "session/update", "params": {"sessionId": "s", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}}}})
        send({"id": m["id"], "result": {"stopReason": "end_turn"}})
"#;

// MARK: the fake Bot API

#[derive(Default)]
struct FakeState {
    updates: Mutex<Vec<Value>>,
    calls: Mutex<Vec<(String, Value)>>,
    next_message: AtomicI64,
}

struct Fake {
    state: Arc<FakeState>,
    endpoint: Endpoint,
}

impl Fake {
    fn push(&self, update: Value) {
        self.state.updates.lock().expect("updates").push(update);
    }

    fn calls(&self, method: &str) -> Vec<Value> {
        self.state.calls.lock().expect("calls").iter().filter(|(m, _)| m == method).map(|(_, b)| b.clone()).collect()
    }

    /// The text of every message sent to `chat`.
    fn said(&self, chat: i64) -> Vec<String> {
        self.calls("sendMessage")
            .iter()
            .filter(|b| b["chat_id"] == chat)
            .filter_map(|b| b["text"].as_str().map(str::to_owned))
            .collect()
    }
}

async fn bot_api(
    State(state): State<Arc<FakeState>>,
    Path((bot, method)): Path<(String, String)>,
    body: Bytes,
) -> axum::Json<Value> {
    assert_eq!(bot, format!("bot{TOKEN}"), "every call carries the token of the Telegram bot");
    let parsed =
        serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&body)}));
    state.calls.lock().expect("calls").push((method.clone(), parsed.clone()));
    let result = match method.as_str() {
        "getMe" => json!({"id": 123_456_789, "username": "home_bot"}),
        "getUpdates" => {
            let offset = parsed["offset"].as_i64().unwrap_or(0);
            let fresh: Vec<Value> = state
                .updates
                .lock()
                .expect("updates")
                .iter()
                .filter(|u| u["update_id"].as_i64() >= Some(offset))
                .cloned()
                .collect();
            if fresh.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            json!(fresh)
        }
        "sendMessage" => json!({"message_id": state.next_message.fetch_add(1, Ordering::SeqCst) + 1}),
        _ => json!(true),
    };
    axum::Json(json!({"ok": true, "result": result}))
}

async fn fake_telegram() -> Fake {
    let state = Arc::new(FakeState::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("port");
    let api = format!("http://{}", listener.local_addr().expect("addr"));
    let app = axum::Router::new().route("/{bot}/{method}", axum::routing::post(bot_api)).with_state(state.clone());
    tokio::spawn(async move { axum::serve(listener, app).await });
    let endpoint = Endpoint {
        api,
        policy: Policy::UserChosen,
        poll_seconds: 0,
        edit_every: Duration::ZERO,
        retry: Duration::from_millis(50),
        reconcile: Duration::from_millis(50),
    };
    Fake { state, endpoint }
}

// MARK: a host with real agents

struct Fixture {
    hub: Arc<Hub>,
    dir: std::path::PathBuf,
    next_update: AtomicI64,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-telegram-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("dir");
        let store = Store::open(&dir.join("test.db")).expect("store");
        let hub = Hub::new(
            store,
            "telegram-test".into(),
            Identity::load_or_create(&dir).expect("identity"),
            "test".into(),
            0,
        );
        hub.start().expect("start");
        *hub.store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![9; 32]));
        Self { hub, dir, next_update: AtomicI64::new(1) }
    }

    fn bot(&self, name: &str, script: &std::path::Path, permission: &str) -> String {
        let command = crate::shell::python(script);
        let cfg = serde_json::from_value(json!({"id": "", "name": name, "backend": "fixture", "command": command,
            "cwd": self.dir, "permission": permission}))
        .expect("config");
        self.hub.create_bot(cfg).expect("bot")["id"].as_str().expect("id").to_owned()
    }

    fn agent(&self, name: &str) -> String {
        let script = self.dir.join("agent.py");
        std::fs::write(&script, AGENT).expect("agent");
        self.bot(name, &script, "auto")
    }

    fn asking_agent(&self, name: &str) -> String {
        let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/command_agent.py");
        self.bot(name, &script, "ask")
    }

    /// A private-chat message from `user`.
    fn message(&self, user: i64, text: &str) -> Value {
        json!({"update_id": self.next_update.fetch_add(1, Ordering::SeqCst),
            "message": {"chat": {"id": user, "type": "private"}, "from": {"id": user}, "text": text}})
    }

    fn button(&self, user: i64, data: &str) -> Value {
        json!({"update_id": self.next_update.fetch_add(1, Ordering::SeqCst),
            "callback_query": {"id": format!("cb{}", self.next_update.load(Ordering::SeqCst)), "from": {"id": user}, "data": data,
                "message": {"chat": {"id": user}, "message_id": 1}}})
    }

    async fn add(&self, fake: &Fake, bot: &str) -> Value {
        call_with(&self.hub, &fake.endpoint, "addTelegramBot", json!({"name": "home", "token": TOKEN, "botId": bot}))
            .await
            .expect("added")
    }

    async fn pair(&self, fake: &Fake, user: i64) {
        let made = call_with(&self.hub, &fake.endpoint, "pairTelegram", json!({"name": "home"})).await.expect("code");
        let code = made["result"]["code"].as_str().expect("code").to_owned();
        fake.push(self.message(user, &format!("/start {code}")));
        until("the pairing to be confirmed", || fake.said(user).iter().any(|t| t.starts_with("Paired"))).await;
    }

    fn user_texts(&self, bot: &str) -> Vec<String> {
        let entries = self.hub.store.history(bot, i64::MAX, 100).expect("history");
        entries.iter().filter(|e| e.kind == "user").filter_map(|e| e.data["text"].as_str().map(str::to_owned)).collect()
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

fn serve(f: &Fixture, fake: &Fake) -> tokio::task::JoinHandle<()> {
    tokio::spawn(worker::run(f.hub.clone(), fake.endpoint.clone()))
}

// MARK: tests

#[tokio::test]
async fn a_telegram_bot_is_added_without_ever_showing_its_token() {
    let f = Fixture::new();
    let fake = fake_telegram().await;
    let bot = f.agent("Agent");
    let status = f.add(&fake, &bot).await;
    assert_eq!(status["bots"][0]["username"], "home_bot");
    assert_eq!(status["bots"][0]["botName"], "Agent");
    assert!(!status.to_string().contains(TOKEN), "{status}");
    // Wrong shapes and bots that are not ours are refused before Telegram is asked.
    for body in [
        json!({"name": "Home", "token": TOKEN, "botId": bot}),
        json!({"name": "x", "token": "nope", "botId": bot}),
        json!({"name": "x", "token": TOKEN, "botId": "missing"}),
    ] {
        assert!(call_with(&f.hub, &fake.endpoint, "addTelegramBot", body).await.is_err());
    }
    let removed =
        call_with(&f.hub, &fake.endpoint, "removeTelegramBot", json!({"name": "home"})).await.expect("removed");
    assert_eq!(removed["bots"].as_array().expect("bots").len(), 0);
}

#[tokio::test]
async fn a_person_pairs_with_a_one_time_code_and_strangers_get_nothing() {
    let f = Fixture::new();
    let fake = fake_telegram().await;
    f.add(&fake, &f.agent("Agent")).await;
    let task = serve(&f, &fake);

    fake.push(f.message(99, "let me in"));
    fake.push(f.message(99, "/start WRONG123"));
    until("the wrong code to be answered", || fake.said(99).iter().any(|t| t.contains("didn't work"))).await;
    assert_eq!(fake.said(99).len(), 1, "a stranger who just talks gets no answer");

    f.pair(&fake, HOME).await;
    let status = call_with(&f.hub, &fake.endpoint, "telegramStatus", json!({})).await.expect("status");
    assert_eq!(status["bots"][0]["allowed"], json!([HOME]));
    assert_eq!(status["bots"][0]["state"], "ok");
    assert!(status["bots"][0]["lastReceivedAt"].is_i64());

    // The code is used up: another person cannot use it.
    fake.push(f.message(7, &format!("/start {}", "ABCDEFGH")));
    until("a second try to be refused", || fake.said(7).len() == 1).await;
    assert!(fake.said(7)[0].contains("didn't work"));

    // Taking someone out of the list closes the door again.
    call_with(&f.hub, &fake.endpoint, "setTelegramAllowed", json!({"name": "home", "userId": HOME, "allowed": false}))
        .await
        .expect("removed");
    fake.push(f.message(HOME, "still there?"));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(f.hub.store.bots().expect("bots").iter().all(|b| f.user_texts(&b.config.id).is_empty()));
    task.abort();
}

#[tokio::test]
async fn a_message_reaches_the_bot_and_its_reply_comes_back() {
    let f = Fixture::new();
    let fake = fake_telegram().await;
    let bot = f.agent("Agent");
    f.add(&fake, &bot).await;
    let task = serve(&f, &fake);
    f.pair(&fake, HOME).await;

    fake.push(f.message(HOME, "hello there"));
    until("the reply", || fake.said(HOME).iter().any(|t| t == "answered")).await;
    assert_eq!(f.user_texts(&bot), ["hello there"], "it is in the chat like any message");
    task.abort();
}

#[tokio::test]
async fn a_reply_too_long_for_one_message_also_comes_as_a_file() {
    let f = Fixture::new();
    let fake = fake_telegram().await;
    let bot = f.agent("Agent");
    f.add(&fake, &bot).await;
    let task = serve(&f, &fake);
    f.pair(&fake, HOME).await;

    fake.push(f.message(HOME, "send a bigreply"));
    until("the file", || !fake.calls("sendDocument").is_empty()).await;
    let document = fake.calls("sendDocument").remove(0);
    assert!(document["raw"].as_str().expect("raw").contains(&"x".repeat(5000)), "the whole reply is in the file");
    assert!(fake.said(HOME).iter().all(|t| t.chars().count() <= client::MAX_TEXT));
    task.abort();
}

#[tokio::test]
async fn commands_answer_at_once_and_spend_no_turn() {
    let f = Fixture::new();
    let fake = fake_telegram().await;
    let bot = f.agent("Agent");
    f.add(&fake, &bot).await;
    let task = serve(&f, &fake);
    f.pair(&fake, HOME).await;

    for text in ["/estado", "/rutinas", "/detener"] {
        fake.push(f.message(HOME, text));
    }
    until("three answers", || fake.said(HOME).len() >= 4).await;
    let said = fake.said(HOME);
    assert!(said.iter().any(|t| t == "Agent: idle"), "{said:?}");
    assert!(said.iter().any(|t| t == "Agent has no routines."), "{said:?}");
    assert!(said.iter().any(|t| t == "Stopping Agent."), "{said:?}");
    assert!(f.user_texts(&bot).is_empty(), "no command became a message to the model");
    task.abort();
}

#[tokio::test]
async fn an_approval_is_answered_from_a_button_and_the_first_answer_wins() {
    let f = Fixture::new();
    let fake = fake_telegram().await;
    let bot = f.asking_agent("Runner");
    f.add(&fake, &bot).await;
    let task = serve(&f, &fake);
    f.pair(&fake, HOME).await;

    fake.push(f.message(HOME, "run:cargo build"));
    until("the approval message", || {
        fake.calls("sendMessage").iter().any(|b| b["reply_markup"]["inline_keyboard"].is_array())
    })
    .await;
    let card = fake.calls("sendMessage").into_iter().find(|b| b["reply_markup"].is_object()).expect("card");
    assert!(card["text"].as_str().expect("text").contains("cargo build"));
    let buttons: Vec<(String, String)> = card["reply_markup"]["inline_keyboard"]
        .as_array()
        .expect("rows")
        .iter()
        .flat_map(|row| row.as_array().expect("row").clone())
        .map(|b| (b["text"].as_str().expect("text").to_owned(), b["callback_data"].as_str().expect("data").to_owned()))
        .collect();
    assert_eq!(buttons.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), ["Allow", "Always allow", "Reject"]);
    assert!(buttons.iter().all(|b| b.1.len() <= 64), "Telegram refuses longer callback data");
    let data = |label: &str| buttons.iter().find(|b| b.0 == label).expect("button").1.clone();

    // A stranger's tap, and a tap on a button whose card changed, answer nothing.
    fake.push(f.button(99, &data("Allow")));
    fake.push(f.button(HOME, &format!("{}x", &data("Allow")[..data("Allow").len() - 1])));
    until("both taps to be answered", || fake.calls("answerCallbackQuery").len() == 2).await;
    let toasts: Vec<String> =
        fake.calls("answerCallbackQuery").iter().filter_map(|b| b["text"].as_str().map(str::to_owned)).collect();
    assert_eq!(toasts, ["You are not allowed to answer this.", "That approval changed."]);

    fake.push(f.button(HOME, &data("Allow")));
    until("the reply after the approval", || fake.said(HOME).iter().any(|t| t.starts_with("outcome:"))).await;
    // The same card cannot be answered the other way afterwards.
    fake.push(f.button(HOME, &data("Reject")));
    until("the late tap", || fake.calls("answerCallbackQuery").len() == 4).await;
    let last = fake.calls("answerCallbackQuery").pop().expect("answer");
    assert!(last["text"].as_str().expect("text").contains("already answered"), "{last}");
    let edited =
        fake.calls("editMessageText").iter().filter_map(|b| b["text"].as_str().map(str::to_owned)).collect::<Vec<_>>();
    assert!(edited.iter().any(|t| t.contains("✓ Allow")), "the buttons make way for the answer: {edited:?}");
    task.abort();
}
