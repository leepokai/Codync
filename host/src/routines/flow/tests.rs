use super::engine::{Root, Start, execute};
use super::*;
use crate::api::devices::{Caller, Forbidden};
use crate::hub::Hub;
use crate::store::{BotConfig, FlowLink, NewStep, StepStatus, Store, now_ms};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// A host with bots `a`, `b`, `c` (the team fixture agent), a group `g` and a terminal session `x`.
struct Fixture {
    hub: Arc<Hub>,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-flow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        for (id, extra) in [("a", json!({})), ("b", json!({})), ("c", json!({})), ("g", json!({"kind": "group"}))] {
            let cwd = dir.join(id);
            std::fs::create_dir_all(&cwd).unwrap();
            let mut cfg = json!({
                "id": id, "name": id, "backend": "fixture", "cwd": cwd,
                "command": crate::shell::python(&agent), "notify": false, "permission": "ask",
            });
            cfg.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            store.save_bot(&serde_json::from_value::<BotConfig>(cfg).unwrap()).unwrap();
        }
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        let hub = Hub::new(store, "test-host".into(), identity, "test-token".into(), 19222);
        hub.start().unwrap();
        // A terminal session doesn't outlive the host that started, so this one comes after the start.
        let external: BotConfig =
            serde_json::from_value(json!({"id": "x", "name": "x", "backend": "external", "cwd": dir})).unwrap();
        hub.store.save_bot(&external).unwrap();
        Self { hub, dir }
    }

    fn routine(&self, bot: &str) -> String {
        let body = json!({"name": "Digest", "instruction": "Make the digest",
            "triggers": [{"type": "interval", "seconds": 3600}]});
        self.hub.routines.save(&self.hub, bot, &body).unwrap()["routine"]["id"].as_str().unwrap().to_owned()
    }

    fn link(&self, routine: &str, from: &str, to: &str, instruction: &str) -> FlowLink {
        let owner = "a";
        let link = FlowLink {
            id: uuid::Uuid::new_v4().to_string(),
            routine_id: routine.into(),
            owner_bot_id: owner.into(),
            from_bot_id: from.into(),
            to_bot_id: to.into(),
            instruction: instruction.into(),
            created_at: 1,
        };
        self.hub.store.flow_add_link(&link, |_| Ok(())).unwrap();
        link
    }

    #[allow(clippy::unused_self, reason = "reads like the other fixture steps")]
    fn start(&self, routine: &str, run: &str, root: Root) -> Start {
        Start {
            run_id: run.into(),
            routine_id: routine.into(),
            routine_name: "Digest".into(),
            owner: "a".into(),
            run_at: now_ms(),
            timeout: Duration::from_secs(20),
            max_depth: 5,
            instruction: "Make the digest".into(),
            root,
        }
    }

    fn steps(&self, run: &str) -> Vec<(String, StepStatus)> {
        self.hub.store.flow_run_steps(run).unwrap().into_iter().map(|s| (s.bot_id, s.status)).collect()
    }

    fn step(&self, run: &str, bot: &str) -> crate::store::FlowStep {
        self.hub.store.flow_run_steps(run).unwrap().into_iter().find(|s| s.bot_id == bot).unwrap()
    }

    fn prompts_of(&self, bot: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.join(bot).join("prompts.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    async fn until(&self, condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture condition timed out");
    }

    async fn call(&self, method: &str, body: Value) -> anyhow::Result<Value> {
        crate::api::dispatch(&self.hub, &Caller::Local, method, body).await
    }

    async fn shutdown(self) {
        self.hub.shutdown().await;
        let _ = std::fs::remove_dir_all(self.dir);
    }
}

