//! Starts under Ask, which wait on an approval card.

use super::*;

#[tokio::test]
async fn under_ask_a_start_needs_a_turn_to_ask_in() {
    let host = Host::start().await;
    let bot = host.bot("Builder", "ask").await;
    let arguments = json!({"command": sleeper(300)});
    let refused =
        host.refused("chatCall", json!({"botId": bot, "name": "start_dev_server", "arguments": arguments})).await;
    assert!(refused.contains("didn't allow"), "{refused}");
    assert_eq!(servers(&host, &bot).await, Vec::<Value>::new());
}

/// What makes the fixture agent call the tool through its real chat helper, in a turn.
fn start_through_agent(command: &str) -> String {
    format!("CHAT start_dev_server {}", json!({"command": command, "name": "web"}))
}

async fn entry(host: &Host, bot: &str, found: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let history = host.call("history", json!({"botId": bot})).await;
        if let Some(entry) = history["entries"].as_array().unwrap().iter().find(|e| found(e)) {
            return entry.clone();
        }
        assert!(Instant::now() < deadline, "{bot}: {history}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Has the agent start `command`, answers its card with `option`, and returns the agent's reply.
async fn through_card(host: &Host, bot: &str, command: &str, option: &str, check: impl Fn(&str)) -> String {
    host.call("send", json!({"botId": bot, "text": start_through_agent(command)})).await;
    let card = entry(host, bot, |e| {
        e["kind"] == "permission" && e["data"]["status"] == "pending" && e["data"]["title"] == "Start a dev server"
    })
    .await;
    check(card["data"]["detail"].as_str().unwrap());
    host.call("respondPermission", json!({"entryId": card["id"], "optionId": option})).await;
    let reply =
        entry(host, bot, |e| e["kind"] == "agent" && e["data"]["final"] == true && e["turn"] == card["turn"]).await;
    reply["data"]["text"].as_str().unwrap().to_owned()
}

async fn cards(host: &Host, bot: &str) -> usize {
    let history = host.call("history", json!({"botId": bot})).await;
    history["entries"].as_array().unwrap().iter().filter(|e| e["kind"] == "permission").count()
}

#[tokio::test]
async fn under_ask_a_start_waits_for_its_card() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Builder", "team_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;

    let denied = free_port();
    let reply = through_card(&host, &bot, &server_command(denied), "deny", |_| {}).await;
    assert!(reply.starts_with("Refused: ") && reply.contains("didn't allow"), "{reply}");
    assert_eq!(servers(&host, &bot).await, Vec::<Value>::new());
    assert!(!serves(denied));

    let folder = workspace(&host, &bot).await;
    let port = free_port();
    let command = server_command(port);
    let reply = through_card(&host, &bot, &command, "allow", |detail| {
        assert!(detail.contains(&command) && detail.contains(folder.to_str().unwrap()), "{detail}");
    })
    .await;
    assert!(reply.starts_with("Done: "), "{reply}");
    assert!(serves(port));

    // Allowed once, so a stopped one comes back with no second card.
    let id = servers(&host, &bot).await[0]["id"].clone();
    stop(&host, &bot, &id, port).await;
    let before = cards(&host, &bot).await;
    host.reply(&bot, &start_through_agent(&command)).await;
    assert!(serves(port));
    assert_eq!(cards(&host, &bot).await, before);
    stop(&host, &bot, &id, port).await;
}

#[tokio::test]
async fn a_server_nobody_allowed_gets_its_card_once_the_bot_is_under_ask() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Builder", "team_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "auto"})).await;
    let port = free_port();
    let command = server_command(port);
    let started = start(&host, &bot, &command, "web").await;
    stop(&host, &bot, &started["id"], port).await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;

    let reply = through_card(&host, &bot, &command, "deny", |_| {}).await;
    assert!(reply.starts_with("Refused: "), "{reply}");
    assert!(!serves(port));

    let reply = through_card(&host, &bot, &command, "allow", |_| {}).await;
    assert!(reply.starts_with("Done: "), "{reply}");
    assert!(serves(port));

    stop(&host, &bot, &started["id"], port).await;
    let before = cards(&host, &bot).await;
    host.reply(&bot, &start_through_agent(&command)).await;
    assert_eq!(cards(&host, &bot).await, before, "now someone has allowed it");
    stop(&host, &bot, &started["id"], port).await;
}

async fn pending_cards(host: &Host, bot: &str) -> Vec<Value> {
    let history = host.call("history", json!({"botId": bot})).await;
    let pending = |e: &&Value| e["kind"] == "permission" && e["data"]["status"] == "pending";
    history["entries"].as_array().unwrap().iter().filter(pending).cloned().collect()
}

#[tokio::test]
async fn an_allow_that_finds_the_server_ended_brings_it_back() {
    let host = Host::start().await;
    let bot = host.fixture_bot("Builder", "team_agent.py", "").await;
    host.call("updateBot", json!({"id": bot, "permission": "ask"})).await;
    let port = free_port();
    let arguments = json!({"command": server_command(port), "name": "web"});
    let text = format!("CHAT_TWICE start_dev_server {arguments}");
    host.call("send", json!({"botId": bot, "text": text})).await;
    common::eventually("a card for each", Duration::from_secs(40), async || {
        pending_cards(&host, &bot).await.len() == 2
    })
    .await;
    let waiting = pending_cards(&host, &bot).await;

    host.call("respondPermission", json!({"entryId": waiting[0]["id"], "optionId": "allow"})).await;
    wait_running(&host, &bot, port).await;
    let id = servers(&host, &bot).await[0]["id"].clone();
    stop(&host, &bot, &id, port).await;

    host.call("respondPermission", json!({"entryId": waiting[1]["id"], "optionId": "allow"})).await;
    let reply =
        entry(&host, &bot, |e| e["kind"] == "agent" && e["data"]["final"] == true && e["turn"] == waiting[0]["turn"])
            .await;
    let reply = reply["data"]["text"].as_str().unwrap();
    assert!(reply.contains(r#""reused":true"#), "{reply}");
    assert!(serves(port));
    assert_eq!(servers(&host, &bot).await.len(), 1);
    stop(&host, &bot, &id, port).await;
}
