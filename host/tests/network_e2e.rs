//! Two real hosts on this machine in one network (direct connections, no relay, no internet),
//! with deterministic ACP agents: a bot on one computer finds and messages a bot on the other,
//! that computer's user approves its tool call, and the reply comes back as the sender's turn.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Child;
use std::time::Duration;

struct Host {
    child: Child,
    home: PathBuf,
    base: String,
    token: String,
}

impl Drop for Host {
    fn drop(&mut self) {
        common::stop(&mut self.child);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

impl Host {
    async fn start() -> Self {
        let home = std::env::temp_dir().join(format!("codync-network-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let (child, base) = common::serve(&home).await;
        let token = std::fs::read_to_string(home.join("token")).unwrap().trim().to_owned();
        Self { child, home, base, token }
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .timeout(Duration::from_mins(2))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert!(status.is_success(), "{method}: {value}");
        value
    }

    async fn bot(&self, name: &str) -> String {
        let cwd = self.home.join(name);
        std::fs::create_dir_all(&cwd).unwrap();
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        let result = self
            .call(
                "createBot",
                json!({
                    "name": name, "backend": "custom", "cwd": cwd, "permission": "ask", "notify": false,
                    "command": common::python_agent(&agent),
                }),
            )
            .await;
        result["bot"]["id"].as_str().unwrap().to_owned()
    }

    async fn wait_for(&self, bot: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                let history = self.call("history", json!({"botId": bot})).await;
                if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| predicate(e)) {
                    return entry.clone();
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("the network flow did not finish")
    }
}

fn is_final(e: &Value) -> bool {
    e["kind"] == "agent" && e["data"]["final"] == true
}

/// An exchange notice that ended with `status`.
fn exchange(status: &'static str) -> impl Fn(&Value) -> bool {
    move |e| e["kind"] == "notice" && e["data"]["botMessage"].is_object() && e["data"]["status"] == status
}

/// Two hosts in one network: desk (the owner) with a Reviewer bot, laptop with a Lead bot.
async fn two_computers() -> (Host, Host, String, String) {
    let (desk, laptop) = (Host::start().await, Host::start().await);
    assert_eq!(desk.call("networkStatus", json!({})).await["joined"], false);
    let created = desk.call("networkCreate", json!({"name": "desk", "connector": "private"})).await;
    assert_eq!(created["computer"], "desk");
    assert!(created["recoveryCode"].as_str().unwrap().len() > 60);
    let invite = desk.call("networkInvite", json!({})).await["invite"].as_str().unwrap().to_owned();
    let joined = laptop.call("networkJoin", json!({"invite": invite, "name": "laptop", "connector": "private"})).await;
    let home = &joined["networks"][0];
    assert_eq!((home["computer"].as_str(), home["role"].as_str()), (Some("laptop"), Some("member")));
    let reviewer = desk.bot("Reviewer").await;
    let lead = laptop.bot("Lead").await;
    (desk, laptop, reviewer, lead)
}

#[tokio::test]
async fn a_bot_messages_a_bot_on_another_computer_and_relays_its_reply() {
    let (desk, laptop, reviewer, lead) = two_computers().await;
    let peers = laptop.call("networkPeers", json!({})).await;
    let desk_row = computer(&peers, "desk").clone();
    assert_eq!((desk_row["online"].as_bool(), desk_row["role"].as_str()), (Some(true), Some("owner")));
    // Every visible bot goes by its address name (the host's own lead bot too).
    assert!(desk_row["agents"].as_array().unwrap().iter().any(|a| a["id"] == "reviewer"), "{desk_row}");

    // Listings say what kind of agent each one is, and can pick one kind.
    let list =
        |kind: &str| json!({"botId": lead, "name": "list_bots", "arguments": {"kind": kind, "computer": "desk"}});
    let bots = laptop.call("teamCall", list("codync")).await;
    let reviewer_row = bots["bots"].as_array().unwrap().iter().find(|b| b["id"] == "reviewer@desk").cloned().unwrap();
    assert_eq!(reviewer_row["kind"], "codync");
    assert_eq!(laptop.call("teamCall", list("codex")).await["total"], 0);

    // The lead lists the reviewer by its address and messages it; its turn ends at once.
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk"})).await;
    let permission = desk.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    let queued = laptop.wait_for(&lead, is_final).await;
    assert_eq!(queued["data"]["text"], "Message queued");
    // The desk's user approves the reviewer's tool call; the reply travels back.
    desk.call("respondPermission", json!({"entryId": permission["id"], "optionId": "allow"})).await;
    let relayed = laptop.wait_for(&lead, |e| is_final(e) && e["data"]["text"] != "Message queued").await;
    assert_eq!(relayed["data"]["text"], "Relayed: reply: PERMISSION review the changes");

    // Each side's exchange notice ends completed, naming the bot on the other computer.
    let sent = laptop.wait_for(&lead, exchange("completed")).await;
    assert_eq!(sent["data"]["botMessage"]["targetBotId"], "reviewer@desk");
    assert_eq!(sent["data"]["botMessage"]["peerName"], "Reviewer on desk");
    assert_eq!(sent["data"]["botMessage"]["reply"], "reply: PERMISSION review the changes");
    let taken = desk.wait_for(&reviewer, exchange("completed")).await;
    assert_eq!(taken["data"]["botMessage"]["sourceBotId"], "lead@laptop");
    assert_eq!(taken["data"]["botMessage"]["peerName"], "Lead on laptop");

    // The pair's history is there for the conversation sheet on either side.
    let history = laptop.call("botConversation", json!({"botId": lead, "peerId": "reviewer@desk"})).await;
    assert_eq!(history["entries"].as_array().unwrap().len(), 1);
    let status = laptop.call("networkStatus", json!({})).await;
    assert_eq!(status["networks"][0]["computers"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_stopped_or_failed_request_ends_the_senders_notice() {
    let (desk, laptop, reviewer, lead) = two_computers().await;
    // The desk's user stops the reviewer while it waits for approval: the lead's notice ends
    // cancelled, and nothing wakes the lead.
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk PERMISSION then stop"})).await;
    desk.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    desk.call("stop", json!({"botId": reviewer})).await;
    let stopped = laptop.wait_for(&lead, exchange("cancelled")).await;
    assert!(stopped["data"]["botMessage"]["detail"].as_str().unwrap().starts_with("Reviewer on desk was stopped"));

    // A turn that fails comes back as a failure, in a reply turn.
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk FAIL on purpose"})).await;
    laptop.wait_for(&lead, exchange("failed")).await;
    let relayed = laptop.wait_for(&lead, |e| is_final(e) && e["data"]["text"] != "Message queued").await;
    assert_eq!(relayed["data"]["text"], "Relayed: failed");
    let history = laptop.call("history", json!({"botId": lead})).await;
    let finals = history["entries"].as_array().unwrap().iter().filter(|e| is_final(e)).count();
    assert_eq!(finals, 3, "two requests and one reply turn: {history}");
}

/// The first `network` event on the host's stream that `predicate` accepts. The stream is open
/// once this returns its future's first poll; the caller gives it a moment before changing things.
async fn network_event(base: String, token: String, predicate: impl Fn(&Value) -> bool) -> Value {
    use futures::StreamExt as _;
    let res = reqwest::Client::new()
        .get(format!("{base}/events?since={}&client=test", i64::MAX))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    let mut body = res.bytes_stream();
    let mut buf = String::new();
    tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(chunk)) = body.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(i) = buf.find('\n') {
                let line: String = buf.drain(..=i).collect();
                let event = line.trim_end().strip_prefix("data:").and_then(|d| serde_json::from_str::<Value>(d).ok());
                if let Some(event) = event.filter(|e| e["type"] == "network" && predicate(&e["network"])) {
                    return event["network"].clone();
                }
            }
        }
        panic!("the event stream ended");
    })
    .await
    .expect("no network event")
}

/// A computer in the view's first network.
fn computer<'a>(view: &'a Value, name: &str) -> &'a Value {
    view["networks"][0]["computers"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()
}

#[tokio::test]
async fn the_network_view_says_who_may_do_what_and_changes_reach_the_apps() {
    let (desk, laptop, _reviewer, _lead) = two_computers().await;
    desk.call("networkPeers", json!({})).await;
    let view = desk.call("networkView", json!({})).await;
    assert_eq!((view["joined"].as_bool(), view["assistants"].as_bool()), (Some(true), Some(false)));
    let home = &view["networks"][0];
    assert_eq!((&home["canInvite"], &home["canRecover"]), (&json!(true), &json!(false)), "the owner invites");
    let row = computer(&view, "laptop");
    assert_eq!((row["canRemove"].as_bool(), row["canPromote"].as_bool()), (Some(true), Some(true)));
    assert!(row["agents"].as_array().unwrap().iter().any(|a| a["name"] == "Lead"), "{row}");
    assert_eq!(computer(&view, "desk")["canRemove"], false, "not itself");
    let invite = desk.call("networkInvite", json!({})).await;
    let in_an_hour = chrono::Utc::now().timestamp_millis() + 3_500_000;
    assert!(invite["expiresAt"].as_i64().unwrap() > in_an_hour, "{invite}");
    // A member may do nothing to the owner.
    let theirs = laptop.call("networkView", json!({})).await;
    assert_eq!(
        (computer(&theirs, "desk")["canRemove"].as_bool(), theirs["networks"][0]["role"].as_str()),
        (Some(false), Some("member"))
    );
    let home = &theirs["networks"][0];
    assert_eq!((&home["canInvite"], &home["canRecover"]), (&json!(false), &json!(true)));
    let key = &view["networks"][0]["key"];
    assert!(key.is_string() && *key == home["key"], "one key on every computer: {key} {}", home["key"]);

    // The owner promotes the laptop: both computers' apps hear of it without asking.
    let there =
        tokio::spawn(network_event(laptop.base.clone(), laptop.token.clone(), |v| v["networks"][0]["role"] == "admin"));
    let here = tokio::spawn(network_event(desk.base.clone(), desk.token.clone(), |v| {
        computer(v, "laptop")["role"] == "admin"
    }));
    tokio::time::sleep(Duration::from_millis(500)).await;
    desk.call("networkPromote", json!({"computer": "laptop"})).await;
    let there = there.await.unwrap();
    assert_eq!(computer(&there, "desk")["canRemove"], false, "an admin doesn't remove the owner");
    assert_eq!(computer(&here.await.unwrap(), "laptop")["canPromote"], false, "already an admin");
}

#[tokio::test]
async fn a_bot_kept_off_the_network_and_a_deny_rule_keep_letters_out() {
    let (desk, laptop, reviewer, lead) = two_computers().await;
    let secret = desk.bot("Secret").await;
    desk.call("updateBot", json!({"id": secret, "offeredToNetwork": false})).await;
    let peers = laptop.call("networkPeers", json!({})).await;
    let offered: Vec<&Value> =
        computer(&peers, "desk")["agents"].as_array().unwrap().iter().map(|a| &a["id"]).collect();
    assert!(offered.contains(&&json!("reviewer")), "{offered:?}");
    assert!(!offered.contains(&&json!("secret")), "kept off the network: {offered:?}");

    // The desk takes nothing from the laptop's bots; the lead's message_bot says why.
    let rules = json!([{"fromComputer": "laptop", "fromAgent": "*", "toAgent": "*", "access": "deny"}]);
    let view = desk.call("setNetworkRules", json!({"rules": rules})).await;
    assert_eq!(view["networks"][0]["rules"][0]["access"], "deny");
    laptop.call("send", json!({"botId": lead, "text": "TRY reviewer@desk review this"})).await;
    let refused = laptop.wait_for(&lead, is_final).await;
    let text = refused["data"]["text"].as_str().unwrap();
    assert!(
        text.starts_with("Refused: ") && text.contains("reviewer doesn't take messages from lead@laptop"),
        "{text}"
    );
    let failed = laptop.wait_for(&lead, exchange("failed")).await;
    assert!(failed["data"]["botMessage"]["detail"].as_str().unwrap().contains("doesn't take messages"), "{failed}");
    let history = desk.call("history", json!({"botId": reviewer})).await;
    assert!(!history.to_string().contains("review this"), "nothing reached the reviewer");

    // Kept off the network, Secret still asks the laptop's lead; the answer gets past the deny rule.
    desk.call("send", json!({"botId": secret, "text": "TRY lead@laptop status?"})).await;
    let answered = desk.wait_for(&secret, exchange("completed")).await;
    assert_eq!(answered["data"]["botMessage"]["reply"], "reply: status?", "{answered}");
}

#[tokio::test]
async fn a_computer_in_two_networks_keeps_them_apart() {
    let (desk, laptop, _reviewer, lead) = two_computers().await;
    let server = Host::start().await;
    // The desk makes a second network for the server; the invite carries its name.
    let work = desk.call("networkCreate", json!({"network": "work", "name": "desk", "connector": "private"})).await;
    let invite = desk.call("networkInvite", json!({"network": "work"})).await["invite"].as_str().unwrap().to_owned();
    let joined = server.call("networkJoin", json!({"invite": invite, "name": "server", "connector": "private"})).await;
    assert_eq!(joined["networks"][0]["name"], "work");
    let tester = server.bot("Tester").await;
    let status = desk.call("networkPeers", json!({})).await;
    let names: Vec<&str> = status["networks"].as_array().unwrap().iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["home", "work"]);
    // The laptop is in one network: it never sees the server.
    let laptops = laptop.call("networkPeers", json!({})).await;
    assert_eq!(laptops["networks"].as_array().unwrap().len(), 1);
    assert!(!laptops.to_string().contains("server"));
    // Joining a network twice is refused; inviting needs the network named now.
    let again = desk.call("networkInvite", json!({"network": "work"})).await["invite"].as_str().unwrap().to_owned();
    let refused = reqwest::Client::new()
        .post(format!("{}/api/networkJoin", server.base))
        .bearer_auth(&server.token)
        .json(&json!({"invite": again}))
        .send()
        .await
        .unwrap();
    assert!(!refused.status().is_success());
    let unnamed = reqwest::Client::new()
        .post(format!("{}/api/networkInvite", desk.base))
        .bearer_auth(&desk.token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(unnamed.text().await.unwrap().contains("several networks"));
    assert!(work["recoveryCode"].as_str().unwrap().len() > 60);

    // The desk's bots list both networks' computers with the network named.
    let desk_lead = desk.bot("Lead").await;
    let _ = (lead, tester);
    desk.call("send", json!({"botId": desk_lead, "text": "TRY tester@server.work hello"})).await;
    let sent = desk.wait_for(&desk_lead, is_final).await;
    assert!(sent["data"]["text"].as_str().unwrap().starts_with("Sent: "), "{sent}");
    let exchange = desk.wait_for(&desk_lead, |e| e["kind"] == "notice" && e["data"]["botMessage"].is_object()).await;
    assert_eq!(exchange["data"]["botMessage"]["targetBotId"], "tester@server.work");

    // Leaving work forgets it here; home stays.
    let left = desk.call("networkLeave", json!({"network": "work"})).await;
    let names: Vec<&str> = left["networks"].as_array().unwrap().iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["home"]);
    assert!(!desk.home.join(work["network"].as_str().unwrap()).exists());
}
