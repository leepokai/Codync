use super::*;
use serde_json::json;

fn event(update: Value) -> Event {
    let mut params = json!({"sessionId": "s1"});
    params["update"] = update;
    Update::parse(&params).event
}

#[test]
fn an_update_names_its_session() {
    let update = Update::parse(&json!({"sessionId": "s1", "update": {"sessionUpdate": "agent_thought_chunk",
        "content": {"type": "text", "text": "hm"}}}));
    assert_eq!(update.session.as_deref(), Some("s1"));
    assert_eq!(update.event, Event::Thought { text: "hm".into() });
}

#[test]
fn reply_text_says_when_it_is_an_adapters_notification() {
    let reply =
        event(json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "Checking…"}}));
    assert_eq!(reply, Event::Text { text: "Checking…".into(), notice: None });
    // pi-acp passes a notification pi showed on as text.
    let notified = event(json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "503"},
        "_meta": {"piAcp": {"notify": {"level": "error"}}}}));
    assert_eq!(notified, Event::Text { text: "503".into(), notice: Some("error".into()) });
}

#[test]
fn a_tool_call_keeps_what_it_sent_and_nothing_else() {
    let started =
        event(json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Edit a.txt", "kind": "edit",
        "status": "pending", "content": [{"type": "diff", "path": "a.txt", "oldText": "x\n", "newText": "y\n"},
        {"type": "content", "content": {"type": "text", "text": "ran"}}, {"type": "terminal", "terminalId": "term"}]}));
    let Event::ToolStarted(call) = started else { panic!("a tool call") };
    assert_eq!(
        (call.id.as_str(), call.title.as_deref(), call.kind.as_deref()),
        ("t1", Some("Edit a.txt"), Some("edit"))
    );
    assert_eq!(
        call.content,
        Some(vec![
            ToolContent::Diff { path: "a.txt".into(), old_text: "x\n".into(), new_text: "y\n".into() },
            ToolContent::Text("ran".into()),
            ToolContent::Terminal { id: "term".into() },
        ])
    );
    assert_eq!(call.locations, None);
    let updated = event(json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
        "locations": null}));
    let Event::ToolUpdated(patch) = updated else { panic!("a tool call update") };
    assert_eq!(*patch, ToolCall { id: "t1".into(), status: Some("completed".into()), ..ToolCall::default() });
}

#[test]
fn a_plan_keeps_its_entries_as_sent() {
    let entries = json!([{"content": "edit", "status": "completed", "priority": "high"}]);
    assert_eq!(event(json!({"sessionUpdate": "plan", "entries": entries})), Event::Plan { entries });
}

#[test]
fn usage_carries_claudes_rate_limit() {
    let plain = event(json!({"sessionUpdate": "usage_update", "used": 7611, "size": 128_000}));
    assert_eq!(plain, Event::Usage(Usage { used: Some(7611), size: Some(128_000), claude_rate_limit: None }));
    let limit = json!({"rateLimitType": "five_hour", "utilization": 0.42});
    let claude = event(json!({"sessionUpdate": "usage_update", "used": 1, "size": 2,
        "_meta": {"_claude/rateLimit": limit}}));
    assert!(matches!(claude, Event::Usage(Usage { claude_rate_limit: Some(l), .. }) if l == limit));
}

#[test]
fn a_compaction_says_when_it_is_done_and_its_summary() {
    let going = event(json!({"sessionUpdate": "compaction_update", "compactionId": "c1", "status": "in_progress"}));
    assert_eq!(going, Event::Compaction(Compaction { id: "c1".into(), done: false, summary: None }));
    let done = event(json!({"sessionUpdate": "compaction_update", "compactionId": "c1", "status": "completed",
        "summary": [{"type": "text", "text": "the gist"}]}));
    assert_eq!(done, Event::Compaction(Compaction { id: "c1".into(), done: true, summary: Some("the gist".into()) }));
    let blank = event(json!({"sessionUpdate": "compaction_update", "compactionId": "c1", "status": "completed",
        "summary": {"type": "text", "text": " "}}));
    assert_eq!(blank, Event::Compaction(Compaction { id: "c1".into(), done: true, summary: None }));
}

#[test]
fn commands_are_kept_as_the_agent_names_them() {
    let listed = event(json!({"sessionUpdate": "available_commands_update", "availableCommands": [
        {"name": "review", "description": " Review ", "input": {"hint": "what"}}, {"name": "/compact"}, {}]}));
    assert_eq!(
        listed,
        Event::Commands(vec![
            Command { name: "review".into(), description: Some(" Review ".into()), hint: Some("what".into()) },
            Command { name: "/compact".into(), description: None, hint: None },
        ])
    );
}

