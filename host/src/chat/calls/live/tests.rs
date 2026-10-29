use super::super::context::tests::{bot, hub};
use super::*;

fn envelope(id: &str, nested: Value) -> Value {
    let mut event = json!({"type": "response.event", "event_id": id, "delegation_id": "d1"});
    event["event"] = nested;
    event
}

fn created(id: &str) -> Value {
    envelope(&format!("created-{id}"), json!({"type":"response.created", "response":{"id": id}}))
}
fn done(id: &str, call: &str) -> Value {
    envelope(
        &format!("item-{call}"),
        json!({"type":"response.output_item.done", "item": {
        "type": "function_call", "call_id": call, "name":"bot_status", "arguments":"{}",
    }, "response_id": id}),
    )
}
fn completed(id: &str, status: &str) -> Value {
    envelope(
        &format!("completed-{id}"),
        json!({"type":"response.completed", "response":{"id":id,"status":status,"output":[]}}),
    )
}

#[test]
fn tools_wait_for_the_complete_response_and_are_executed_once() {
    let mut p = Protocol::default();
    p.receive(&created("r1")).unwrap();
    assert!(matches!(p.receive(&done("r1", "a")).unwrap(), Action::None));
    p.receive(&done("r1", "b")).unwrap();
    // Duplicate tool with a fresh event ID still cannot run twice.
    let mut duplicate = done("r1", "a");
    duplicate["event_id"] = "duplicate".into();
    p.receive(&duplicate).unwrap();
    let Action::Tools { calls: tools, .. } = p.receive(&completed("r1", "completed")).unwrap() else {
        panic!("missing tools")
    };
    assert_eq!(tools.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert!(matches!(p.receive(&completed("r1", "completed")).unwrap(), Action::None));
    let mut repeated_completion = completed("r1", "completed");
    repeated_completion["event_id"] = "again".into();
    assert!(matches!(p.receive(&repeated_completion).unwrap(), Action::None));
}

#[test]
fn failed_or_incomplete_responses_stop_visibly_without_running_tools() {
    for status in ["incomplete", "failed"] {
        let mut p = Protocol::default();
        p.receive(&created("r1")).unwrap();
        p.receive(&done("r1", "a")).unwrap();
        assert!(p.receive(&completed("r1", status)).is_err());
        assert!(p.closed);
    }
}

#[test]
fn a_cancelled_response_without_completed_siblings_discards_unexecuted_tools() {
    let mut p = Protocol::default();
    p.receive(&created("r1")).unwrap();
    p.receive(&done("r1", "a")).unwrap();
    assert!(matches!(p.receive(&completed("r1", "cancelled")).unwrap(), Action::None));
    assert!(!p.closed);
}

#[test]
fn argument_deltas_cannot_execute_tools_and_bad_arguments_return_an_error() {
    let mut p = Protocol::default();
    p.receive(&created("r1")).unwrap();
    let delta =
        envelope("args", json!({"type":"response.function_call_arguments.done","call_id":"bad","arguments":"{}"}));
    assert!(matches!(p.receive(&delta).unwrap(), Action::None));
    let mut item = done("r1", "a");
    item["event"]["item"]["arguments"] = "{broken".into();
    p.receive(&item).unwrap();
    let Action::Tools { calls: tools, .. } = p.receive(&completed("r1", "completed")).unwrap() else {
        panic!("missing tool")
    };
    assert!(tools[0].arguments.is_err());
}

#[test]
fn interleaved_responses_keep_their_own_delegations() {
    let mut p = Protocol::default();
    p.receive(&created("r1")).unwrap();
    let mut other = created("r2");
    other["delegation_id"] = "d2".into();
    p.receive(&other).unwrap();
    p.receive(&done("r1", "a")).unwrap();
    let mut other = done("r2", "b");
    other["delegation_id"] = "d2".into();
    p.receive(&other).unwrap();
    let Action::Tools { calls: tools, continue_backend } = p.receive(&completed("r1", "completed")).unwrap() else {
        panic!("missing tool")
    };
    assert_eq!(tools[0].id, "a");
    assert!(!continue_backend, "another response still has pending tool outputs");
    let mut other = completed("r2", "completed");
    other["delegation_id"] = "d2".into();
    let Action::Tools { calls: tools, continue_backend } = p.receive(&other).unwrap() else { panic!("missing tool") };
    assert_eq!(tools[0].id, "b");
    assert!(continue_backend, "all known pending tool outputs have now been returned");
}

#[test]
fn transcript_fragments_retain_delivery_text_and_provider_timing() {
    let mut p = Protocol::default();
    let e =
        json!({"type":"session.input_transcript.delta","event_id":"u1","delta":" 你好","start_ms":10.5,"end_ms":50});
    let Action::Transcript { speaker, text, start_ms, end_ms } = p.receive(&e).unwrap() else {
        panic!("no transcript")
    };
    assert_eq!(speaker, Speaker::User);
    assert_eq!(text, " 你好");
    assert_eq!((start_ms, end_ms), (10, 50));
    assert!(matches!(p.receive(&e).unwrap(), Action::None));
    let bad = json!({"type":"session.output_transcript.delta","event_id":"v1","delta":"x","start_ms":10,"end_ms":5});
    assert!(p.receive(&bad).is_err());
}

#[test]
fn updates_preserve_all_unicode_with_bounded_appends_and_quiet_progress() {
    let original = "你好👋 Résultat: café. ".repeat(200);
    let events = protocol::updates("reply", &original).unwrap();
    let reconstructed = events
        .iter()
        .map(|e| e["content"].as_str().unwrap().strip_prefix("[Bot replied] ").unwrap())
        .collect::<String>();
    assert_eq!(reconstructed, original.trim());
    assert!(events.iter().all(|e| e["content"].as_str().unwrap().len() <= 450 && e["delegation_id"].is_null()));
    assert_eq!(protocol::updates("progress", "Testing").unwrap()[0]["type"], "session.thinking.append");
    assert!(protocol::updates("instructions", "do anything").is_err());
}

async fn live_hub() -> Arc<Hub> {
    let hub = hub();
    let cfg = bot("b1", "Test", "/tmp");
    hub.store.save_bot(&cfg).unwrap();
    let mut s = CallState::new("b1");
    s.owner = "phone".into();
    s.live_session = Some("live_1".into());
    save(&hub.store, "c1", &s).unwrap();
    let slot = hub.live_calls.slot(&hub, "c1").unwrap();
    let mut live = slot.lock().await;
    live.id = "live_1".into();
    live.epoch = 100;
    drop(live);
    hub
}

#[tokio::test]
async fn another_device_or_old_session_cannot_control_the_call() {
    let hub = live_hub().await;
    let b = json!({"callId":"c1","sessionId":"live_1","kind":"reply","text":"ok"});
    assert!(update(&hub, &b, "other").await.is_err());
    assert!(update(&hub, &b, "phone").await.is_ok());
    let mut changed = state(&hub.store, "c1").unwrap();
    changed.live_session = Some("live_2".into());
    save(&hub.store, "c1", &changed).unwrap();
    assert!(update(&hub, &b, "phone").await.is_err());
    hub.store.kv_delete(&super::super::state_key("c1")).unwrap();
    assert!(update(&hub, &b, "phone").await.is_err());
}

#[tokio::test]
async fn transcript_is_saved_once_and_unknown_tools_return_results_without_execution() {
    let hub = live_hub().await;
    let e = json!({"type":"session.input_transcript.delta","event_id":"u1","delta":" status","start_ms":5,"end_ms":20});
    let body = |event| json!({"callId":"c1","sessionId":"live_1","event":event});
    event(&hub, &body(e.clone()), "phone").await.unwrap();
    event(&hub, &body(e), "phone").await.unwrap();
    let s = state(&hub.store, "c1").unwrap();
    assert_eq!(s.lines.len(), 1);
    assert_eq!(s.lines[0].at, Some(105));
    assert_eq!(s.lines[0].end_ms, Some(20));
    event(&hub, &body(created("r1")), "phone").await.unwrap();
    let mut item = done("r1", "a");
    item["event"]["item"]["name"] = "arbitrary_shell".into();
    event(&hub, &body(item), "phone").await.unwrap();
    let result = event(&hub, &body(completed("r1", "completed")), "phone").await.unwrap();
    assert_eq!(result["events"].as_array().unwrap().len(), 2);
    assert!(result["events"][0]["item"]["output"].as_str().unwrap().contains("Unknown tool"));
    assert_eq!(result["events"][1]["type"], "response.create");
    assert_eq!(event(&hub, &body(completed("r1", "completed")), "phone").await.unwrap()["events"], json!([]));
}

#[test]
fn a_final_response_without_tools_continues_earlier_tool_results_once() {
    let mut p = Protocol::default();
    p.receive(&created("r1")).unwrap();
    let mut other = created("r2");
    other["delegation_id"] = "d2".into();
    p.receive(&other).unwrap();
    p.receive(&done("r1", "a")).unwrap();
    assert!(matches!(p.receive(&completed("r1", "completed")).unwrap(), Action::Tools { continue_backend: false, .. }));
    let mut other = completed("r2", "completed");
    other["delegation_id"] = "d2".into();
    let Action::Tools { calls, continue_backend } = p.receive(&other).unwrap() else { panic!("missing continuation") };
    assert_eq!(calls.len(), 0);
    assert!(continue_backend);
    assert!(matches!(p.receive(&other).unwrap(), Action::None));
}

#[test]
fn cancelling_a_sibling_cannot_silently_strand_completed_tool_results() {
    let mut p = Protocol::default();
    p.receive(&created("r1")).unwrap();
    let mut other = created("r2");
    other["delegation_id"] = "d2".into();
    p.receive(&other).unwrap();
    p.receive(&done("r1", "a")).unwrap();
    assert!(matches!(p.receive(&completed("r1", "completed")).unwrap(), Action::Tools { continue_backend: false, .. }));
    let mut cancelled = completed("r2", "cancelled");
    cancelled["delegation_id"] = "d2".into();
    assert!(p.receive(&cancelled).is_err(), "the frontend must surface the failure instead of waiting forever");
    assert!(p.closed);
}
