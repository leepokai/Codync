use super::super::tests::Fixture;
use super::*;
use crate::agent::bot::Cmd;
use crate::remote::cloud::Owner;
use std::sync::atomic::Ordering;

async fn call(hub: &Arc<Hub>, user: &str, tool: &str, args: &Value) -> Value {
    answer(hub, &json!({"t": "assistant.call", "id": "c1", "user": user, "tool": tool, "args": args})).await
}

fn refused(v: &Value, why: &str) {
    assert!(v["ok"] == false && v["error"].as_str().unwrap().contains(why), "{v}");
}

/// What a state pull that found this computer in `user_1`'s account leaves.
fn in_account(hub: &Hub) {
    cloud::update_status(hub, |s| s.owner = Some(Owner { user_id: "user_1".into(), email: None }));
}

#[tokio::test]
async fn the_cloud_assistant_needs_the_switch_and_the_account_and_reads_replies_later() {
    let f = Fixture::new();
    let hub = &f.hub;
    refused(&call(hub, "user_1", "list_bots", &json!({})).await, "Cloud assistants are off");
    // Only a computer in an account can turn it on; off always works.
    let turn_on = || set_enabled(hub, true).unwrap_err().to_string();
    assert!(turn_on().contains("isn't connected to the Codync cloud yet"), "no state pull yet: {}", turn_on());
    hub.cloud_synced.store(true, Ordering::Release);
    assert!(turn_on().contains("isn't in your Codync account"), "{}", turn_on());
    assert!(!hub.cloud.status().assistants);
    set_enabled(hub, false).unwrap();
    in_account(hub);
    set_enabled(hub, true).unwrap();
    assert!(hub.cloud.status().assistants);
    refused(&call(hub, "user_2", "list_bots", &json!({})).await, "isn't in your Codync account");

    let listed = call(hub, "user_1", "list_bots", &json!({"query": "b"})).await;
    assert_eq!((&listed["id"], &listed["result"]["bots"][0]["id"]), (&json!("c1"), &json!("b")));
    let sent = call(hub, "user_1", "message_bot", &json!({"botId": "b", "message": "review this"})).await;
    let request = sent["result"]["requestId"].as_str().unwrap().to_owned();
    let read = || hub.assistant.read(ID, Some(&request))["replies"][0].clone();
    f.until(|| read()["status"] == "completed").await;
    assert_eq!((&read()["bot"], &read()["reply"]), (&json!("b"), &json!("reply: review this")));
    // The bot's chat shows who asked.
    let history = hub.store.history("b", i64::MAX, 50).unwrap();
    let notice = history.into_iter().find(|e| e.data["delegationId"] == *request).unwrap();
    assert_eq!(notice.data["botMessage"]["peerName"], NAME);
    refused(
        &call(hub, "user_1", "message_bot", &json!({"botId": "nobody", "message": "hi"})).await,
        "unknown or hidden bot",
    );
    // A computer that left the account answers no one, even with the switch still on.
    cloud::update_status(hub, |s| s.owner = None);
    refused(&call(hub, "user_1", "list_bots", &json!({})).await, "isn't in your Codync account");
    f.shutdown().await;
}

#[tokio::test]
async fn off_works_on_a_private_relay_too() {
    let f = Fixture::new();
    let hub = &f.hub;
    in_account(hub);
    set_enabled(hub, true).unwrap();
    // Moved to a private relay, as the relay restart shows it: no account there.
    hub.store.kv_set("test_cloud_url", "http://127.0.0.1:9").unwrap();
    hub.store.kv_set("test_cloud_token", "s3cret-token-value").unwrap();
    cloud::update_status(hub, |s| cloud::show(s, cloud::current(&hub.store).as_ref()));
    assert!(set_enabled(hub, true).unwrap_err().to_string().contains("private relay"));
    set_enabled(hub, false).unwrap();
    assert!(!enabled(&hub.store));
    f.shutdown().await;
}

#[tokio::test]
async fn read_replies_tells_a_failed_request_from_a_stopped_one() {
    let f = Fixture::new();
    let hub = &f.hub;
    in_account(hub);
    set_enabled(hub, true).unwrap();
    let ask = async |message: &str| {
        let sent = call(hub, "user_1", "message_bot", &json!({"botId": "b", "message": message})).await;
        sent["result"]["requestId"].as_str().unwrap().to_owned()
    };
    let read = |request: &str| hub.assistant.read(ID, Some(request))["replies"][0].clone();

    let failed = ask("FAIL agent error").await;
    f.until(|| read(&failed)["status"] == "failed").await;
    assert!(read(&failed)["detail"].as_str().is_some_and(|d| !d.is_empty()), "{}", read(&failed));
    let stopped = ask("BLOCK long work").await;
    f.until(|| read(&stopped)["status"] == "sent").await;
    hub.send_cmd("b", Cmd::Stop).unwrap();
    f.until(|| read(&stopped)["status"] == "cancelled").await;
    assert_eq!(read(&stopped)["detail"], STOPPED);
    f.shutdown().await;
}

