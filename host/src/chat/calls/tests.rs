use super::*;

#[test]
fn call_ids_are_plain() {
    assert!(checked("6f1c2e0a-1b2c-4d5e-8f90-123456789abc").is_ok());
    assert!(checked("").is_err());
    assert!(checked("a/b").is_err());
    assert!(checked(&"a".repeat(65)).is_err());
}

#[test]
fn transcripts_keep_only_lines_of_the_user_or_the_voice() {
    let raw = json!([
        {"from": "user", "text": "  幫我看 git status ", "at": 5},
        {"from": "voice", "text": "Checking."},
        {"from": "bot", "text": "spoofed"},
        {"from": "user", "text": "   "},
        {"text": "no speaker"},
    ]);
    let lines = serde_json::to_value(transcript(&raw)).unwrap();
    assert_eq!(
        lines,
        json!([{"from": "user", "text": "幫我看 git status", "at": 5}, {"from": "voice", "text": "Checking."}])
    );
    let long: Vec<Value> = (0..MAX_LINES + 5).map(|_| json!({"from": "user", "text": "x"})).collect();
    assert_eq!(transcript(&Value::Array(long)).len(), MAX_LINES);
}

#[test]
fn titles_are_parsed() {
    assert_eq!(parse_title("title: 「規劃 2.26 發佈」").as_deref(), Some("規劃 2.26 發佈"));
    assert_eq!(parse_title("Sure.\n- Title: Fix the login bug.").as_deref(), Some("Fix the login bug"));
    assert_eq!(parse_title("NONE"), None);
    assert_eq!(parse_title(&format!("title: {}", "x".repeat(61))), None);
}

#[test]
fn a_call_reads_as_one_conversation_in_order() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let chat = Lane::main("b");
    store.insert_entry(&chat, EntryKind::User, 1, &json!({"text": "check git", "call": "c", "from": "voice"})).unwrap();
    store.insert_entry(&chat, EntryKind::Agent, 1, &json!({"text": "Clean.", "final": true, "call": "c"})).unwrap();
    store.insert_entry(&chat, EntryKind::Agent, 1, &json!({"text": "narration", "call": "c"})).unwrap();
    store.insert_entry(&chat, EntryKind::User, 2, &json!({"text": "typed, not the call's"})).unwrap();
    let data = json!({"text": UNTITLED, "call": "c", "transcript": [
        {"from": "user", "text": "看一下 git", "at": 0},
        {"from": "voice", "text": "It's clean.", "at": i64::MAX},
    ]});
    let line = store.insert_entry(&chat, EntryKind::Notice, 2, &data).unwrap();
    let said: Vec<String> =
        conversation(&store, "b", &line).into_iter().map(|(who, text)| format!("{who}: {text}")).collect();
    assert_eq!(said, ["User: 看一下 git", "Voice (relayed): check git", "Bot: Clean.", "Voice: It's clean."]);
}

