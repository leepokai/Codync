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