#[tokio::test]
async fn a_reply_travels_the_chain_and_every_bot_leaves_a_step() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "Summarize it");
    f.link(&r, "b", "c", "Translate it");
    execute(f.hub.clone(), f.start(&r, "run1", Root::Replied("Seven items today".into()))).await;
    let steps = f.steps("run1");
    assert_eq!(steps.len(), 3);
    assert!(steps.iter().all(|(_, status)| *status == StepStatus::Succeeded), "{steps:?}");
    let (b, c) = (f.step("run1", "b"), f.step("run1", "c"));
    assert!(b.input.contains("a answered:\n\nSeven items today") && b.input.ends_with("Summarize it"));
    assert_eq!(b.output.as_deref(), Some("reply: Summarize it"));
    assert!(c.input.contains("b answered:\n\nreply: Summarize it"), "{}", c.input);
    assert!(b.delivery_id.is_some() && f.step("run1", "a").delivery_id.is_none());
    assert_eq!(f.prompts_of("b").len(), 1);
    assert_eq!(f.hub.routines.flows.running(), 0, "a finished flow is no longer registered");
    f.shutdown().await;
}

#[tokio::test]
async fn a_failure_in_the_middle_skips_what_hangs_below_it() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "FAIL on purpose");
    f.link(&r, "b", "c", "Never reached");
    execute(f.hub.clone(), f.start(&r, "run2", Root::Replied("seed".into()))).await;
    assert_eq!(
        f.steps("run2"),
        [("a".into(), StepStatus::Succeeded), ("b".into(), StepStatus::Failed), ("c".into(), StepStatus::Skipped)]
    );
    assert!(f.step("run2", "b").error.is_some_and(|e| !e.is_empty()));
    assert!(f.prompts_of("c").is_empty(), "a skipped step sends nothing");
    f.shutdown().await;
}

#[tokio::test]
async fn a_routine_run_that_failed_skips_the_whole_flow() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "");
    f.link(&r, "b", "c", "");
    execute(f.hub.clone(), f.start(&r, "run3", Root::Failed("agent crashed".into()))).await;
    assert_eq!(
        f.steps("run3"),
        [("a".into(), StepStatus::Failed), ("b".into(), StepStatus::Skipped), ("c".into(), StepStatus::Skipped)]
    );
    assert_eq!(f.prompts_of("b").len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn cancelling_a_flow_cancels_the_running_step_and_the_ones_after_it() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "BLOCK the work");
    f.link(&r, "b", "c", "");
    let hub = f.hub.clone();
    let start = f.start(&r, "run4", Root::Replied("seed".into()));
    let flow = tokio::spawn(execute(hub, start));
    f.until(|| f.hub.store.flow_run_steps("run4").unwrap().iter().any(|s| s.bot_id == "b")).await;
    assert!(f.hub.routines.flows.cancel_run("run4"));
    flow.await.unwrap();
    assert_eq!(f.step("run4", "b").status, StepStatus::Cancelled);
    assert_eq!(f.step("run4", "c").status, StepStatus::Skipped);
    f.shutdown().await;
}

#[tokio::test]
async fn running_the_same_run_again_adds_no_steps_and_sends_nothing_twice() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "Once");
    execute(f.hub.clone(), f.start(&r, "run5", Root::Replied("seed".into()))).await;
    let before = f.hub.store.flow_run_steps("run5").unwrap();
    execute(f.hub.clone(), f.start(&r, "run5", Root::Replied("seed".into()))).await;
    assert_eq!(f.hub.store.flow_run_steps("run5").unwrap(), before);
    assert_eq!(f.prompts_of("b").len(), 1);
    f.shutdown().await;
}

#[tokio::test]
async fn links_that_form_a_loop_are_refused_again_when_running() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "");
    f.link(&r, "b", "c", "");
    f.link(&r, "c", "b", "");
    execute(f.hub.clone(), f.start(&r, "run6", Root::Replied("seed".into()))).await;
    assert_eq!(
        f.steps("run6"),
        [("a".into(), StepStatus::Succeeded), ("b".into(), StepStatus::Skipped), ("c".into(), StepStatus::Skipped)]
    );
    assert_eq!(f.prompts_of("b").len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn the_depth_limit_skips_the_bots_beyond_it() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "");
    f.link(&r, "b", "c", "");
    let mut start = f.start(&r, "run7", Root::Replied("seed".into()));
    start.max_depth = 1;
    execute(f.hub.clone(), start).await;
    assert_eq!(f.step("run7", "b").status, StepStatus::Succeeded);
    let c = f.step("run7", "c");
    assert_eq!((c.status, c.error.unwrap().contains("at most 1")), (StepStatus::Skipped, true));
    f.shutdown().await;
}

