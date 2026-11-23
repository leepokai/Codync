use super::*;

async fn descendant_agent() -> (Arc<Acp>, mpsc::UnboundedReceiver<Incoming>) {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/process_agent.py");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let command = format!("{python} -u {}", crate::shell::quote(&script.display().to_string()));
    let (acp, mut rx) = Acp::spawn(&command, env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(15), rx.recv()).await;
    let ready = ready.unwrap();
    assert!(matches!(ready, Some(Incoming::Notification { method, .. }) if method == "ready"));
    (acp, rx)
}

async fn assert_descendant_pipes_close(mut rx: mpsc::UnboundedReceiver<Incoming>) {
    let closed = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
    let closed = closed.unwrap();
    assert!(matches!(closed, Some(Incoming::Closed { .. })));
}

#[tokio::test]
async fn leader_exit_releases_descendants_without_waiting_for_stdout_eof() {
    let (acp, rx) = descendant_agent().await;
    let notified = acp.notify("exit", json!({})).await;
    notified.unwrap();
    assert_descendant_pipes_close(rx).await;
}

#[tokio::test]
async fn dropping_an_agent_releases_its_process_tree() {
    let (acp, rx) = descendant_agent().await;
    drop(acp);
    assert_descendant_pipes_close(rx).await;
}

/// The cleanup of a dropped agent doesn't wait for the runtime's blocking threads, which may all be busy.
#[cfg(unix)]
#[test]
fn a_dropped_agent_is_released_even_when_the_blocking_pool_is_full() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().max_blocking_threads(1).build().unwrap();
    let (acp, rx) = runtime.block_on(descendant_agent());
    let (entered, started) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let blocker = runtime.spawn_blocking(move || {
        entered.send(()).unwrap();
        let _ = gate.recv();
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    runtime.block_on(async {
        drop(acp);
        assert_descendant_pipes_close(rx).await;
    });
    drop(release);
    runtime.block_on(blocker).unwrap();
}

#[tokio::test]
async fn actor_unwinding_releases_its_process_tree() {
    let (acp, rx) = descendant_agent().await;
    let task = tokio::spawn(async move {
        let _owned = acp;
        panic!("simulated actor panic");
    });
    let panicked = task.await;
    assert!(panicked.unwrap_err().is_panic());
    assert_descendant_pipes_close(rx).await;
}

/// #399: Antigravity's Windows server is a `PyInstaller` onefile launcher that removes its ~131 MB `_MEI*` folder only
/// after its agent child ends, which takes it a moment. Stopping it must let the launcher clean up, not kill both at
/// once, also with parents under the agent that outlive what they started.
#[tokio::test]
async fn a_stopped_onefile_agent_leaves_no_unpacked_folder() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/onefile_agent.py");
    let unpacked = std::env::temp_dir().join(format!("codync-_MEI-{}", uuid::Uuid::new_v4()));
    let python = if cfg!(windows) { "python" } else { "python3" };
    let quote = |path: &std::path::Path| crate::shell::quote(&path.display().to_string());
    let command = format!("{python} -u {} {}", quote(&script), quote(&unpacked));
    let (acp, mut rx) = Acp::spawn(&command, env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(15), rx.recv()).await.unwrap();
    assert!(matches!(ready, Some(Incoming::Notification { method, .. }) if method == "ready"));
    assert!(unpacked.is_dir());
    acp.kill().await;
    stopped().await;
    // The launcher's cleanup is the last thing it does; give it a moment past the stop.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while unpacked.exists() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let left = unpacked.exists();
    let _ = std::fs::remove_dir_all(&unpacked);
    assert!(!left, "stopping the agent left its unpacked folder behind");
}

