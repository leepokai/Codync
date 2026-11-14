//! Voice calls on a real host driving scripted agents: what a call hands the bot, and the replies to it, are the
//! call's while it lasts; a reply to a typed message, or one after the call ended, shows in the chat as usual.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Polls `chat`'s history until an entry matches `pred`, past the ones `seen` already matched.
async fn wait_for(host: &Host, chat: &str, seen: usize, pred: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let history = host.call("history", json!({"botId": chat})).await;
        if let Some(e) = history["entries"].as_array().unwrap().iter().filter(|e| pred(e)).nth(seen) {
            return e.clone();
        }
        assert!(Instant::now() < deadline, "timed out; history: {history}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn pending(e: &Value) -> bool {
    e["kind"] == "permission" && e["data"]["status"] == "pending"
}

fn reply(e: &Value) -> bool {
    e["kind"] == "agent" && e["data"]["final"] == true
}

/// The next thing the computer tells call `call`'s voice, of `kind`.
async fn cue(events: &mut tokio::sync::mpsc::UnboundedReceiver<Value>, call: &str, kind: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let event =
            tokio::time::timeout_at(deadline, events.recv()).await.expect("no cue in time").expect("stream ended");
        if event["type"] == "call" && event["call"] == call && event["kind"] == kind {
            return event["text"].as_str().unwrap().to_owned();
        }
    }
}

async fn tool(host: &Host, call: &str, name: &str, arguments: Value) -> Value {
    host.call("callTool", json!({"callId": call, "name": name, "arguments": arguments})).await["result"].clone()
}

#[tokio::test]
async fn replies_are_the_calls_while_it_lasts() {
    let host = Host::start().await;
    // Each turn asks to edit a file, then answers with how it was approved.
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.py");
    let bot = json!({"name": "Mika", "backend": "custom", "command": common::python_agent(&agent), "permission": "ask",
        "connectors": [], "notify": false});
    let bot = host.call("createBot", bot).await["bot"]["id"].as_str().unwrap().to_owned();
    let mut events = host.events().await;
    host.call("startCall", json!({"botId": bot, "callId": "c1", "mode": "device"})).await;

    // The user's words the app hadn't sent yet ride along with the hand-off.
    let lines = json!([{"from": "user", "text": "改一下 a.txt", "at": 1}]);
    let body = json!({"callId": "c1", "name": "send_to_bot", "arguments": {"text": "Edit a.txt"}, "lines": lines});
    assert_eq!(host.call("callTool", body).await["result"]["sent"], true);
    let relayed = wait_for(&host, &bot, 0, |e| e["kind"] == "user").await;
    assert_eq!((relayed["data"]["call"].as_str(), relayed["data"]["from"].as_str()), (Some("c1"), Some("voice")));
    assert!(relayed["data"]["text"].as_str().unwrap().contains("«改一下 a.txt»"), "{relayed}");

    wait_for(&host, &bot, 0, pending).await;
    // The voice hears of the approval from the computer, worded for an on-device call's listener.
    let approval = cue(&mut events, "c1", "notice").await;
    assert_eq!(approval, "Mika needs your approval in the chat: Edit a.txt");
    // While it waits: a message said on the call and a typed one, answered together.
    host.call("send", json!({"botId": bot, "text": "and b.txt", "clientNonce": "n1", "call": "c1"})).await;
    host.call("send", json!({"botId": bot, "text": "typed", "clientNonce": "n2"})).await;
    let status = tool(&host, "c1", "bot_status", json!({})).await;
    assert_eq!(status["approval"], json!({"request": "Edit a.txt", "options": ["Allow", "Always", "Reject"]}));
    assert_eq!(tool(&host, "c1", "answer_approval", json!({"option": "allow"})).await["answered"], "Allow");
    let first = wait_for(&host, &bot, 0, reply).await;
    assert_eq!(first["data"]["call"], "c1", "a reply to the call is the call's: {first}");
    let said = cue(&mut events, "c1", "reply").await;
    assert_eq!(said, first["data"]["text"].as_str().unwrap().trim(), "the voice hears the reply");

    wait_for(&host, &bot, 0, pending).await;
    assert_eq!(tool(&host, "c1", "answer_approval", json!({"option": "ALLOW"})).await["answered"], "Allow");
    let mixed = wait_for(&host, &bot, 1, reply).await;
    assert!(mixed["data"]["call"].is_null(), "a typed message in the batch shows its reply: {mixed}");
    let said = tool(&host, "c1", "recent_messages", json!({"count": 3})).await;
    assert_eq!(said["messages"][0]["from"], "User", "{said}");

    // The call ends while the bot works on what it was handed last.
    assert_eq!(tool(&host, "c1", "send_to_bot", json!({"text": "Edit a.txt again"})).await["sent"], true);
    let card = wait_for(&host, &bot, 0, pending).await;
    let end =
        json!({"botId": bot, "callId": "c1", "seconds": 42, "lines": [{"from": "voice", "text": "Asking Mika."}]});
    let line = host.call("endCall", end.clone()).await["entry"].clone();
    assert_eq!(line["data"]["transcript"].as_array().map(Vec::len), Some(2), "{line}");
    assert!(host.call("endCall", end).await["entry"].is_null(), "an app trying again ends the call once");
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let after = wait_for(&host, &bot, 2, reply).await;
    assert!(after["data"]["call"].is_null(), "after the call, replies show in the chat: {after}");
    assert!(tool(&host, "c1", "bot_status", json!({})).await["error"].is_string(), "an ended call runs no tools");

    // Its conversation, whatever part of the chat an app holds: the call's messages and replies only.
    let carried = host.call("callEntries", json!({"botId": bot, "callId": "c1"})).await["entries"].clone();
    let kinds: Vec<(&str, &str)> = carried
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["kind"].as_str().unwrap(), e["data"]["call"].as_str().unwrap()))
        .collect();
    assert_eq!(kinds, [("user", "c1"), ("user", "c1"), ("agent", "c1"), ("user", "c1")], "{carried}");
}

#[tokio::test]
async fn a_group_calls_replies_are_the_calls() {
    let host = Host::start().await;
    let member = host.fixture_bot("Mika", "relay_agent.py", "mika").await;
    let group = json!({"name": "Room", "kind": "group", "members": [member]});
    let group = host.call("createBot", group).await["bot"]["id"].as_str().unwrap().to_owned();
    host.call("startCall", json!({"botId": group, "callId": "g1", "mode": "device"})).await;

    assert_eq!(tool(&host, "g1", "send_to_bot", json!({"text": "Who's here?"})).await["sent"], true);
    let answer = wait_for(&host, &group, 0, reply).await;
    assert_eq!((answer["data"]["call"].as_str(), answer["data"]["author"].as_str()), (Some("g1"), Some(&*member)));

    host.call("endCall", json!({"botId": group, "callId": "g1", "seconds": 5})).await;
    host.call("send", json!({"botId": group, "text": "And now?", "clientNonce": "n1"})).await;
    let typed = wait_for(&host, &group, 1, reply).await;
    assert!(typed["data"]["call"].is_null(), "{typed}");
}