#[tokio::test]
async fn a_call_keeps_what_was_said_and_hands_the_users_words_to_the_bot() {
    let hub = context::tests::hub();
    let id = uuid::Uuid::new_v4().to_string();
    // No agent to run: the title writer gives up at once.
    let cfg = BotConfig { backend: "none".into(), ..context::tests::bot(&id, "Mika", "/tmp") };
    hub.store.save_bot(&cfg).unwrap();
    let started = start(&hub, &json!({"botId": id, "callId": "c1", "mode": "device"}), "phone").await.unwrap();
    assert!(started["instructions"].as_str().unwrap().contains("voice relay for Mika"));
    assert!(started["tools"].as_array().unwrap().iter().any(|t| t["name"] == "search_memory"));
    assert!(started.get("credential").is_none(), "an on-device call has no provider");
    add_lines(&hub, "c1", &json!([{"from": "user", "text": "首頁", "at": 1}])).unwrap();
    // A provider session that replaces a dropped one picks up from what was said.
    let again = start(&hub, &json!({"botId": id, "callId": "c1", "mode": "device"}), "phone").await.unwrap();
    let instructions = again["instructions"].as_str().unwrap();
    assert!(instructions.contains("This call so far") && instructions.contains("User: 首頁"), "{instructions}");

    // The rest of what the user said comes with the tool call that hands it on.
    let unsent = json!([{"from": "user", "text": "改成藍色", "at": 2}, {"from": "voice", "text": "好"}]);
    let sent = tools::call(&hub, "c1", "send_to_bot", &json!({"text": "Make the home page blue"}), &unsent).await;
    // No bot actor runs in this test, so delivery fails; the message is in the chat all the same.
    assert!(sent["error"].is_string() || sent["sent"] == true);
    let relayed = hub.store.call_entries(&id, "c1", CALL_ENTRIES).unwrap();
    assert_eq!(relayed.len(), 1);
    assert_eq!(relayed[0].data["from"], "voice");
    let text = relayed[0].data["text"].as_str().unwrap();
    assert!(text.starts_with("Make the home page blue") && text.contains("«首頁 改成藍色»"), "{text}");
    assert_eq!(take_said(&hub.store, "c1").unwrap(), "", "the bot already heard it");

    let last = json!([{"from": "voice", "text": "Done."}]);
    let line = end(&hub, &id, "c1", 30, &last).unwrap().unwrap();
    assert_eq!((line.data["callSeconds"].as_u64(), line.data["call"].as_str()), (Some(30), Some("c1")));
    assert_eq!(line.data["transcript"].as_array().map(Vec::len), Some(4), "with the lines the app still held");
    assert!(end(&hub, &id, "c1", 31, &last).unwrap().is_none(), "an app trying again ends it once");
    assert!(bot_of(&hub.store, "c1").is_none(), "an ended call takes no more lines");
    assert!(start(&hub, &json!({"botId": id, "callId": "c1", "mode": "device"}), "phone").await.is_err());
    assert!(of_turn(&hub.store, &[relayed[0].id.clone()]).is_none(), "its replies show again");
    assert!(tools::call(&hub, "c1", "bot_status", &json!({}), &Value::Null).await["error"].is_string());
}

#[tokio::test]
async fn a_line_sent_again_is_kept_once() {
    let hub = context::tests::hub();
    heard(&hub, "b", "c1", "phone").unwrap();
    let batch =
        json!([{"from": "user", "text": "hi", "at": 1, "id": "l1"}, {"from": "voice", "text": "Hello.", "id": "l2"}]);
    add_lines(&hub, "c1", &batch).unwrap();
    // The answer to the first send was lost, so the app sends the batch again, and one more line.
    add_lines(&hub, "c1", &batch).unwrap();
    add_lines(&hub, "c1", &json!([{"from": "user", "text": "hi", "id": "l3"}])).unwrap();
    let s = state(&hub.store, "c1").unwrap();
    assert_eq!((s.owner.as_str(), s.mode), ("phone", CallMode::Device), "a call first heard by a message is on-device");
    let lines: Vec<String> = s.lines.iter().map(|l| l.text.clone()).collect();
    assert_eq!(lines, ["hi", "Hello.", "hi"], "repeats skipped, a new line with the same words kept");
}

#[test]
fn a_turn_answers_a_call_only_when_all_its_messages_came_from_it() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let chat = Lane::main("b");
    let id = |data: Value| store.insert_entry(&chat, EntryKind::User, 1, &data).unwrap().id;
    let a = id(json!({"text": "one", "call": "c1"}));
    let b = id(json!({"text": "two", "call": "c1"}));
    let typed = id(json!({"text": "typed"}));
    assert_eq!(of_turn(&store, &[a.clone(), b.clone()]).as_deref(), Some("c1"));
    assert_eq!(of_turn(&store, &[a.clone(), typed]), None);
    store.kv_set(&ended_key("c1"), "1").unwrap();
    assert_eq!(of_turn(&store, &[a, b]), None);
}
