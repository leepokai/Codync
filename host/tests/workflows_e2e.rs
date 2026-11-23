//! Workflow create, save, conflict and a run that does not need an agent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn graph() -> Value {
    let raw = include_str!("fixtures/workflow-graph.json");
    serde_json::from_str(raw).unwrap()
}

#[tokio::test]
async fn a_manual_run_finishes_and_a_stale_save_conflicts() {
    let host = common::host::Host::start().await;
    let created = host.call("createWorkflow", json!({"title": "Morning"})).await;
    let id = created["workflow"]["id"].as_str().unwrap().to_owned();
    assert_eq!(created["workflow"]["revision"], 1);
    assert_eq!(created["workflow"]["enabled"], true);
    assert!(created["workflow"]["graph"]["nodes"].as_array().unwrap().iter().any(|n| n["kind"] == "trigger"));

    let saved = host.call("saveWorkflowGraph", json!({"id": id, "baseRevision": 1, "graph": graph()})).await;
    assert_eq!(saved["ok"], true);
    let revision = saved["revision"].as_i64().unwrap();

    let stale = host.call("saveWorkflowGraph", json!({"id": id, "baseRevision": 1, "graph": graph()})).await;
    assert_eq!(stale["ok"], false);
    assert_eq!(stale["conflict"], true);
    assert_eq!(stale["workflow"]["revision"], revision);

    let empty = host.call("createWorkflow", json!({"title": "Empty", "graph": {"nodes": [], "edges": []}})).await;
    let empty_id = empty["workflow"]["id"].as_str().unwrap();
    let refused = host.refused("startWorkflowRun", json!({"id": empty_id, "requestId": "empty-1"})).await;
    assert!(refused.contains("Add a trigger first"), "{refused}");

    let run = host.call("startWorkflowRun", json!({"id": id, "requestId": "run-1"})).await;
    let run_id = run["run"]["id"].as_str().unwrap().to_owned();
    let finished = wait_done(&host, &run_id).await;
    assert_eq!(finished["run"]["status"], "done", "{finished}");
    let steps = finished["run"]["steps"].as_array().unwrap();
    assert!(steps.iter().any(|s| s["kind"] == "prompt" && s["status"] == "done"));
    assert!(steps.iter().any(|s| s["kind"] == "output" && s["status"] == "done"));

    let again = host.call("startWorkflowRun", json!({"id": id, "requestId": "run-1"})).await;
    assert_eq!(again["run"]["id"], run_id, "the same request id returns the same run");

    host.call("setWorkflowEnabled", json!({"id": id, "enabled": false})).await;
    let manual = host.call("startWorkflowRun", json!({"id": id, "requestId": "run-2"})).await;
    assert_ne!(manual["run"]["id"], run_id);
    let _ = wait_done(&host, manual["run"]["id"].as_str().unwrap()).await;

    let hello = host.call("hello", json!({})).await;
    assert_eq!(hello["workflows"], true);
}

#[tokio::test]
async fn an_approval_waits_until_someone_decides() {
    let host = common::host::Host::start().await;
    let created = host.call("createWorkflow", json!({"title": "Check"})).await;
    let id = created["workflow"]["id"].as_str().unwrap();
    let graph = json!({
        "nodes": [
            {"id": "t", "kind": "trigger", "at": {"x": 80, "y": 80}, "fields": {"trigger": "manual"}},
            {"id": "a", "kind": "approval", "at": {"x": 480, "y": 80}, "fields": {"question": "Ship it?"}}
        ],
        "edges": [{"id": "t>a:text", "from": "t", "to": "a", "type": "text"}]
    });
    host.call("saveWorkflowGraph", json!({"id": id, "baseRevision": 1, "graph": graph})).await;
    let run = host.call("startWorkflowRun", json!({"id": id, "requestId": "approve-1"})).await;
    let run_id = run["run"]["id"].as_str().unwrap();
    let waiting = wait_status(&host, run_id, "approval").await;
    let step = waiting["run"]["steps"].as_array().unwrap().iter().find(|s| s["kind"] == "approval").unwrap();
    host.call("decideWorkflowStep", json!({"stepId": step["id"], "approved": true})).await;
    let done = wait_done(&host, run_id).await;
    assert_eq!(done["run"]["status"], "done", "{done}");
}

#[tokio::test]
async fn a_manual_run_starts_from_the_steps_when_there_is_no_trigger() {
    let host = common::host::Host::start().await;
    let created = host.call("createWorkflow", json!({"title": "Plain", "graph": {"nodes": [], "edges": []}})).await;
    let id = created["workflow"]["id"].as_str().unwrap();
    let graph = json!({
        "nodes": [
            {"id": "p", "kind": "prompt", "at": {"x": 80, "y": 80}, "fields": {"text": "hello"}},
            {"id": "o", "kind": "output", "at": {"x": 400, "y": 80}, "fields": {"target": "notify"}}
        ],
        "edges": [{"id": "p>o:text", "from": "p", "to": "o", "type": "text"}]
    });
    host.call("saveWorkflowGraph", json!({"id": id, "baseRevision": 1, "graph": graph})).await;
    let run = host.call("startWorkflowRun", json!({"id": id, "requestId": "plain-1"})).await;
    let done = wait_done(&host, run["run"]["id"].as_str().unwrap()).await;
    assert_eq!(done["run"]["status"], "done", "{done}");
    let output = done["run"]["steps"].as_array().unwrap().iter().find(|s| s["kind"] == "output").unwrap();
    assert_eq!(output["status"], "done");
    assert_eq!(output["output"], "hello");
}

#[tokio::test]
async fn a_script_notifies_and_a_graph_save_cannot_add_a_step() {
    let host = common::host::Host::start().await;
    let script = "export const meta = { name: \"Ping\", description: \"\", phases: [{ title: \"Send\" }] }\nexport default async function ({ output, args, phase }) {\n  phase(\"Send\")\n  await output({ target: \"notify\", text: args.input })\n  return \"ok\"\n}\n";
    let created = host.call("createWorkflow", json!({"title": "Ping", "script": script})).await;
    let id = created["workflow"]["id"].as_str().unwrap();
    assert_eq!(created["workflow"]["source"], "script");
    assert_eq!(created["workflow"]["editMode"], "script");
    let run = host.call("startWorkflowRun", json!({"id": id, "requestId": "script-1", "input": "hello"})).await;
    let done = wait_done(&host, run["run"]["id"].as_str().unwrap()).await;
    assert_eq!(done["run"]["status"], "done", "{done}");
    assert_eq!(done["run"]["output"], "ok");
    let graph = created["workflow"]["graph"].clone();
    let mut nodes = graph["nodes"].as_array().unwrap().clone();
    nodes
        .push(json!({"id": "a", "kind": "agent", "at": {"x": 480, "y": 80}, "fields": {"botId": "bot", "task": "Hi"}}));
    let refused = host
        .refused(
            "saveWorkflowGraph",
            json!({"id": id, "baseRevision": created["workflow"]["revision"], "graph": {"nodes": nodes, "edges": []}}),
        )
        .await;
    assert!(refused.contains("This workflow is a script"), "{refused}");
}

async fn wait_done(host: &common::host::Host, run_id: &str) -> Value {
    wait_status(host, run_id, "done").await
}

async fn wait_status(host: &common::host::Host, run_id: &str, status: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let detail = host.call("workflowRun", json!({"runId": run_id})).await;
        if detail["run"]["status"] == status {
            return detail;
        }
        assert!(Instant::now() < deadline, "run stayed {}: {detail}", detail["run"]["status"]);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