#[test]
fn modes_and_options_are_read_and_anything_else_is_kept() {
    assert_eq!(
        event(json!({"sessionUpdate": "current_mode_update", "currentModeId": "plan"})),
        Event::ModeChanged { mode: Some("plan".into()) }
    );
    let options = json!({"sessionUpdate": "config_option_update", "configOptions": []});
    assert_eq!(event(options.clone()), Event::ConfigOptions(options));
    let unknown = json!({"sessionUpdate": "session_info_update", "_meta": {"piAcp": {"running": true}}});
    assert_eq!(event(unknown.clone()), Event::Other(unknown));
}

/// What each update of claude-agent-acp 0.109.1 (trimmed) says: which call launched it, and about its subagent.
fn read(u: Value) -> (Option<String>, Option<Subagent>) {
    let (Event::ToolStarted(call) | Event::ToolUpdated(call)) = event(u) else { panic!("a tool call") };
    (call.parent, call.subagent)
}

fn agent(update: Value) -> Value {
    let mut u = update;
    u["toolCallId"] = "toolu_A".into();
    u["_meta"]["claudeCode"]["toolName"] = "Agent".into();
    u
}

#[test]
fn a_subagents_calls_name_their_launch_and_the_launch_reports_its_totals() {
    let asked = read(agent(json!({"sessionUpdate": "tool_call_update", "rawInput": {"description": "Count files",
        "subagent_type": "general-purpose", "run_in_background": false}})));
    let kind = Subagent { kind: Some("general-purpose".into()), background: Some(false), ..Subagent::default() };
    assert_eq!(asked, (None, Some(kind)));
    let child = read(json!({"sessionUpdate": "tool_call", "toolCallId": "toolu_B", "title": "ls", "kind": "execute",
        "_meta": {"claudeCode": {"toolName": "Bash", "parentToolUseId": "toolu_A"}}}));
    assert_eq!(child, (Some("toolu_A".into()), None));
    let ended = read(agent(json!({"sessionUpdate": "tool_call_update", "_meta": {"claudeCode": {"toolResponse": {
        "status": "completed", "agentId": "ab00", "agentType": "general-purpose", "resolvedModel": "claude-opus-5-5",
        "totalDurationMs": 6287, "totalTokens": 42310, "totalToolUseCount": 1}}}})));
    let totals = Subagent {
        id: Some("ab00".into()),
        kind: Some("general-purpose".into()),
        model: Some("claude-opus-5-5".into()),
        background: None,
        tokens: Some(42310),
        tool_uses: Some(1),
        duration_ms: Some(6287),
    };
    assert_eq!(ended, (None, Some(totals)));
    // The Agent call's first and last updates say nothing about its subagent.
    let started = agent(json!({"sessionUpdate": "tool_call", "rawInput": {}, "status": "pending", "title": "Task"}));
    assert_eq!(read(started), (None, None));
    assert_eq!(read(agent(json!({"sessionUpdate": "tool_call_update", "status": "completed"}))), (None, None));
}

#[test]
fn a_background_subagent_has_its_kind_from_what_it_was_asked_and_no_totals() {
    let launched = read(agent(json!({"sessionUpdate": "tool_call_update", "_meta": {"claudeCode": {"toolResponse": {
        "isAsync": true, "status": "async_launched", "agentId": "ac78", "resolvedModel": "claude-opus-5-5"}}}})));
    let reported = Subagent {
        id: Some("ac78".into()),
        model: Some("claude-opus-5-5".into()),
        background: Some(true),
        ..Subagent::default()
    };
    assert_eq!(launched, (None, Some(reported)));
    let asked = read(agent(json!({"sessionUpdate": "tool_call_update", "rawInput": {"subagent_type": "Explore",
        "run_in_background": true}})));
    assert_eq!(asked.1.and_then(|s| s.kind).as_deref(), Some("Explore"));
}

#[test]
fn other_calls_say_nothing_about_subagents() {
    let Event::ToolStarted(call) = event(json!({"sessionUpdate": "tool_call", "toolCallId": "t1",
        "rawInput": {"subagent_type": "x"}, "_meta": {"claudeCode": {"toolName": "Bash"}}}))
    else {
        panic!("a tool call")
    };
    assert_eq!((call.parent, call.subagent), (None, None));
}