#[tokio::test]
async fn a_link_made_after_a_run_began_does_not_apply_to_it() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "");
    let mut start = f.start(&r, "run8", Root::Replied("seed".into()));
    start.run_at = 0;
    execute(f.hub.clone(), start).await;
    assert_eq!(f.hub.store.flow_run_steps("run8").unwrap().len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn a_restart_fails_the_steps_it_interrupted_and_skips_the_rest() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.link(&r, "a", "b", "");
    f.link(&r, "b", "c", "");
    let step = |bot, status| NewStep {
        run_id: "run9",
        routine_id: &r,
        owner_bot_id: "a",
        bot_id: bot,
        run_at: now_ms(),
        input: "in",
        status,
        output: None,
        error: None,
    };
    f.hub.store.flow_add_step(&step("a", StepStatus::Succeeded)).unwrap();
    f.hub.store.flow_add_step(&step("b", StepStatus::Running)).unwrap();
    let mut events = f.hub.events.subscribe();
    recover(&f.hub).unwrap();
    assert_eq!(
        f.steps("run9"),
        [("a".into(), StepStatus::Succeeded), ("b".into(), StepStatus::Failed), ("c".into(), StepStatus::Skipped)]
    );
    assert!(f.step("run9", "b").error.unwrap().contains("restarted"));
    assert_eq!(events.try_recv().unwrap()["type"], "routineFlow");
    recover(&f.hub).unwrap();
    assert_eq!(f.hub.store.flow_run_steps("run9").unwrap().len(), 3, "recovering twice changes nothing");
    f.shutdown().await;
}

#[tokio::test]
async fn the_api_validates_every_input_and_connects_valid_links() {
    let f = Fixture::new();
    let r = f.routine("a");
    let connect = |from: &str, to: &str| json!({"routineId": r, "fromBotId": from, "toBotId": to, "instruction": "Go"});
    let link = f.call("connectRoutineFlow", connect("a", "b")).await.unwrap();
    assert_eq!(link["link"]["instruction"], "Go");
    let refuse = |body: Value, why: &'static str| {
        let f = &f;
        async move {
            let error = f.call("connectRoutineFlow", body).await.unwrap_err().to_string();
            assert!(error.contains(why), "{error}");
        }
    };
    refuse(connect("a", "b"), "already connected").await;
    refuse(connect("a", "a"), "itself").await;
    refuse(connect("b", "a"), "own bot starts").await;
    refuse(connect("c", "b"), "doesn't reach that bot").await;
    refuse(connect("b", "g"), "group chat").await;
    refuse(connect("b", "x"), "terminal session").await;
    refuse(connect("b", "nobody"), "unknown or hidden").await;
    refuse(json!({"routineId": "nope", "fromBotId": "a", "toBotId": "c"}), "routine not found").await;
    refuse(
        json!({"routineId": r, "fromBotId": "a", "toBotId": "c", "instruction": "x".repeat(MAX_INSTRUCTION_BYTES + 1)}),
        "at most",
    )
    .await;
    refuse(json!({"routineId": r, "fromBotId": "", "toBotId": "c"}), "1 to 200").await;
    f.call("connectRoutineFlow", connect("b", "c")).await.unwrap();
    refuse(connect("c", "b"), "loop").await;
    let id = link["link"]["id"].as_str().unwrap();
    let updated = f.call("updateRoutineFlowLink", json!({"linkId": id, "instruction": "Better"})).await.unwrap();
    assert_eq!(updated["link"]["instruction"], "Better");
    assert!(f.call("updateRoutineFlowLink", json!({"linkId": "nope", "instruction": ""})).await.is_err());
    f.call("disconnectRoutineFlow", json!({"linkId": id})).await.unwrap();
    assert_eq!(f.hub.store.flow_links(&r).unwrap().len(), 1);
    f.shutdown().await;
}

