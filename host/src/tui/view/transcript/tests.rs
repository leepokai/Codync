use super::*;

#[tokio::test]
async fn generated_files_are_selectable_cards_without_duplicate_fallback_text() {
    use crate::tui::app::Msg;
    use serde_json::json;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(crate::tui::net::Client::new("http://127.0.0.1:1", Some("fixture")), tx, String::new());
    app.on_msg(Msg::Event(json!({"type":"bot", "bot":{"id":"bot", "name":"Files", "cwd":"/workspace", "backend":"custom", "status":"idle"}})));
    app.on_msg(Msg::Event(json!({"type":"entry", "entry":{"id":"file-entry", "botId":"bot", "seq":1, "kind":"agent", "turn":1, "data":{"author":"bot", "text":"fallback must stay hidden", "final":true, "files":[{"id":"file", "name":".empty", "size":0, "sha256":"hash"}]}}})));
    let bot = app.bots.get("bot").unwrap();
    let chat = build_chat(&app, bot, 80);
    let text = chat.lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
    assert!(text.contains(".empty"));
    assert!(text.contains("f to download"));
    assert!(!text.contains("fallback must stay hidden"));
    assert!(chat.messages.iter().any(|(id, _, _)| id == "file-entry"));
}

#[tokio::test]
async fn an_artifact_card_is_one_line_with_its_newest_title() {
    use crate::tui::app::Msg;
    use serde_json::json;
    let mut app = app_with_bots();
    app.on_msg(Msg::Event(
        json!({"type":"entry", "entry":{"id":"card", "botId":"a", "seq":1, "kind":"agent", "turn":1,
        "data":{"author":"a", "text":"Artifact: Weekly report", "final":true,
                "artifact":{"id":"p", "title":"Weekly report", "updatedAt":1}}}}),
    ));
    let text = |app: &App| {
        let chat = build_chat(app, &app.bots["a"], 80);
        assert!(chat.messages.iter().any(|(id, _, _)| id == "card"), "a card can be picked");
        chat.lines.iter().map(ToString::to_string).collect::<Vec<_>>()
    };
    let lines = text(&app);
    assert!(lines.iter().any(|l| l == " ▤ Artifact: Weekly report"), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("o opens it in the browser · f saves it")));
    app.on_msg(Msg::Event(json!({"type":"artifact", "artifact":{"id":"p", "botId":"a", "title":"Q4 report", "size":1,
        "sha256":"0".repeat(64), "createdAt":0, "updatedAt":2}})));
    let lines = text(&app);
    assert!(lines.iter().any(|l| l == " ▤ Artifact: Q4 report"), "a rename shows: {lines:?}");
    assert!(!lines.iter().any(|l| l.contains("Weekly report")));
}

fn app_with_bots() -> App {
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(super::super::super::net::Client::new("http://127.0.0.1:1", None), tx, String::new());
    for (id, name) in [("a", "Egan"), ("b", "Owen")] {
        app.on_msg(super::super::super::app::Msg::Event(serde_json::json!({"type":"bot","bot":{"id":id,"name":name}})));
    }
    app
}

fn exchange_entry(status: &str) -> Entry {
    Entry {
        id: "n".into(),
        seq: 1,
        turn: 1,
        kind: Kind::Notice,
        created_at: 0,
        data: serde_json::json!({
            "text": "Messaged b: Please review the secret plan\nCompleted.", "style": "info", "status": status,
            "heading": "Messaged b: Please review the secret plan",
            "botMessage": {"sourceBotId": "a", "targetBotId": "b", "text": "Please review the secret plan"},
            "sourceBotId": "a", "targetBotId": "b", "delegationId": "r",
        }),
    }
}

#[test]
fn bot_message_is_one_line_with_a_click_and_marks_only_failure() {
    let app = app_with_bots();
    let bot = &app.bots["a"];
    let render = |e: &Entry| {
        let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
        entry_lines(&mut out, &app, bot, e, &HashMap::new(), 80);
        out
    };
    let ok = render(&exchange_entry("completed"));
    let text = ok.lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
    assert!(text.contains("Messaged") && text.contains("Owen"));
    assert!(!text.contains("secret plan") && !text.contains("failed"));
    assert!(matches!(ok.buttons.as_slice(), [(_, 3, _, Click::BotChat(id))] if id == "n"));
    assert_eq!(ok.messages.len(), 1);
    let mut failed = exchange_entry("failed");
    failed.data["text"] = "Messaged b: Please review the secret plan\nRecipient stopped.".into();
    let bad = render(&failed);
    let line = bad.lines.last().unwrap();
    assert!(line.to_string().ends_with("Owen · failed"));
    assert_eq!(line.spans[0].style, theme().red);
}