#[tokio::test]
async fn timed_out_requests_do_not_accumulate_pending_answers() {
    let (acp, rx) = descendant_agent().await;
    for _ in 0..10 {
        let response = tokio::time::timeout(Duration::from_millis(10), acp.request("never", json!({}))).await;
        assert!(response.is_err());
        assert_eq!(acp.pending.locked().len(), 0);
    }
    acp.kill().await;
    stopped().await;
    assert_descendant_pipes_close(rx).await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_stopped_agent_that_exits_cleanly_does_not_hold_the_host_for_the_grace_period() {
    let (acp, mut rx) = Acp::spawn("cat", env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
    let started = tokio::time::Instant::now();
    acp.kill().await;
    // Times this agent's own stop: its task takes the process once yielded to, and lets go when the agent has ended.
    // `stopped()` would also wait for other tests' agents (#375).
    tokio::task::yield_now().await;
    drop(acp.child.lock().await);
    assert!(started.elapsed() < EXIT_GRACE / 2, "stopping waited out the grace period");
    assert!(matches!(rx.recv().await, Some(Incoming::Closed { .. })));
}

#[test]
fn content_text_flattens_blocks() {
    let v = json!([
        {"type": "content", "content": {"type": "text", "text": "a"}},
        {"type": "diff", "path": "/x"},
        {"type": "text", "text": "b"}
    ]);
    assert_eq!(content_text(&v), "a\nb");
    assert_eq!(truncate("héllo", 2), "h…");
}

#[tokio::test]
async fn an_error_keeps_the_adapters_category_only_when_it_is_an_identifier() {
    let kind = |data: Value| async move {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Err(json!({"code": -32603, "message": "Internal error", "data": data})));
        let e =
            Answer { id: 0, pending: Pending::default(), rx: Some(rx) }.response().await.expect_err("an error answer");
        e.downcast::<RpcError>().expect("an RpcError").kind
    };
    assert_eq!(kind(json!({"errorKind": "rate_limit"})).await.as_deref(), Some("rate_limit"));
    assert_eq!(kind(json!({"errorKind": "Usage: /Users/me"})).await, None);
    assert_eq!(kind(json!({"errorKind": ""})).await, None);
    assert_eq!(kind(json!("details")).await, None);
    assert_eq!(kind(json!({"codexErrorInfo": "usageLimitExceeded"})).await.as_deref(), Some("usage_limit_exceeded"));
    assert_eq!(
        kind(json!({"codexErrorInfo": {"httpConnectionFailed": {"httpStatusCode": 502}}})).await.as_deref(),
        Some("http_connection_failed")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn stopping_an_agent_closes_its_descendants_pipes() {
    let (acp, mut rx) =
        Acp::spawn(r#"sh -c 'sleep 60 & echo "{\"jsonrpc\":\"2.0\",\"method\":\"ready\"}"; read line'"#, "/tmp", &[])
            .unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap();
    assert!(matches!(ready, Some(Incoming::Notification { .. })));
    acp.kill().await;
    // Killing only the shell leaves sleep holding stdout for 60 seconds.
    let closed = tokio::time::timeout(EXIT_GRACE + Duration::from_secs(3), rx.recv()).await.unwrap();
    assert!(matches!(closed, Some(Incoming::Closed { .. })));
}

#[cfg(windows)]
#[tokio::test]
async fn an_agents_job_lists_what_it_started() {
    let (acp, _rx) = Acp::spawn("ping -n 30 127.0.0.1", ".", &[]).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let listed = acp.job_processes().await.unwrap();
        if let Some(ping) = listed.iter().find(|p| p.name.eq_ignore_ascii_case("ping.exe")) {
            let shell = listed.iter().find(|p| p.pid == ping.parent).expect("what started ping is in the job");
            assert!(shell.name.eq_ignore_ascii_case("cmd.exe"), "{}", shell.name);
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "ping never showed up in the job");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_stopped_agent_finishes_by_itself_first() {
    // The adapter exits on its closed stdin; the CLI it started saves its login a second later.
    let (acp, mut rx) =
        Acp::spawn(r#"sh -c 'read line; (sleep 1; echo "{\"jsonrpc\":\"2.0\",\"method\":\"saved\"}") &'"#, "/tmp", &[])
            .unwrap();
    acp.kill().await;
    let saved = tokio::time::timeout(EXIT_GRACE, rx.recv()).await.unwrap();
    assert!(matches!(saved, Some(Incoming::Notification { method, .. }) if method == "saved"));
    assert!(acp.notify("x", json!({})).await.is_err(), "a stopped agent takes no more messages");
}

#[cfg(unix)]
#[tokio::test]
async fn request_response_roundtrip() {
    // A fake agent: answers `initialize`, then sends a notification and exits.
    let script = r#"read l; id=$(echo "$l" | sed 's/.*"id":\([0-9]*\).*/\1/'); echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"ok\":true}}"; echo '{"jsonrpc":"2.0","method":"session/update","params":{"x":1}}'"#;
    let (acp, mut rx) = Acp::spawn(&format!("sh -c '{}'", script.replace('\'', "'\\''")), "/tmp", &[]).unwrap();
    let res = acp.request("initialize", json!({})).await.unwrap();
    assert_eq!(res["ok"], true);
    match rx.recv().await.unwrap() {
        Incoming::Update(update) => assert_eq!(update.event, crate::events::Event::Other(Value::Null)),
        _ => panic!("expected an update"),
    }
    assert!(matches!(rx.recv().await.unwrap(), Incoming::Closed { .. }));
}

/// A fake agent that asks `method` and then says what it was answered, as a `got` notification.
#[cfg(unix)]
fn asking(method: &str) -> (Arc<Acp>, mpsc::UnboundedReceiver<Incoming>) {
    let ask = format!(r#"{{"jsonrpc":"2.0","id":"r1","method":"{method}","params":{{"sessionId":"s1"}}}}"#);
    let script = format!(r#"echo '{ask}'; read l; echo "{{\"jsonrpc\":\"2.0\",\"method\":\"got\",\"params\":$l}}""#);
    Acp::spawn(&format!("sh -c '{}'", script.replace('\'', "'\\''")), "/tmp", &[]).unwrap()
}

#[cfg(unix)]
async fn answer_to(rx: &mut mpsc::UnboundedReceiver<Incoming>) -> Value {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap() {
            Incoming::Notification { method, params } if method == "got" => return params,
            Incoming::Closed { .. } => panic!("no answer"),
            _ => {}
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_request_dropped_unanswered_answers_by_itself() {
    let (_acp, mut rx) = asking(PERMISSION);
    let Some(Incoming::Request(request)) = rx.recv().await else { panic!("a request") };
    assert_eq!((request.method(), request.session()), (PERMISSION, Some("s1")));
    assert!(request.permission().is_some());
    drop(request);
    let got = answer_to(&mut rx).await;
    assert_eq!((got["id"].clone(), got["result"].clone()), (json!("r1"), json!({"outcome": {"outcome": "cancelled"}})));

    let (_acp, mut rx) = asking("fs/read_text_file");
    let Some(Incoming::Request(request)) = rx.recv().await else { panic!("a request") };
    assert!(request.permission().is_none());
    drop(request);
    assert_eq!(answer_to(&mut rx).await["error"]["code"], METHOD_NOT_FOUND);

    let (_acp, mut rx) = asking(ELICITATION);
    let Some(Incoming::Request(request)) = rx.recv().await else { panic!("a request") };
    drop(request);
    assert_eq!(answer_to(&mut rx).await["result"], json!({"action": "cancel"}));
}

#[cfg(unix)]
#[tokio::test]
async fn a_request_is_answered_once() {
    let (_acp, mut rx) = asking(PERMISSION);
    let Some(Incoming::Request(request)) = rx.recv().await else { panic!("a request") };
    request.choose(Some("yes")).await.unwrap();
    let got = answer_to(&mut rx).await;
    assert_eq!(got["result"], json!({"outcome": {"outcome": "selected", "optionId": "yes"}}));
}

#[cfg(unix)]
#[tokio::test]
async fn a_request_dropped_outside_the_runtime_still_answers() {
    let (_acp, mut rx) = asking(PERMISSION);
    let Some(Incoming::Request(request)) = rx.recv().await else { panic!("a request") };
    std::thread::spawn(move || drop(request)).join().unwrap();
    assert_eq!(answer_to(&mut rx).await["result"], json!({"outcome": {"outcome": "cancelled"}}));
}

#[cfg(unix)]
#[tokio::test]
async fn an_answer_whose_caller_stops_waiting_still_reaches_the_agent() {
    let (acp, mut rx) = asking(PERMISSION);
    let Some(Incoming::Request(request)) = rx.recv().await else { panic!("a request") };
    let busy = acp.writer.0.lock().await;
    let waited = tokio::time::timeout(Duration::from_millis(50), request.choose(Some("yes"))).await;
    assert!(waited.is_err(), "the answer waits for the writer");
    drop(busy);
    let got = answer_to(&mut rx).await;
    assert_eq!(got["result"], json!({"outcome": {"outcome": "selected", "optionId": "yes"}}));
}

#[cfg(unix)]
#[tokio::test]
async fn a_held_request_doesnt_keep_the_agent() {
    let (acp, mut rx) = asking(PERMISSION);
    let Some(Incoming::Request(request)) = rx.recv().await else { panic!("a request") };
    let process = Arc::downgrade(&acp);
    drop(acp);
    assert!(process.upgrade().is_none(), "the request holds no agent");
    assert!(request.respond(json!({})).await.is_err(), "nothing is left to answer");
}

#[test]
fn a_permission_names_its_call_command_and_answers() {
    let request = Permission::parse(&json!({"sessionId": "s1",
        "toolCall": {"toolCallId": "t1", "title": "Run", "kind": "execute", "rawInput": {"command": ["npm", "test"]}},
        "options": [{"optionId": "no", "name": "Reject", "kind": "reject_once"},
            {"optionId": "always", "name": "Always", "kind": "allow_always"},
            {"optionId": "yes", "name": "Allow", "kind": "allow_once"}]}));
    assert_eq!(request.session.as_deref(), Some("s1"));
    assert_eq!(request.tool.title.as_deref(), Some("Run"));
    assert_eq!(request.command.as_deref(), Some("npm test"));
    assert_eq!(request.allow(), Some("yes"));
    let unnamed = Permission::parse(&json!({"toolCall": {"rawInput": {"command": "ls"}},
        "options": [{"optionId": "a"}, {"optionId": "b"}]}));
    assert_eq!((unnamed.command.as_deref(), unnamed.allow()), (Some("ls"), Some("a")));
    assert_eq!(Permission::parse(&json!({})).allow(), None);
}

#[test]
fn a_stop_reason_defaults_to_end_turn_and_keeps_unknown_ones() {
    assert_eq!(StopReason::of(&json!({})), StopReason::EndTurn);
    assert_eq!(StopReason::of(&json!({"stopReason": "cancelled"})), StopReason::Cancelled);
    let other = StopReason::of(&json!({"stopReason": "paused"}));
    assert_eq!((&other, other.as_str()), (&StopReason::Other("paused".into()), "paused"));
}