#[tokio::test]
async fn positions_are_validated_and_placed_bots_stay_on_the_canvas() {
    let f = Fixture::new();
    let r = f.routine("a");
    let save = |key: &str, x: Value| json!({"botId": "a", "nodeKey": key, "x": x, "y": 5});
    f.call("saveRoutineFlowPosition", save(&format!("routine:{r}"), json!(10.5))).await.unwrap();
    for (key, x) in [
        ("routine:missing", json!(1)),
        ("agent:a", json!(1)),
        (&format!("routine:{r}:x"), json!(1)),
        (&format!("routine:{r}"), json!(1_000_001)),
        (&format!("routine:{r}"), json!("1")),
    ] {
        assert!(f.call("saveRoutineFlowPosition", save(key, x)).await.is_err(), "{key}");
    }
    f.call("placeRoutineFlowBot", json!({"botId": "a", "placedBotId": "b", "x": -3, "y": 4})).await.unwrap();
    assert!(f.call("placeRoutineFlowBot", json!({"botId": "a", "placedBotId": "g", "x": 0, "y": 0})).await.is_err());
    assert!(f.call("placeRoutineFlowBot", json!({"botId": "a", "placedBotId": "a", "x": 0, "y": 0})).await.is_err());
    let canvas = f.call("routineFlowCanvas", json!({"botId": "a"})).await.unwrap();
    assert_eq!(canvas["placedBotIds"], json!(["b"]));
    assert_eq!(canvas["positions"].as_array().unwrap().len(), 2);
    assert_eq!(canvas["routines"][0]["routine"]["id"], r.as_str());
    assert!(canvas["routines"][0]["upcomingRuns"].as_array().unwrap().len() > 100, "hourly for a week");
    f.call("removeRoutineFlowPosition", json!({"botId": "a", "nodeKey": "bot:b"})).await.unwrap();
    let canvas = f.call("routineFlowCanvas", json!({"botId": "a"})).await.unwrap();
    assert_eq!(canvas["placedBotIds"], json!([]));
    assert!(f.call("routineFlowCanvas", json!({"botId": "g"})).await.is_err(), "a group has no canvas");
    f.shutdown().await;
}

#[tokio::test]
async fn a_canvas_shows_the_flow_on_every_bot_it_passes_through() {
    let f = Fixture::new();
    let r = f.routine("a");
    f.call("connectRoutineFlow", json!({"routineId": r, "fromBotId": "a", "toBotId": "b"})).await.unwrap();
    let canvas = f.call("routineFlowCanvas", json!({"botId": "b"})).await.unwrap();
    assert_eq!(canvas["routines"][0]["routine"]["id"], r.as_str());
    assert_eq!(canvas["links"][0]["toBotId"], "b");
    let other = f.call("routineFlowCanvas", json!({"botId": "c"})).await.unwrap();
    assert_eq!(other["routines"].as_array().unwrap().len(), 0);
    f.shutdown().await;
}

#[tokio::test]
async fn changes_stamp_a_rev_and_reach_the_event_stream_and_catch_up() {
    let f = Fixture::new();
    let r = f.routine("a");
    let mut events = f.hub.events.subscribe();
    let before = f.hub.store.current_rev();
    f.call("connectRoutineFlow", json!({"routineId": r, "fromBotId": "a", "toBotId": "b"})).await.unwrap();
    let event = events.try_recv().unwrap();
    assert_eq!(event["type"], "routineFlow");
    assert!(event["rev"].as_i64().unwrap() > before);
    assert_eq!(event["botIds"], json!(["a", "b"]));
    let caught = catch_up(&f.hub.store, before);
    assert_eq!(caught.len(), 2, "one per canvas that changed");
    assert_eq!(catch_up(&f.hub.store, event["rev"].as_i64().unwrap()).len(), 0);
    f.call("saveRoutineFlowPosition", json!({"botId": "b", "nodeKey": "bot:a", "x": 1, "y": 1})).await.unwrap();
    assert_eq!(events.try_recv().unwrap()["botIds"], json!(["b"]));
    f.shutdown().await;
}

