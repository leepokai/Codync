//! The policy against simulated readings, and the acceptance: with memory simulated at 90 % no new turn starts and the
//! chat says why; when memory frees up, the next message is answered.

use super::*;
use crate::api::devices::Caller;
use crate::remote::identity::Identity;
use crate::store::Store;
use serde_json::Value;

const GB: u64 = 1024 * 1024 * 1024;

/// A computer that reads whatever the test last set.
#[derive(Clone, Default)]
struct Simulated(Arc<Mutex<Reading>>);

impl Simulated {
    fn memory(&self, used_percent: u64) {
        let mut reading = self.0.locked();
        reading.ram_total = 100 * GB;
        reading.ram_free = (100 - used_percent) * GB;
    }

    fn agent(&self, bot: &str, pid: u32, rss: u64) {
        let mut reading = self.0.locked();
        reading.agents.retain(|a| a.pid != pid);
        reading.agents.push(AgentUse { bot_id: bot.into(), pid, rss });
    }

    /// The disk with `free_percent` of its space left.
    fn disk(&self, free_percent: u64) {
        let mut reading = self.0.locked();
        reading.disk_total = 100 * GB;
        reading.disk_free = free_percent * GB;
    }
}

impl Source for Simulated {
    fn read(&self, agents: &[(String, u32)]) -> Reading {
        let mut reading = self.0.locked().clone();
        // Only the agents the host asked about, as the real computer answers.
        reading.agents.retain(|a| agents.iter().any(|(_, pid)| *pid == a.pid));
        reading
    }
}

fn monitor(simulated: &Simulated) -> Monitor {
    Monitor::new(Box::new(simulated.clone()), Thresholds::DEFAULT)
}

#[test]
fn a_low_disk_is_said_once_and_recovered_once() {
    let simulated = Simulated::default();
    let monitor = monitor(&simulated);
    simulated.memory(10);
    simulated.disk(50);
    assert_eq!(monitor.sample(1).disk, None, "plenty of room says nothing");
    simulated.disk(5);
    assert_eq!(monitor.sample(2).disk, Some(true));
    simulated.disk(6);
    assert_eq!(monitor.sample(3).disk, None, "an episode is not repeated every sample");
    simulated.disk(20);
    assert_eq!(monitor.sample(4).disk, Some(false));
}

#[test]
fn levels_follow_the_thresholds_and_the_thresholds_must_make_sense() {
    let t = Thresholds::DEFAULT;
    assert_eq!(
        [69, 70, 84, 85, 100].map(|u| t.level(u)),
        [Level::Normal, Level::Warning, Level::Warning, Level::Critical, Level::Critical]
    );
    assert!(t.valid());
    for bad in [(0, 50), (50, 50), (60, 40), (50, 100), (99, 99)] {
        assert!(!Thresholds { warn: bad.0, critical: bad.1 }.valid(), "{bad:?}");
    }
    let mut reading = Reading::default();
    assert_eq!(reading.ram_used_percent(), 0, "nothing known is not full");
    (reading.ram_total, reading.ram_free) = (8 * GB, 2 * GB);
    assert_eq!(reading.ram_used_percent(), 75);
}

#[test]
fn a_reading_that_hovers_at_the_limit_does_not_flip_every_sample() {
    let t = Thresholds::DEFAULT;
    // From a warning, 85 % is the limit, and it stays the limit until 82 %.
    assert_eq!(t.level_from(84, Level::Warning), Level::Warning);
    assert_eq!(t.level_from(85, Level::Warning), Level::Critical);
    assert_eq!(t.level_from(84, Level::Critical), Level::Critical);
    assert_eq!(t.level_from(82, Level::Critical), Level::Critical);
    assert_eq!(t.level_from(81, Level::Critical), Level::Warning);
    // From a warning, it stays one until 3 points below the warning level.
    assert_eq!(t.level_from(69, Level::Warning), Level::Warning);
    assert_eq!(t.level_from(66, Level::Warning), Level::Normal);
    // From normal, the thresholds themselves apply.
    assert_eq!(t.level_from(69, Level::Normal), Level::Normal);
    assert_eq!(t.level_from(70, Level::Normal), Level::Warning);
}

