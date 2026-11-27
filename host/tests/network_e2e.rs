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

    /// Whether the host took the call, and its answer.
    async fn post(&self, method: &str, body: Value) -> (bool, Value) {
        let response = reqwest::Client::new()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .timeout(Duration::from_mins(2))
            .send()
            .await
            .unwrap();
        (response.status().is_success(), response.json().await.unwrap())
    }

    async fn call(&self, method: &str, body: Value) -> Value {
        let (ok, value) = self.post(method, body).await;
        assert!(ok, "{method}: {value}");
        value
    }

    /// A call that must be refused: its error text.
    async fn refused(&self, method: &str, body: Value) -> String {
        let (ok, value) = self.post(method, body).await;
        assert!(!ok, "{method} should have been refused: {value}");
        value["error"].as_str().unwrap_or_default().to_owned()
    }

    /// Stops the host, lets `change` edit its database while it's down, and starts it again on the same data.
    async fn restart(&mut self, change: impl FnOnce(&rusqlite::Connection)) {
        common::stop(&mut self.child);
        change(&rusqlite::Connection::open(self.home.join("codync.db")).unwrap());
        (self.child, self.base) = common::serve(&self.home).await;
    }

    /// The saved networks (kv `networks`).
    fn saved_networks(&self) -> Value {
        let db = rusqlite::Connection::open(self.home.join("codync.db")).unwrap();
        let text: String = db.query_row("SELECT v FROM kv WHERE k = 'networks'", [], |row| row.get(0)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// The first network's rules, once its node has started.
    async fn rules(&self) -> Value {
        tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                let status = self.call("networkStatus", json!({})).await;
                if status["joined"] == true {
                    return status["networks"][0]["rules"].clone();
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("the network did not start")
    }

    async fn bot(&self, name: &str) -> String {
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        let result = self
            .call(
                "createBot",
                json!({
                    "name": name, "backend": "custom", "permission": "ask", "notify": false,
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
    // As the computers already in a network are kept when the default became `auto`.
    let allow = json!({"rules": [{"fromComputer": "*", "fromAgent": "*", "toAgent": "*", "access": "allow"}]});
    desk.call("setNetworkRules", allow.clone()).await;
    laptop.call("setNetworkRules", allow).await;
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

    // The desk names the lead from its own listing of the laptop, which it takes when asked: until then the
    // request's notice could only show the address (`lead@laptop`).
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let peers = desk.call("networkPeers", json!({})).await;
            if computer(&peers, "laptop")["agents"].as_array().is_some_and(|a| a.iter().any(|a| a["id"] == "lead")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the desk never listed the laptop's lead");

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
    // cancelled, and the lead hears so in a reply turn.
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk PERMISSION then stop"})).await;
    desk.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    desk.call("stopWork", json!({"botId": reviewer})).await;
    let stopped = laptop.wait_for(&lead, exchange("cancelled")).await;
    assert!(stopped["data"]["botMessage"]["detail"].as_str().unwrap().starts_with("Reviewer on desk was stopped"));
    laptop.wait_for(&lead, |e| is_final(e) && e["data"]["text"] == "Relayed: failed").await;

    // A turn that fails comes back as a failure, in a reply turn too.
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk FAIL on purpose"})).await;
    laptop.wait_for(&lead, exchange("failed")).await;
    let relayed = |e: &Value| is_final(e) && e["data"]["text"] == "Relayed: failed";
    let finals = tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let history = laptop.call("history", json!({"botId": lead})).await;
            let entries = history["entries"].as_array().unwrap().clone();
            if entries.iter().filter(|e| relayed(e)).count() == 2 {
                return entries.iter().filter(|e| is_final(e)).count();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the failure's reply turn did not come");
    assert_eq!(finals, 4, "two requests and two reply turns");
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

/// The session modes a bot's agent lists from its next session on, the first the one it opens in.
fn list_modes(host: &Host, bot: &str, ids: &[&str]) {
    let modes: Vec<Value> = ids.iter().map(|id| json!({"id": id, "name": id})).collect();
    let listed = json!({"modes": {"currentModeId": ids[0], "availableModes": modes}});
    std::fs::write(common::folder(&host.home, bot).join("modes.json"), listed.to_string()).unwrap();
}

/// The modes the host switched a bot's agent to, in order.
fn switched(host: &Host, bot: &str) -> Vec<String> {
    let log = std::fs::read_to_string(common::folder(&host.home, bot).join("modes.jsonl")).unwrap_or_default();
    log.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[tokio::test]
async fn a_rule_at_ask_holds_a_bot_that_approves_by_itself_to_asking() {
    let (desk, laptop, reviewer, lead) = two_computers().await;
    list_modes(&desk, "Reviewer", &["bypassPermissions", "default"]);
    list_modes(&laptop, "Lead", &["bypassPermissions", "default"]);
    desk.call("updateBot", json!({"id": reviewer, "permission": "auto"})).await;
    let pending = |e: &Value| e["kind"] == "permission" && e["data"]["status"] == "pending";
    let relayed = |text: &'static str| move |e: &Value| is_final(e) && e["data"]["text"] == text;

    // At allow the laptop's request runs as the reviewer is set, so its tool call is approved for it.
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk PERMISSION first"})).await;
    laptop.wait_for(&lead, relayed("Relayed: reply: PERMISSION first")).await;
    assert!(switched(&desk, "Reviewer").is_empty(), "it stayed in its own mode");

    // At ask the same request waits for the desk's user, in the agent's asking mode.
    let rules = json!([{"fromComputer": "laptop", "fromAgent": "*", "toAgent": "*", "access": "ask"}]);
    let view = desk.call("setNetworkRules", json!({"rules": rules})).await;
    assert_eq!(view["networks"][0]["rules"][0]["access"], "ask");
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk PERMISSION second"})).await;
    let card = desk.wait_for(&reviewer, pending).await;
    assert_eq!(switched(&desk, "Reviewer"), ["default"]);
    let prompts = std::fs::read_to_string(common::folder(&desk.home, "Reviewer").join("prompts.jsonl")).unwrap();
    let told = prompts.lines().last().unwrap();
    assert!(told.contains("a computer the user trusts less than this one"), "{told}");

    // A thread of the reviewer answers it: only reading and answering the user are left to that thread.
    let thread = card["data"]["thread"]["id"].as_str().expect("a thread's card").to_owned();
    let tool = |name: &str, arguments: Value| json!({"botId": thread, "name": name, "arguments": arguments});
    let command = format!("{} -c \"import time; time.sleep(300)\"", common::python());
    let server = desk.refused("chatCall", tool("start_dev_server", json!({"command": command}))).await;
    assert!(server.contains("start_dev_server isn't available in this turn"), "{server}");
    let routine = desk.refused("routineCall", tool("save_routine", json!({}))).await;
    assert!(routine.contains("save_routine isn't available in this turn"), "{routine}");
    desk.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    laptop.wait_for(&lead, relayed("Relayed: reply: PERMISSION second")).await;

    // The user's own message runs in the chat as the bot is set: only the thread's session was held.
    desk.call("send", json!({"botId": reviewer, "text": "PERMISSION mine"})).await;
    desk.wait_for(&reviewer, relayed("reply: PERMISSION mine")).await;
    assert_eq!(switched(&desk, "Reviewer"), ["default"]);

    // An answer from a computer under a ceiling holds the turn that reads it to the ceiling too.
    let rules = json!([{"fromComputer": "desk", "fromAgent": "*", "toAgent": "*", "access": "ask"}]);
    laptop.call("setNetworkRules", json!({"rules": rules})).await;
    let allow = json!([{"fromComputer": "*", "fromAgent": "*", "toAgent": "*", "access": "allow"}]);
    desk.call("setNetworkRules", json!({"rules": allow})).await;
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk third"})).await;
    laptop.wait_for(&lead, relayed("Relayed: reply: third")).await;
    assert_eq!(switched(&laptop, "Lead"), ["default"]);
    let prompts = std::fs::read_to_string(common::folder(&laptop.home, "Lead").join("prompts.jsonl")).unwrap();
    let told = prompts.lines().last().unwrap();
    assert!(told.contains("these are bot replies") && told.contains("a computer the user trusts less"), "{told}");
    // The ceiling stands past that turn's end: background work would run without it.
    let work = json!({"botId": lead, "name": "add_task", "arguments": {"title": "More", "brief": "More"}});
    let refused = laptop.refused("chatCall", work).await;
    assert!(refused.contains("add_task isn't available in this turn"), "{refused}");

    // That turn messages no one: the answer's text can't travel on to a bot that isn't held to the ceiling.
    std::fs::write(common::folder(&desk.home, "Reviewer").join("handoff.json"), r#"{"target":"lead@laptop"}"#).unwrap();
    std::fs::write(common::folder(&laptop.home, "Lead").join("handoff.json"), r#"{"target":"reviewer@desk"}"#).unwrap();
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk CHAIN"})).await;
    let stopped = laptop
        .wait_for(&lead, |e| is_final(e) && e["data"]["text"].as_str().is_some_and(|t| t.starts_with("Chain stopped")))
        .await;
    let text = stopped["data"]["text"].as_str().unwrap();
    assert!(text.contains("a computer the user trusts less"), "{text}");
}

#[tokio::test]
async fn a_computer_no_rule_matches_works_under_auto() {
    let (desk, laptop, reviewer, lead) = two_computers().await;
    list_modes(&desk, "Reviewer", &["bypassPermissions", "default", "auto"]);
    desk.call("updateBot", json!({"id": reviewer, "permission": "auto"})).await;
    let view = desk.call("setNetworkRules", json!({"rules": []})).await;
    assert_eq!(view["networks"][0]["defaultAccess"], "auto");
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk PERMISSION review"})).await;
    // Not approved by itself, in a mode no freer than Auto: the fixture's `auto` isn't Claude's, so `default`.
    let card = desk.wait_for(&reviewer, |e| e["kind"] == "permission" && e["data"]["status"] == "pending").await;
    assert_eq!(switched(&desk, "Reviewer"), ["default"]);
    desk.call("respondPermission", json!({"entryId": card["id"], "optionId": "allow"})).await;
    let relayed = |e: &Value| is_final(e) && e["data"]["text"] == "Relayed: reply: PERMISSION review";
    laptop.wait_for(&lead, relayed).await;
}

#[tokio::test]
async fn a_network_saved_before_the_auto_default_keeps_its_computers_at_allow_once() {
    let (mut desk, _laptop, _, _) = two_computers().await;
    desk.call("setNetworkRules", json!({"rules": []})).await;
    // The network as a host before the `auto` default saved it.
    desk.restart(|db| {
        let text: String = db.query_row("SELECT v FROM kv WHERE k = 'networks'", [], |row| row.get(0)).unwrap();
        let mut networks: Value = serde_json::from_str(&text).unwrap();
        networks[0].as_object_mut().unwrap().remove("auto_default").expect("saved since the default");
        db.execute("UPDATE kv SET v = ?1 WHERE k = 'networks'", [networks.to_string()]).unwrap();
    })
    .await;
    // The laptop, already in it, keeps what it had; this computer gets no rule of its own.
    let kept = json!([{"fromComputer": "laptop", "fromAgent": "*", "toAgent": "*", "access": "allow"}]);
    assert_eq!(desk.rules().await, kept);
    assert_eq!(desk.saved_networks()[0]["auto_default"], true);
    assert_eq!(desk.saved_networks()[0]["rules"], kept);
    // Once: what the user changes later stays.
    desk.call("setNetworkRules", json!({"rules": []})).await;
    desk.restart(|_| {}).await;
    assert_eq!(desk.rules().await, json!([]));
}

#[tokio::test]
async fn an_agent_that_cant_be_held_to_a_ceiling_doesnt_work_on_the_request() {
    let (desk, laptop, reviewer, lead) = two_computers().await;
    // The reviewer's agent lists no modes, so how freely it works can't be told.
    let rules = json!([{"fromComputer": "laptop", "fromAgent": "*", "toAgent": "*", "access": "auto"}]);
    desk.call("setNetworkRules", json!({"rules": rules})).await;
    laptop.call("send", json!({"botId": lead, "text": "DELEGATE reviewer@desk review this"})).await;
    let failed = laptop.wait_for(&lead, exchange("failed")).await;
    let detail = failed["data"]["botMessage"]["detail"].as_str().unwrap();
    assert!(detail.contains("no mode careful enough"), "{detail}");
    let history = desk.call("history", json!({"botId": reviewer})).await;
    assert!(!history["entries"].as_array().unwrap().iter().any(is_final), "the reviewer never answered");
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

#[tokio::test]
async fn a_session_a_rule_opens_takes_letters_from_another_computer_in_its_mailbox() {
    let (desk, laptop, _, lead) = two_computers().await;
    let call = |session: Option<&str>, name: &str| {
        let agent = json!({"kind": "codex", "session": session, "process": "1-e2e", "cwd": "/work/api"});
        json!({"agent": agent, "name": name, "arguments": {}})
    };
    let address = desk.call("agentCall", call(Some("t1"), "whoami")).await["address"].as_str().unwrap().to_owned();
    let temporary = desk.call("agentCall", call(None, "whoami")).await["address"].as_str().unwrap().to_owned();
    let offered = async || {
        let peers = laptop.call("networkPeers", json!({})).await;
        let agents = computer(&peers, "desk")["agents"].as_array().unwrap().clone();
        agents.iter().filter_map(|a| a["id"].as_str().map(str::to_owned)).collect::<Vec<_>>()
    };
    assert!(!offered().await.contains(&address), "`allow * -> *` doesn't open a session");
    // It reads its own mail: no one there to hold it to Auto or Ask, so such a rule denies it.
    let ask = json!({"rules": [{"fromComputer": "*", "fromAgent": "*", "toAgent": address, "access": "ask"}]});
    let saved = desk.call("setNetworkRules", ask).await;
    assert_eq!(saved["networks"][0]["rules"][0]["access"], "deny", "{saved}");
    let view = desk.call("networkView", json!({})).await;
    let shown: Vec<&str> = view["sessions"].as_array().unwrap().iter().filter_map(|s| s["address"].as_str()).collect();
    assert_eq!(shown, [address.as_str()], "the temporary one isn't listed");
    // A rule that names it opens it; never the temporary one.
    let rules = json!({"rules": [
        {"fromComputer": "laptop", "fromAgent": "lead", "toAgent": address, "access": "allow"},
        {"fromComputer": "laptop", "fromAgent": "lead", "toAgent": temporary, "access": "allow"},
        {"fromComputer": "*", "fromAgent": "*", "toAgent": "*", "access": "allow"},
    ]});
    desk.call("setNetworkRules", rules).await;
    let listed = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let agents = offered().await;
            if agents.contains(&address) {
                return agents;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the desk never offered the session");
    assert!(!listed.contains(&temporary), "{listed:?}");
    // The letter waits in the session's mailbox under its own id, and the session's answer ends the request.
    laptop.call("send", json!({"botId": lead, "text": format!("DELEGATE {address}@desk hello session")})).await;
    let mut wait = call(Some("t1"), "check_inbox");
    wait["arguments"] = json!({"waitSecs": 20});
    let inbox = desk.call("agentCall", wait).await;
    let letters = inbox["letters"].as_str().unwrap();
    assert!(letters.contains("from=\"lead@laptop\"") && letters.contains("hello session"), "{inbox}");
    let id = letters.split(" id=\"").nth(1).and_then(|rest| rest.split('"').next()).unwrap().to_owned();
    let mut answer = call(Some("t1"), "message_bot");
    answer["arguments"] = json!({"botId": "lead@laptop", "message": "hi back", "replyTo": id});
    assert_eq!(desk.call("agentCall", answer).await["status"], "delivered");
    let answered = |e: &Value| is_final(e) && e.to_string().contains("hi back");
    laptop.wait_for(&lead, answered).await;
}

/// What `codync-host network hook <agent>` prints, fed `stdin`, against the host on `port` with `home`'s token.
async fn hook(home: &std::path::Path, port: &str, agent: &str, stdin: &str, env: &[(&str, &str)]) -> String {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_codync-host"))
        .args(["network", "hook", agent, "--port", port])
        .env("CODYNC_HOME", home)
        .env_remove("CODYNC_BOT")
        .envs(env.iter().copied())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).await.unwrap();
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output()).await.unwrap().unwrap();
    assert!(output.status.success(), "a hook never fails its agent's step");
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
async fn an_agents_hook_tells_its_session_of_new_mail_in_the_shape_its_agent_reads() {
    let host = Host::start().await;
    let port = host.base.rsplit(':').next().unwrap();
    let run = async |agent: &str, stdin: &str, env: &[(&str, &str)]| hook(&host.home, port, agent, stdin, env).await;
    let event = |name: &str, session: &str| json!({"hook_event_name": name, "session_id": session}).to_string();
    let told = async |agent: &str, name: &str, session: &str| -> Value {
        serde_json::from_str(&run(agent, &event(name, session), &[]).await).unwrap()
    };
    let tool = |kind: &str, session: &str, name: &str, arguments: Value| {
        let agent = json!({"kind": kind, "session": session, "process": "1-e2e", "cwd": "/work/api"});
        json!({"agent": agent, "name": name, "arguments": arguments})
    };
    let address = async |kind: &str, session: &str| {
        host.call("agentCall", tool(kind, session, "whoami", json!({}))).await["address"].as_str().unwrap().to_owned()
    };
    let mail = async |to: &str| {
        let letter = json!({"botId": to, "message": "did the API change?"});
        host.call("agentCall", tool("claude-code", "s1", "message_bot", letter)).await;
    };
    assert_eq!(run("codex", &event("PostToolUse", "t1"), &[]).await, "", "a session its MCP server never registered");
    let (codex, copilot) = (address("codex", "t1").await, address("copilot", "c1").await);
    address("claude-code", "s1").await;
    assert_eq!(run("codex", &event("PostToolUse", "t1"), &[]).await, "", "no mail");
    mail(&codex).await;
    // Silent for a bot's own agent, stdin it can't read, an event it doesn't answer, and a host that's down.
    assert_eq!(run("codex", &event("PostToolUse", "t1"), &[("CODYNC_BOT", "b")]).await, "");
    assert_eq!(run("codex", "{", &[]).await, "");
    assert_eq!(run("codex", &event("SessionStart", "t1"), &[]).await, "");
    assert_eq!(hook(&host.home, "1", "codex", &event("PostToolUse", "t1"), &[]).await, "");
    // Codex and Grok read `hookSpecificOutput`, Copilot CLI a top-level field; a `Stop` keeps the turn going.
    let after_tool = told("codex", "PostToolUse", "t1").await;
    let text = after_tool["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert!(text.contains("1 letter(s)") && !text.contains("did the API"), "{after_tool}");
    assert_eq!(run("codex", &event("Stop", "t1"), &[]).await, "", "told once");
    mail(&codex).await;
    let stop = told("codex", "Stop", "t1").await;
    assert!(stop["decision"] == "block" && stop["reason"].as_str().unwrap().contains("2 letter(s)"), "{stop}");
    mail(&copilot).await;
    let copilot_told = told("copilot", "PostToolUse", "c1").await;
    assert!(copilot_told["additionalContext"].as_str().unwrap().contains("1 letter(s)"), "{copilot_told}");
    let grok = address("grok", "g1").await;
    mail(&grok).await;
    let grok_told = told("grok", "PostToolUse", "g1").await;
    let text = grok_told["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert!(text.contains("1 letter(s)"), "{grok_told}");
    assert_eq!(host.call("agentCall", tool("codex", "t1", "whoami", json!({}))).await["tier"], "turn");
}