#[tokio::test]
async fn deleting_a_routine_or_a_bot_takes_its_flow_with_it() {
    let f = Fixture::new();
    let (r, r2) = (f.routine("a"), f.routine("a"));
    f.link(&r, "a", "b", "");
    f.link(&r2, "a", "c", "");
    f.hub.store.flow_save_position("a", &format!("routine:{r}"), 1.0, 1.0).unwrap();
    f.hub.store.flow_place_bot("a", "c", 2.0, 2.0).unwrap();
    f.hub
        .store
        .flow_add_step(&NewStep {
            run_id: "run",
            routine_id: &r,
            owner_bot_id: "a",
            bot_id: "b",
            run_at: 1,
            input: "i",
            status: StepStatus::Succeeded,
            output: None,
            error: None,
        })
        .unwrap();
    f.hub.routines.remove(&f.hub, "a", &r).unwrap();
    assert!(f.hub.store.flow_links(&r).unwrap().is_empty() && f.hub.store.flow_run_steps("run").unwrap().is_empty());
    assert_eq!(f.hub.store.flow_positions("a").unwrap().len(), 1, "only the bot's node stays");
    assert_eq!(f.hub.store.flow_links(&r2).unwrap().len(), 1);
    f.hub.delete_bot("c").unwrap();
    assert_eq!(f.hub.store.flow_links(&r2).unwrap().len(), 0);
    assert!(
        f.hub.store.flow_positions("a").unwrap().is_empty() && f.hub.store.flow_placed_bots("a").unwrap().is_empty()
    );
    f.shutdown().await;
}

#[tokio::test]
async fn testing_a_flow_runs_the_links_without_the_routine() {
    let f = Fixture::new();
    let r = f.routine("a");
    let error = f.call("testRoutineFlow", json!({"botId": "a", "routineId": r})).await.unwrap_err();
    assert!(error.to_string().contains("no flow"));
    f.call("connectRoutineFlow", json!({"routineId": r, "fromBotId": "a", "toBotId": "b", "instruction": "Check it"}))
        .await
        .unwrap();
    assert!(f.call("testRoutineFlow", json!({"botId": "b", "routineId": r})).await.is_err(), "only the owner tests");
    let run = f.call("testRoutineFlow", json!({"botId": "a", "routineId": r, "input": "Sample"})).await.unwrap();
    let run = run["runId"].as_str().unwrap().to_owned();
    f.until(|| {
        f.hub.store.flow_run_steps(&run).unwrap().iter().any(|s| s.bot_id == "b" && s.status == StepStatus::Succeeded)
    })
    .await;
    assert!(f.step(&run, "b").input.contains("Sample"));
    f.shutdown().await;
}

#[tokio::test]
async fn a_bot_can_only_change_the_flow_of_its_own_routines_through_its_tools() {
    let f = Fixture::new();
    let r = f.routine("a");
    let args = json!({"routineId": r, "toBotId": "b", "instruction": "Go"});
    assert!(
        tool_call(&f.hub, "c", "link_routine_bots", &args).unwrap_err().to_string().contains("only the bot that owns")
    );
    let linked = tool_call(&f.hub, "a", "link_routine_bots", &args).unwrap();
    assert_eq!(linked["link"]["fromBotId"], "a");
    let listed = tool_call(&f.hub, "a", "list_routine_flow", &json!({})).unwrap();
    assert_eq!(listed["flows"][0]["links"][0]["to"], "b");
    assert!(
        tool_call(&f.hub, "c", "unlink_routine_bots", &json!({"routineId": r, "toBotId": "b", "fromBotId": "a"}))
            .is_err()
    );
    tool_call(&f.hub, "a", "unlink_routine_bots", &json!({"routineId": r, "toBotId": "b"})).unwrap();
    assert_eq!(f.hub.store.flow_links(&r).unwrap().len(), 0);
    assert_eq!(tool_list().len(), 3);
    f.shutdown().await;
}

#[test]
fn remote_devices_get_the_same_access_as_for_routines() {
    use crate::api::devices::permit;
    use crate::store::Scope;
    for method in api::METHODS.iter().chain(&["routines"]) {
        assert!(permit(&Caller::Device { key: "k".into(), scopes: vec![Scope::Control] }, method).is_ok(), "{method}");
        assert!(permit(&Caller::Device { key: "k".into(), scopes: vec![Scope::Screen] }, method).is_err(), "{method}");
        assert!(matches!(permit(&Caller::Pairing { key: "k".into() }, method), Err(Forbidden)), "{method}");
    }
}

#[test]
fn the_configured_depth_stays_within_its_bounds() {
    assert_eq!(max_depth(), graph::DEFAULT_MAX_DEPTH);
}