#[tokio::test]
async fn phones_hear_the_switch_through_the_network_view() {
    let f = Fixture::new();
    let hub = &f.hub;
    let mut events = hub.events.subscribe();
    let switched = |events: &mut tokio::sync::broadcast::Receiver<Value>| {
        std::iter::from_fn(|| events.try_recv().ok()).find(|e| e["type"] == "network").expect("a network event")
    };
    set_enabled(hub, false).unwrap();
    let view = &switched(&mut events)["network"];
    assert_eq!(
        (&view["assistants"], &view["assistantsAvailable"]),
        (&json!(false), &json!(false)),
        "not in an account yet"
    );
    in_account(hub);
    set_enabled(hub, true).unwrap();
    let view = &switched(&mut events)["network"];
    assert_eq!((&view["assistants"], &view["assistantsAvailable"]), (&json!(true), &json!(true)));
    f.shutdown().await;
}

#[tokio::test]
async fn a_bot_closed_to_cloud_assistants_is_neither_listed_nor_reached() {
    let f = Fixture::new();
    let hub = &f.hub;
    in_account(hub);
    set_enabled(hub, true).unwrap();
    hub.update_bot(&json!({"id": "b", "reachableByAssistants": false})).unwrap();
    let listed = call(hub, "user_1", "list_bots", &json!({})).await;
    let ids: Vec<&Value> = listed["result"]["bots"].as_array().unwrap().iter().map(|b| &b["id"]).collect();
    assert!(!ids.contains(&&json!("b")), "{listed}");
    refused(
        &call(hub, "user_1", "message_bot", &json!({"botId": "b", "message": "hi"})).await,
        "isn't open to cloud assistants",
    );
    // Nor through another of the user's computers: this computer's switches decide its letters too.
    let bot = |id: &str| hub.store.bot(id).unwrap().unwrap().config;
    assert!(admit_letter(hub, &bot("b")).unwrap_err().to_string().contains("isn't open to cloud assistants"));
    assert!(admit_letter(hub, &bot("c")).is_ok());
    set_enabled(hub, false).unwrap();
    assert!(admit_letter(hub, &bot("c")).unwrap_err().to_string().contains("Cloud assistants are off"));
    f.shutdown().await;
}

#[tokio::test]
async fn an_answer_from_another_computer_waits_in_the_replies() {
    let f = Fixture::new();
    let hub = &f.hub;
    let added = |request: &str| Reply {
        asker: ID.into(),
        request: request.into(),
        bot: "Coder on laptop".into(),
        quote: "status?".into(),
        status: RequestStatus::Sent,
        text: None,
        at: now_ms(),
        limited: true,
    };
    for request in ["l1", "l2"] {
        let _ = hub.assistant.add(added(request));
        remote::awaits_in_replies(hub, request, "Coder on laptop", "status?");
    }
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tokio::spawn(remote::listen(hub.clone(), "network".into(), rx));
    let answer = |request: &str, body: &str, status: &str| codync_network::Message {
        id: format!("re-{request}"),
        from: "coder@laptop".parse().unwrap(),
        to: HANDLE.into(),
        body: body.into(),
        wake: true,
        reply_to: Some(request.into()),
        hops: 1,
        meta: Some(json!({"codync": status})),
        access: Access::Allow,
    };
    tx.send(answer("l1", "all green", "completed")).await.unwrap();
    tx.send(answer("l2", "Couldn't finish: tests fail", "failed")).await.unwrap();
    let read = |request: &str| hub.assistant.read(ID, Some(request))["replies"][0].clone();
    f.until(|| read("l2")["status"] == "failed").await;
    assert_eq!((&read("l1")["status"], &read("l1")["reply"]), (&json!("completed"), &json!("all green")));
    assert_eq!(read("l2")["detail"], "Couldn't finish: tests fail");
    f.shutdown().await;
}

fn session(kind: codync_network::AgentKind, id: &str) -> Caller {
    Caller { kind, session: Some(id.into()), process: "1-test".into(), cwd: "/work/api".into(), hint: None }
}

#[tokio::test]
async fn a_session_on_this_computer_asks_without_the_cloud_and_reads_only_its_own_replies() {
    use codync_network::AgentKind::{ClaudeCode, Codex};
    let f = Fixture::new();
    let hub = &f.hub;
    // No cloud switch or account: it's on the computer, and the cloud scope doesn't apply to it.
    hub.update_bot(&json!({"id": "b", "reachableByAssistants": false})).unwrap();
    let claude = session(ClaudeCode, "s1");
    let listed = local_call(hub, &claude, "list_bots", &json!({})).await.unwrap();
    assert!(listed["bots"].as_array().unwrap().iter().any(|b| b["id"] == "b"), "{listed}");
    assert_eq!(listed["you"], "api-claude", "every result names the caller's address");
    let sent = local_call(hub, &claude, "message_bot", &json!({"botId": "b", "message": "review this"})).await.unwrap();
    let request = sent["requestId"].as_str().unwrap().to_owned();
    let read = |c: &Caller| hub.assistant.read(&format!("session:{}", c.key()), Some(&request))["replies"][0].clone();
    f.until(|| read(&claude)["status"] == "completed").await;
    assert_eq!(read(&claude)["reply"], "reply: review this");
    assert!(read(&session(Codex, "t1")).is_null(), "another session doesn't see it");
    let history = hub.store.history("b", i64::MAX, 50).unwrap();
    let notice = history.into_iter().find(|e| e.data["delegationId"] == *request).unwrap();
    assert_eq!(notice.data["botMessage"]["peerName"], "api-claude (this computer)");
    f.shutdown().await;
}

