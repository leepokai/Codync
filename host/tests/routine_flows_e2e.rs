//! Routine flows end to end: real host processes with the deterministic team agent. Three bots,
//! a routine, A → B → C, a test of the flow, a failure in B that leaves C skipped, and a host
//! restart in the middle of a run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::time::Duration;

struct Flow {
    host: Host,
    bots: [String; 3],
    routine: String,
}

impl Flow {
    async fn start() -> Self {
        let host = Host::start().await;
        let mut bots = vec![];
        for name in ["Ada", "Ben", "Cy"] {
            bots.push(host.fixture_bot(name, "team_agent.py", "").await);
        }
        let body = json!({"botId": bots[0], "name": "Digest", "instruction": "Make the digest",
            "triggers": [{"type": "interval", "seconds": 3600}]});
        let routine = host.call("saveRoutine", body).await["routine"]["id"].as_str().unwrap().to_owned();
        let bots: [String; 3] = bots.try_into().unwrap();
        Self { host, bots, routine }
    }

    async fn connect(&self, from: usize, to: usize, instruction: &str) -> String {
        let body = json!({"routineId": self.routine, "fromBotId": self.bots[from], "toBotId": self.bots[to],
            "instruction": instruction});
        self.host.call("connectRoutineFlow", body).await["link"]["id"].as_str().unwrap().to_owned()
    }

    async fn canvas(&self) -> Value {
        self.host.call("routineFlowCanvas", json!({"botId": self.bots[0]})).await
    }

    async fn test(&self) -> String {
        let body = json!({"botId": self.bots[0], "routineId": self.routine, "input": "Seven items today"});
        self.host.call("testRoutineFlow", body).await["runId"].as_str().unwrap().to_owned()
    }

    /// Bot index → step status of the newest run, once `done` holds for them.
    async fn steps_when(&self, done: impl Fn(&[Option<String>; 3]) -> bool) -> (Vec<Value>, [Option<String>; 3]) {
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let steps = self.canvas().await["routines"][0]["steps"].as_array().unwrap().clone();
                let status: [Option<String>; 3] = std::array::from_fn(|i| {
                    steps
                        .iter()
                        .find(|s| s["botId"] == self.bots[i].as_str())
                        .and_then(|s| s["status"].as_str().map(str::to_owned))
                });
                if done(&status) {
                    return (steps, status);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("the flow didn't reach the expected steps")
    }
}

fn is(status: &[Option<String>; 3], expected: [&str; 3]) -> bool {
    status.iter().zip(expected).all(|(got, want)| got.as_deref() == Some(want))
}

#[tokio::test]
async fn a_flow_passes_a_reply_from_bot_to_bot() {
    let f = Flow::start().await;
    f.connect(0, 1, "Summarize").await;
    f.connect(1, 2, "Translate").await;
    let run = f.test().await;
    let (steps, _) = f.steps_when(|s| is(s, ["succeeded"; 3])).await;
    assert!(steps.iter().all(|s| s["runId"] == run.as_str()));
    let (b, c) = (&steps[1], &steps[2]);
    assert_eq!(b["output"], "reply: Summarize");
    assert!(c["input"].as_str().unwrap().contains("Ben answered:\n\nreply: Summarize"), "{c}");
    assert!(b["deliveryId"].is_string() && steps[0]["deliveryId"].is_null());
    // The flow reaches the other canvases and the event stream.
    let ben = f.host.call("routineFlowCanvas", json!({"botId": f.bots[1]})).await;
    assert_eq!(ben["routines"][0]["routine"]["id"], f.routine.as_str());
    assert_eq!(ben["links"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_failure_in_the_middle_skips_the_rest_and_a_loop_is_refused() {
    let f = Flow::start().await;
    let first = f.connect(0, 1, "Summarize").await;
    f.connect(1, 2, "Translate").await;
    let refused = f
        .host
        .refused("connectRoutineFlow", json!({"routineId": f.routine, "fromBotId": f.bots[2], "toBotId": f.bots[1]}))
        .await;
    assert!(refused.contains("loop"), "{refused}");
    f.host.call("updateRoutineFlowLink", json!({"linkId": first, "instruction": "FAIL on purpose"})).await;
    f.test().await;
    let (steps, _) = f.steps_when(|s| is(s, ["succeeded", "failed", "skipped"])).await;
    assert!(steps[1]["error"].as_str().is_some_and(|e| !e.is_empty()), "{}", steps[1]);
}

#[tokio::test]
async fn a_restart_in_the_middle_of_a_run_fails_the_running_step() {
    let mut f = Flow::start().await;
    f.connect(0, 1, "BLOCK the work").await;
    f.connect(1, 2, "Never reached").await;
    f.test().await;
    f.steps_when(|s| s[1].as_deref() == Some("running")).await;
    f.host.restart(|_| {}).await;
    let (steps, _) = f.steps_when(|s| is(s, ["succeeded", "failed", "skipped"])).await;
    assert!(steps[1]["error"].as_str().unwrap().contains("restarted"), "{}", steps[1]);
    // A client that was away catches up on the canvas that changed.
    let mut events = f.host.events().await;
    let caught = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            if event["type"] == "routineFlow" && event["botIds"].as_array().unwrap().contains(&json!(f.bots[0])) {
                return event;
            }
        }
        panic!("the event stream ended");
    })
    .await
    .expect("catch-up holds the routineFlow event");
    assert!(caught["rev"].as_i64().unwrap() > 0);
}