#[test]
fn memory_lines_and_kept_handoffs_stay_out_of_the_chat() {
    let app = app_with_bots();
    let mut line = exchange_entry("done");
    line.data =
        serde_json::json!({"text": "Remembered: likes tea", "memory": {"action": "remember", "status": "done"}});
    let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
    entry_lines(&mut out, &app, &app.bots["a"], &line, &HashMap::new(), 80);
    assert!(out.lines.is_empty() && out.messages.is_empty());
    line.data = serde_json::json!({"text": "Kept this conversation.", "handoff": {"status": "kept"}});
    entry_lines(&mut out, &app, &app.bots["a"], &line, &HashMap::new(), 80);
    assert!(out.lines.is_empty(), "a kept handoff changed nothing");
    line.data =
        serde_json::json!({"text": "Earlier context was summarized to make room.", "compaction": {"byHost": true}});
    entry_lines(&mut out, &app, &app.bots["a"], &line, &HashMap::new(), 80);
    assert!(out.lines.is_empty(), "a summary keeps the same conversation going");
}

#[test]
fn a_run_of_exchanges_is_one_counted_line() {
    let app = app_with_bots();
    let bot = &app.bots["a"];
    let mut entries = vec![];
    for (i, status) in ["completed", "failed", "completed"].into_iter().enumerate() {
        let mut e = exchange_entry(status);
        e.id = format!("n{i}");
        e.seq = i64::try_from(i).unwrap_or(0) + 1;
        entries.push(e);
    }
    let refs: Vec<&Entry> = entries.iter().collect();
    let build = |refs: &[&Entry]| {
        let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
        for item in group(&bot.id, refs) {
            match item {
                Item::Entry(e) => entry_lines(&mut out, &app, bot, e, &HashMap::new(), 80),
                Item::Group(g) => {
                    let lead = format!("{} messages with ", g.count);
                    bot_message_line(&mut out, &app, g.first, (&g.peer, g.peer_name.as_deref()), &lead, g.failed, 0);
                }
            }
        }
        out
    };
    let out = build(&refs);
    assert_eq!(out.lines.len(), 1);
    let line = &out.lines[0];
    assert!(line.to_string().contains("3 messages with") && line.to_string().ends_with("Owen · failed"), "{line}");
    assert_eq!(line.spans[0].style, theme().red);
    assert!(matches!(out.buttons.as_slice(), [(_, 3, _, Click::BotChat(id))] if id == "n0"));
    assert_eq!(out.messages.len(), 1);
    assert!(build(&refs[..1]).lines[0].to_string().contains("Messaged"));
}

#[test]
fn a_bot_on_another_computer_is_named_by_its_exchange() {
    let app = app_with_bots();
    let mut e = exchange_entry("completed");
    e.data["botMessage"]["targetBotId"] = "reviewer@desk".into();
    e.data["botMessage"]["peerName"] = "Reviewer on desk".into();
    let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
    entry_lines(&mut out, &app, &app.bots["a"], &e, &HashMap::new(), 80);
    assert!(out.lines[out.lines.len() - 1].to_string().ends_with("Messaged ? Reviewer on desk"), "{:?}", out.lines);
}

#[test]
fn a_reply_shows_the_message_it_answers() {
    let app = app_with_bots();
    let reply = Entry {
        id: "u".into(),
        seq: 2,
        turn: 2,
        kind: Kind::User,
        created_at: 0,
        data: serde_json::json!({"text": "why?", "replyTo": {"id": "e", "author": "a", "text": "## Plan\n- ready"}}),
    };
    let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
    entry_lines(&mut out, &app, &app.bots["a"], &reply, &HashMap::new(), 80);
    let text: Vec<String> = out.lines.iter().map(ToString::to_string).collect();
    assert_eq!(text[1], " ↳ “Plan”", "{text:?}");
    assert!(text[2].contains("why?"));
}

#[tokio::test]
async fn a_calls_conversation_takes_what_the_host_sent_beyond_the_loaded_chat() {
    use crate::tui::app::Msg;
    use serde_json::json;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(crate::tui::net::Client::new("http://127.0.0.1:1", Some("fixture")), tx, String::new());
    app.on_msg(Msg::Event(
        json!({"type":"bot", "bot":{"id":"b", "name":"Mika", "cwd":"/w", "backend":"custom", "status":"idle"}}),
    ));
    app.on_msg(Msg::Event(
        json!({"type":"entry", "entry":{"id":"line", "botId":"b", "seq":9, "kind":"notice", "turn":1,
        "createdAt": 9, "data":{"text":"Old call", "callSeconds":30, "call":"c"}}}),
    ));
    let line = app.lane("b").into_iter().find(|e| e.id == "line").cloned().unwrap();
    let bot = app.bots["b"].clone();
    assert_eq!(call_lines(&app, &bot, &line), ["loading the call…"]);
    // Older than anything loaded here.
    let parse = |v: serde_json::Value| Entry::parse(&v).unwrap().1;
    app.call_entries.insert(
        "c".into(),
        vec![
            parse(json!({"id":"u", "botId":"b", "seq":2, "kind":"user", "turn":1, "createdAt":2,
            "data":{"text":"check git", "call":"c", "from":"voice"}})),
            parse(json!({"id":"a", "botId":"b", "seq":3, "kind":"agent", "turn":1, "createdAt":3,
            "data":{"text":"Clean.", "final":true, "call":"c"}})),
        ],
    );
    assert_eq!(call_lines(&app, &bot, &line), ["voice → bot: check git", "Mika: Clean."]);
}