#[tokio::test]
async fn sessions_mail_each_other_and_read_it_once() {
    use codync_network::AgentKind::{ClaudeCode, Codex};
    let f = Fixture::new();
    let hub = &f.hub;
    let (claude, codex) = (session(ClaudeCode, "s1"), session(Codex, "t1"));
    let me = local_call(hub, &codex, "whoami", &json!({})).await.unwrap();
    assert_eq!((&me["address"], &me["kind"], &me["tier"]), (&json!("api-codex"), &json!("codex"), &json!("pull")));
    let listed = local_call(hub, &claude, "list_bots", &json!({"kind": "codex"})).await.unwrap();
    assert_eq!(listed["bots"][0]["id"], "api-codex", "{listed}");
    let sent =
        local_call(hub, &claude, "message_bot", &json!({"botId": "api-codex", "message": "did the API change?"}))
            .await
            .unwrap();
    assert_eq!((&sent["status"], &sent["tier"]), (&json!("delivered"), &json!("pull")));
    let notice = local_call(hub, &codex, "read_replies", &json!({})).await.unwrap();
    assert_eq!(notice["unread"], 1, "a count, never the text: {notice}");
    assert!(!notice.to_string().contains("did the API"));
    let inbox = local_call(hub, &codex, "check_inbox", &json!({})).await.unwrap();
    assert!(inbox["letters"].as_str().unwrap().contains("from=\"api-claude\""), "{inbox}");
    assert!(local_call(hub, &codex, "whoami", &json!({})).await.unwrap().get("unread").is_none(), "read once taken");
    // A wait ends when a letter comes.
    let wait = json!({"waitSecs": 20});
    let waiting = local_call(hub, &codex, "check_inbox", &wait);
    let later = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        local_call(hub, &claude, "message_bot", &json!({"botId": "api-codex", "message": "again"})).await.unwrap();
    };
    let (inbox, ()) = tokio::join!(waiting, later);
    assert_eq!(inbox.unwrap()["count"], 1);
    // An explicit rename; the old address is never handed out again.
    let renamed = local_call(hub, &codex, "set_name", &json!({"name": "API Reviewer"})).await.unwrap();
    assert_eq!(renamed["address"], "api-reviewer");
    let again = local_call(hub, &codex, "set_name", &json!({"name": "api-reviewer"})).await.unwrap();
    assert_eq!(again["address"], "api-reviewer", "asking again keeps it");
    let next = local_call(hub, &session(Codex, "t2"), "whoami", &json!({})).await.unwrap();
    assert_eq!(next["address"], "api-codex-2");
    f.shutdown().await;
}

#[tokio::test]
async fn the_cloud_assistant_has_no_mailbox() {
    let f = Fixture::new();
    let hub = &f.hub;
    hub.store.kv_set("cloud_assistants", "on").unwrap();
    in_account(hub);
    for tool in ["whoami", "set_name", "check_inbox"] {
        refused(&call(hub, "user_1", tool, &json!({"name": "x"})).await, "unknown tool");
    }
    f.shutdown().await;
}

#[tokio::test]
async fn letters_from_another_computer_wait_in_a_sessions_mailbox() {
    let f = Fixture::new();
    let hub = &f.hub;
    let codex = session(codync_network::AgentKind::Codex, "t1");
    let me = local_call(hub, &codex, "whoami", &json!({})).await.unwrap();
    let address = me["address"].as_str().unwrap().to_owned();
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tokio::spawn(remote::listen(hub.clone(), "network".into(), rx));
    let letter = |from: &str, body: &str| codync_network::Message {
        id: uuid::Uuid::new_v4().to_string(),
        from: from.parse().unwrap(),
        to: address.clone(),
        body: body.into(),
        wake: true,
        reply_to: None,
        hops: 1,
        meta: None,
        access: Access::Allow,
    };
    // Cloud assistants are off here, so the cloud assistant's letter through another computer stays out.
    tx.send(letter(&format!("{HANDLE}@laptop"), "from the cloud")).await.unwrap();
    tx.send(letter("lead@laptop", "review the API")).await.unwrap();
    f.until(|| hub.network.sessions.unread(&hub.store, &codex.key()) == 1).await;
    let inbox = local_call(hub, &codex, "check_inbox", &json!({})).await.unwrap();
    let letters = inbox["letters"].as_str().unwrap();
    assert!(letters.contains("from=\"lead@laptop\"") && letters.contains("review the API"), "{inbox}");
    assert!(!letters.contains("from the cloud"), "{inbox}");
    f.shutdown().await;
}
