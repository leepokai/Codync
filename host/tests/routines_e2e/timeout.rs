//! Timeout recovery after the agent has finished cold startup.

use super::*;

#[tokio::test]
async fn a_hung_routine_times_out_and_the_next_task_still_runs() {
    let host = Host::start().await;
    let bot = host.stable_bot("timeout").await;
    // Exercise a hung prompt, not cold process startup (which can exceed 2 s on Windows).
    host.send(&bot, "WARMUP").await;
    host.wait_event("timeout", "finished", "WARMUP").await;
    let hung = host.save(&bot, "hung", "BLOCK[forever] IGNORE_CANCEL", json!([{"type":"webhook"}]), 2).await;
    let next = host.save(&bot, "next", "AFTER_TIMEOUT", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":hung})).await;
    host.wait_event("timeout", "started", "BLOCK[forever]").await;
    host.call("runRoutine", json!({"botId":bot,"id":next})).await;
    let failed = host.wait_status(&bot, &hung, "failed").await;
    assert!(failed["detail"].as_str().unwrap().contains("time limit"));
    host.wait_status(&bot, &next, "succeeded").await;
    host.wait_event("timeout", "finished", "AFTER_TIMEOUT").await;
}

#[tokio::test]
async fn a_resume_setup_deadline_retires_the_agent_during_session_close() {
    resume_setup_failure("close").await;
}

#[tokio::test]
async fn a_resume_setup_deadline_retires_the_agent_during_model_selection() {
    resume_setup_failure("model").await;
}

#[tokio::test]
async fn a_rejected_routine_model_closes_its_session_and_keeps_the_agent() {
    resume_setup_failure("close-ack").await;
}

async fn resume_setup_failure(stage: &str) {
    let mut host = Host::start().await;
    let name = "resume-deadline";
    let bot = host.chat_run_bot(name).await;
    host.call("updateBot", json!({"id":bot,"model":"fixture"})).await;
    host.send(&bot, "MAIN_BEFORE_RESUME").await;
    let chat = host.wait_event(name, "finished", "MAIN_BEFORE_RESUME").await;
    let id = host.save(&bot, "resume-deadline", "BLOCK[resume-deadline]", json!([{"type":"webhook"}]), 4).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    let started = host.wait_event(name, "started", "BLOCK[resume-deadline]").await;
    let running = host.wait_status(&bot, &id, "running").await;
    let session = started["session"].as_str().unwrap();
    std::fs::write(common::folder(&host.home, name).join("resume-setup-fault"), format!("{session}\n{stage}\n"))
        .unwrap();

    let recovery = std::time::Instant::now();
    host.restart().await;
    let resumed = host.wait_event(name, "resume_accepted", "").await;
    assert_eq!(resumed["session"], session);
    assert_eq!(resumed["held"], json!([session]));
    let model_event = if stage == "model" { "model_waiting" } else { "model_refused" };
    let model = host.wait_event(name, model_event, "fixture").await;
    assert_eq!(model["session"], session);
    assert_eq!(model["pid"], resumed["pid"]);
    if stage != "model" {
        let event = if stage == "close" { "close_waiting" } else { "closed" };
        let close = host.wait_event(name, event, "").await;
        assert_eq!(close["session"], session);
        assert_eq!(close["pid"], resumed["pid"]);
        assert_eq!(close["held"], if stage == "close" { json!([session]) } else { json!([]) });
    }
    let failed = host.wait_status(&bot, &id, "failed").await;
    let elapsed = recovery.elapsed();
    assert_eq!(failed["id"], running["id"]);
    let detail = failed["detail"].as_str().unwrap();
    let timed_out = stage != "close-ack";
    let expected = if timed_out { "routine session setup timed out" } else { "Fixture rejected the model" };
    assert!(detail.contains(expected), "{failed}");
    assert!(elapsed < Duration::from_secs(15), "{elapsed:?}: {failed}");

    host.send(&bot, "MAIN_AFTER_RESUME_DEADLINE").await;
    let next = host.wait_event(name, "finished", "MAIN_AFTER_RESUME_DEADLINE").await;
    assert_eq!(next["session"], chat["session"]);
    let events = host.agent_events(name);
    assert_eq!(events.iter().filter(|e| e["event"] == "close_waiting").count(), usize::from(stage == "close"));
    assert!(!events.iter().any(|e| e["event"] == "resumed" && e["session"] == session));
    eprintln!(
        "resume setup {stage}: routine {session} on PID {}, {detail} after {elapsed:.2?}, next chat {} on PID {} held {}",
        resumed["pid"], next["session"], next["pid"], next["held"]
    );
    assert_eq!(next["pid"] != resumed["pid"], timed_out);
    assert_eq!(next["held"], json!([next["session"]]));
    if timed_out {
        let exited = host.wait_event(name, "exit", "").await;
        assert_eq!(exited["pid"], resumed["pid"]);
        eprintln!("expired resume process {} exited with ACP input closed", exited["pid"]);
    }
}