#[test]
fn memory_at_ninety_percent_refuses_new_turns_and_says_why_until_it_frees_up() {
    let computer = Simulated::default();
    let monitor = monitor(&computer);
    let now = 1_000_000;
    assert!(monitor.admit(now).is_ok(), "nothing has been read yet");

    computer.memory(60);
    assert_eq!(monitor.sample(now).level_changed, None);
    assert!(monitor.admit(now).is_ok());

    computer.memory(75);
    assert_eq!(monitor.sample(now).level_changed, Some((Level::Normal, Level::Warning)));
    assert!(monitor.admit(now).is_ok(), "a warning does not stop work");

    computer.memory(90);
    assert_eq!(monitor.sample(now).level_changed, Some((Level::Warning, Level::Critical)));
    let refused = monitor.admit(now).expect_err("no new turn at 90 %").to_string();
    assert!(
        refused.contains("90 %") && refused.contains("85 %") && refused.contains("not starting new turns"),
        "{refused}"
    );
    assert!(monitor.admit(now + FRESH_MS + 1).is_ok(), "a reading that old says nothing about now");

    computer.memory(50);
    assert_eq!(monitor.sample(now + 1).level_changed, Some((Level::Critical, Level::Normal)));
    assert!(monitor.admit(now + 1).is_ok());
}

#[test]
fn a_level_is_announced_once_not_every_sample() {
    let computer = Simulated::default();
    let monitor = monitor(&computer);
    computer.memory(90);
    assert!(monitor.sample(0).level_changed.is_some());
    assert_eq!(monitor.sample(1).level_changed, None);
    assert_eq!(monitor.sample(2).level_changed, None);
}

#[test]
fn the_chat_is_told_in_words_a_person_can_use() {
    let t = Thresholds::DEFAULT;
    assert!(level_text(Level::Normal, 40, t).is_none());
    let (warning, style) = level_text(Level::Warning, 72, t).expect("a warning");
    assert!(warning.contains("72 %") && warning.contains("85 %"), "{warning}");
    assert!(matches!(style, NoticeStyle::Info));
    let (critical, style) = level_text(Level::Critical, 91, t).expect("a limit");
    assert!(critical.contains("not starting new turns") && critical.contains("idle"), "{critical}");
    assert!(matches!(style, NoticeStyle::Error));
    let (easing, _) = level_text(Level::Critical, 83, t).expect("still at the limit");
    assert!(easing.contains("easing") && easing.contains("below 82 %"), "{easing}");
    assert!(!easing.contains("above the 85"), "a reading under the limit must not claim to be above it");
}

#[test]
fn an_agent_that_doubles_is_told_and_one_that_keeps_growing_is_stopped() {
    let computer = Simulated::default();
    let monitor = monitor(&computer);
    monitor.set_agent("bot", Some(7));
    let mut verdicts = Vec::new();
    for rss in [500, 520, 510, 530, 2200, 2400, 2600, 2800, 3000] {
        computer.agent("bot", 7, rss * 1024 * 1024);
        verdicts.push(monitor.sample(0).growth.into_iter().next().map(|(_, _, _, growth)| growth));
    }
    assert_eq!(verdicts[..4], [None, None, None, None], "steady use says nothing");
    assert_eq!(verdicts[4], Some(Growth::Grew), "the jump is told once");
    assert_eq!(verdicts[5..8], [None, None, Some(Growth::Persists)], "and if it persists, it is stopped");
}

#[test]
fn big_but_steady_and_small_but_fast_are_not_abnormal() {
    let mut steady = Track::default();
    assert!((0..10).all(|_| steady.observe(4 * GB) == Growth::Steady), "a big agent that stays big is fine");
    let mut small = Track::default();
    for rss in [10, 12, 11, 13, 80, 90, 95, 100] {
        assert_eq!(small.observe(rss * 1024 * 1024), Growth::Steady, "below the floor nothing is abnormal");
    }
    let mut recovers = Track::default();
    for rss in [500, 500, 500, 500] {
        recovers.observe(rss * 1024 * 1024);
    }
    assert_eq!(recovers.observe(2 * GB), Growth::Grew);
    assert_eq!(recovers.observe(600 * 1024 * 1024), Growth::Steady, "when it settles the count starts over");
}

