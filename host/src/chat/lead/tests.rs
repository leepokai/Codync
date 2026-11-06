use super::*;
use crate::remote::identity::Identity;
use crate::usage::{Provider, Usage, Window};
use std::path::PathBuf;
use std::time::Duration;

/// An ACP agent that answers "opened" to a call's opening and "answered" to anything else.
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
        reply = "opened" if "you speak first" in text else "answered"
        send({"method": "session/update", "params": {"sessionId": "s", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": reply}}}})
        send({"id": m["id"], "result": {"stopReason": "end_turn"}})
"#;

struct Fixture {
    hub: Arc<Hub>,
    dir: PathBuf,
    command: String,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-lead-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let agent = dir.join("agent.py");
        std::fs::write(&agent, AGENT).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let hub = Hub::new(store, "lead-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        hub.start().unwrap();
        Self { hub, command: crate::shell::python(&agent), dir }
    }

    fn cfg(&self, name: &str) -> BotConfig {
        serde_json::from_value(
            json!({"id": "", "name": name, "backend": "fixture", "command": self.command, "cwd": self.dir}),
        )
        .unwrap()
    }

    fn agent_texts(&self, bot: &str) -> Vec<String> {
        let entries = self.hub.store.history(bot, i64::MAX, 50).unwrap();
        entries
            .iter()
            .filter(|e| e.kind == "agent")
            .filter_map(|e| e.data["text"].as_str().map(str::to_owned))
            .collect()
    }

    async fn shutdown(self) {
        self.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(self.dir);
    }
}

#[tokio::test]
async fn the_lead_is_made_once_with_its_check_in_and_stays() {
    let f = Fixture::new();
    let lead = make(&f.hub, f.cfg(NAME)).unwrap();
    assert_eq!(id(&f.hub.store).unwrap().as_deref(), Some(lead.as_str()));
    let routines = f.hub.routines.list(&lead);
    let check_in = &routines["routines"][0];
    assert_eq!(
        (check_in["name"].as_str(), check_in["triggers"][0]["expression"].as_str()),
        (Some("Check in"), Some(CHECK_IN_CRON))
    );
    ensure(&f.hub).await.unwrap();
    assert_eq!(f.hub.store.bots().unwrap().iter().filter(|b| b.config.lead).count(), 1, "a computer has one lead");

    assert!(f.hub.delete_bot(&lead).is_err(), "the lead can't be deleted");
    assert!(f.hub.update_bot(&json!({"id": lead, "hidden": true})).is_err(), "the lead can't be hidden");
    let other = f.hub.create_bot(f.cfg("Other")).unwrap()["id"].as_str().unwrap().to_owned();
    f.hub.update_bot(&json!({"id": other, "lead": true})).unwrap();
    f.hub.update_bot(&json!({"id": lead, "lead": false, "name": "Boss"})).unwrap();
    assert_eq!(id(&f.hub.store).unwrap().as_deref(), Some(lead.as_str()), "only the host picks the lead");
    let asked =
        json!({"name": "X", "lead": true, "connectors": [], "skills": [], "backend": "fixture", "command": "x"});
    assert!(!crate::agent::new_bot::with_defaults(&f.hub, asked).await.unwrap().lead, "nor does a client or a bot");

    let profile = crate::chat::context::render(&f.hub.store, &f.hub.store.bot(&lead).unwrap().unwrap().config);
    assert!(profile.contains(BRIEF));
    assert!(
        !crate::chat::context::render(&f.hub.store, &f.hub.store.bot(&other).unwrap().unwrap().config).contains(BRIEF)
    );
    f.hub.delete_bot(&other).unwrap();
    f.shutdown().await;
}

#[tokio::test]
async fn check_ins_wait_while_the_plan_is_nearly_used_up() {
    let f = Fixture::new();
    let lead = make(&f.hub, f.cfg(NAME)).unwrap();
    let routine = f.hub.routines.list(&lead)["routines"][0]["id"].as_str().unwrap().to_owned();
    let (scheduled, test) = (json!({"source": "schedule"}), json!({"source": "test"}));
    let used = |percent: f64| Usage {
        providers: vec![Provider {
            id: "fixture",
            name: "Fixture",
            windows: vec![Window {
                id: "week".into(),
                label: "Week".into(),
                percent,
                resets_at: None,
                resets_text: None,
                text: None,
            }],
            source: "agent",
            updated_at: 0,
        }],
    };
    f.hub.set_usage(used(40.0));
    assert!(!check_in_waits(&f.hub, &routine, &lead, &scheduled));
    f.hub.set_usage(used(90.0));
    assert!(check_in_waits(&f.hub, &routine, &lead, &scheduled));
    assert!(!check_in_waits(&f.hub, &routine, &lead, &test), "a test run the user asked for still runs");
    assert!(!check_in_waits(&f.hub, "another", &lead, &scheduled), "the user's own routines still run");
    f.shutdown().await;
}

#[tokio::test]
async fn the_lead_speaks_first_on_a_call() {
    let f = Fixture::new();
    let lead = make(&f.hub, f.cfg(NAME)).unwrap();
    let other = f.hub.create_bot(f.cfg("Other")).unwrap()["id"].as_str().unwrap().to_owned();
    call_started(&f.hub, &other, None).unwrap();
    crate::chat::calls::heard(&f.hub, &lead, "c1", "local").unwrap();
    call_started(&f.hub, &lead, Some("c1")).unwrap();
    until(|| f.hub.runtime(&lead).status == BotStatus::Idle && !f.agent_texts(&lead).is_empty(), "the lead opens")
        .await;
    assert_eq!(f.agent_texts(&lead), ["opened"]);
    assert!(f.agent_texts(&other).is_empty(), "other bots wait for the user");
    let opening = f.hub.store.history(&lead, i64::MAX, 50).unwrap().into_iter().find(|e| e.kind == "agent").unwrap();
    assert_eq!(opening.turn, 1, "a turn of its own");
    assert_eq!(opening.data["call"], "c1", "the opening is the call's, kept in its conversation");

    // A lead whose last turn failed still opens the next call.
    f.hub.set_runtime(&lead, |r| r.status = BotStatus::Error);
    call_started(&f.hub, &lead, None).unwrap();
    until(|| f.agent_texts(&lead).len() == 2, "the lead opens again").await;
    f.shutdown().await;
}

/// Waits up to 20 s for `done`.
async fn until(done: impl Fn() -> bool, what: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect(what);
}
