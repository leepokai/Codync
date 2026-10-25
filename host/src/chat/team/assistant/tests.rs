use super::super::tests::Fixture;
use super::*;
use crate::agent::bot::Cmd;
use crate::remote::cloud::Owner;

fn call(hub: &Arc<Hub>, user: &str, tool: &str, args: &Value) -> Value {
    answer(hub, &json!({"t": "assistant.call", "id": "c1", "user": user, "tool": tool, "args": args}))
}

fn refused(v: &Value, why: &str) {
    assert!(v["ok"] == false && v["error"].as_str().unwrap().contains(why), "{v}");
}

#[tokio::test]
async fn the_cloud_assistant_needs_the_switch_and_the_account_and_reads_replies_later() {
    let f = Fixture::new();
    let hub = &f.hub;
    refused(&call(hub, "user_1", "list_bots", &json!({})), "Cloud assistants are off");
    set_enabled(hub, true).unwrap();
    assert!(hub.cloud.status().assistants);
    refused(&call(hub, "user_1", "list_bots", &json!({})), "isn't in your Codync account");
    cloud::update_status(hub, |s| s.owner = Some(Owner { user_id: "user_1".into(), email: None }));
    refused(&call(hub, "user_2", "list_bots", &json!({})), "isn't in your Codync account");

    let listed = call(hub, "user_1", "list_bots", &json!({"query": "b"}));
    assert_eq!((&listed["id"], &listed["result"]["bots"][0]["id"]), (&json!("c1"), &json!("b")));
    let sent = call(hub, "user_1", "message_bot", &json!({"botId": "b", "message": "review this"}));
    let request = sent["result"]["requestId"].as_str().unwrap().to_owned();
    let read = || hub.assistant.read(Some(&request))["replies"][0].clone();
    f.until(|| read()["status"] == "completed").await;
    assert_eq!((&read()["bot"], &read()["reply"]), (&json!("b"), &json!("reply: review this")));
    // The bot's chat shows who asked.
    let history = hub.store.history("b", i64::MAX, 50).unwrap();
    let notice = history.into_iter().find(|e| e.data["delegationId"] == *request).unwrap();
    assert_eq!(notice.data["botMessage"]["peerName"], NAME);
    refused(&call(hub, "user_1", "message_bot", &json!({"botId": "nobody", "message": "hi"})), "unknown or hidden bot");
    f.shutdown().await;
}

#[tokio::test]
async fn read_replies_tells_a_failed_request_from_a_stopped_one() {
    let f = Fixture::new();
    let hub = &f.hub;
    set_enabled(hub, true).unwrap();
    cloud::update_status(hub, |s| s.owner = Some(Owner { user_id: "user_1".into(), email: None }));
    let ask = |message: &str| {
        let sent = call(hub, "user_1", "message_bot", &json!({"botId": "b", "message": message}));
        sent["result"]["requestId"].as_str().unwrap().to_owned()
    };
    let read = |request: &str| hub.assistant.read(Some(request))["replies"][0].clone();

    let failed = ask("FAIL agent error");
    f.until(|| read(&failed)["status"] == "failed").await;
    assert!(read(&failed)["detail"].as_str().is_some_and(|d| !d.is_empty()), "{}", read(&failed));
    let stopped = ask("BLOCK long work");
    f.until(|| read(&stopped)["status"] == "sent").await;
    hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| read(&stopped)["status"] == "cancelled").await;
    assert_eq!(read(&stopped)["detail"], STOPPED);
    f.shutdown().await;
}