#[test]
fn an_agent_that_is_gone_is_forgotten() {
    let computer = Simulated::default();
    let monitor = monitor(&computer);
    monitor.set_agent("bot", Some(9));
    computer.agent("bot", 9, GB);
    monitor.sample(0);
    assert_eq!(monitor.snapshot().expect("a reading").reading.agents.len(), 1);
    computer.0.locked().agents.clear();
    monitor.sample(1);
    assert_eq!(monitor.snapshot().expect("a reading").reading.agents.len(), 0);
    computer.agent("bot", 9, GB);
    monitor.sample(2);
    assert!(
        monitor.snapshot().expect("a reading").reading.agents.is_empty(),
        "it is not measured again until it starts again"
    );
}

#[test]
fn thresholds_are_saved_and_a_bad_pair_is_refused() {
    let store = Store::open(std::path::Path::new(":memory:")).expect("store");
    let computer = Simulated::default();
    let monitor = monitor(&computer);
    computer.memory(80);
    monitor.sample(0);
    assert_eq!(monitor.snapshot().expect("a reading").level, Level::Warning);
    monitor.set_thresholds(&store, Thresholds { warn: 50, critical: 75 }).expect("saved");
    assert_eq!(
        monitor.snapshot().expect("a reading").level,
        Level::Critical,
        "the new limit applies to the last reading too"
    );
    assert_eq!(Monitor::saved_thresholds(&store), Thresholds { warn: 50, critical: 75 });
    assert!(monitor.set_thresholds(&store, Thresholds { warn: 90, critical: 80 }).is_err());
    assert_eq!(
        Monitor::saved_thresholds(&store),
        Thresholds { warn: 50, critical: 75 },
        "a refused pair changes nothing"
    );
}

// MARK: the acceptance, with a real bot

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
        send({"method": "session/update", "params": {"sessionId": "s", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "answered"}}}})
        send({"id": m["id"], "result": {"stopReason": "end_turn"}})
"#;

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn at_ninety_percent_a_message_starts_no_turn_and_the_chat_says_why() {
    let dir = std::env::temp_dir().join(format!("codync-health-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("dir");
    let script = dir.join("agent.py");
    std::fs::write(&script, AGENT).expect("agent");
    let store = Store::open(&dir.join("test.db")).expect("store");
    let hub =
        Hub::new(store, "health-test".into(), Identity::load_or_create(&dir).expect("identity"), "test".into(), 0);
    hub.start().expect("start");
    let cfg = serde_json::from_value(json!({"id": "", "name": "Agent", "backend": "fixture",
        "command": crate::shell::python(&script), "cwd": dir, "permission": "auto"}))
    .expect("config");
    let bot = hub.create_bot(cfg).expect("bot")["id"].as_str().expect("id").to_owned();

    let computer = Simulated::default();
    hub.health.use_source(Box::new(computer.clone()));
    computer.memory(90);
    hub.health.sample(crate::store::now_ms());

    crate::api::dispatch(&hub, &Caller::Local, "send", json!({"botId": bot, "text": "hello"})).await.expect("sent");
    let notices = |hub: &Hub| -> Vec<String> {
        hub.store
            .history(&bot, i64::MAX, 100)
            .expect("history")
            .iter()
            .filter(|e| e.kind == "notice")
            .filter_map(|e| e.data["text"].as_str().map(str::to_owned))
            .collect()
    };
    until("the explanation", || notices(&hub).iter().any(|t| t.contains("Not started"))).await;
    let said = notices(&hub).into_iter().find(|t| t.contains("Not started")).expect("a notice");
    assert!(said.contains("90 %") && said.contains("85 %"), "{said}");
    let answered =
        |hub: &Hub| hub.store.history(&bot, i64::MAX, 100).expect("history").iter().any(|e| e.kind == "agent");
    assert!(!answered(&hub), "no turn ran");

    // The status says the same to a screen.
    let status = crate::api::dispatch(&hub, &Caller::Local, "hostHealth", json!({})).await.expect("health");
    assert_eq!(
        (status["level"].as_str(), status["refusing"].as_bool(), status["ramUsedPercent"].as_u64()),
        (Some("critical"), Some(true), Some(90))
    );

    // Memory frees up: the next message is answered.
    computer.memory(40);
    hub.health.sample(crate::store::now_ms());
    crate::api::dispatch(&hub, &Caller::Local, "send", json!({"botId": bot, "text": "again"})).await.expect("sent");
    until("the answer", || answered(&hub)).await;
    let _: Value = status;
    let _ = std::fs::remove_dir_all(dir);
}
