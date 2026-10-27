use super::*;

#[test]
fn fuzzy_is_a_subsequence_match() {
    assert!(fuzzy("rx", &["Rex"]));
    assert!(fuzzy("", &["anything"]));
    assert!(!fuzzy("xr", &["Rex"]));
    assert!(fuzzy("web", &["Rex", "~/mycode/web"]));
}

#[tokio::test]
async fn the_chat_on_screen_is_marked_read() {
    let mut app = test_app();
    let bot = Bot::parse(&json!({"id": "b", "name": "Bot", "unread": 2})).expect("a bot with an id parses");
    app.bots.insert("b".into(), bot);
    app.selected = Some("b".into());
    (app.width, app.term_focused) = (Width::Wide, true);
    app.tick();
    assert!(app.reading.contains("b"));
}

#[tokio::test]
async fn r_on_a_picked_message_replies_to_it() {
    let mut app = test_app();
    app.bots.insert("b".into(), Bot::parse(&json!({"id": "b", "name": "Bot"})).expect("a bot with an id parses"));
    app.selected = Some("b".into());
    for (seq, kind) in [(1, "user"), (2, "agent")] {
        let v = json!({"botId": "b", "id": format!("e{seq}"), "seq": seq, "turn": 1, "kind": kind,
            "data": {"text": "hi", "final": true}});
        let (bot, e) = Entry::parse(&v).expect("a full entry parses");
        app.entries.entry(bot).or_default().insert(e.seq, e);
    }
    app.step_pick(0);
    assert_eq!(app.pick.as_deref(), Some("e2"), "a fresh pick is the newest message");
    app.step_pick(-1);
    let r = crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('r'));
    assert!(app.pick_key(r));
    assert_eq!(app.reply.as_deref(), Some("e1"));
    assert_eq!(app.replying_to().map(Entry::quoted).as_deref(), Some("hi"));
    assert!(app.typing && app.pick.is_none());
}

#[tokio::test]
async fn a_calls_messages_hide_behind_its_line() {
    let mut app = test_app();
    let messages = [
        (1, "user", json!({"text": "spoken", "call": "c"})),
        (2, "agent", json!({"text": "reply", "final": true, "call": "c"})),
        (3, "user", json!({"text": "on a live call", "call": "live"})),
        (4, "agent", json!({"text": "log.txt (3 bytes)", "final": true, "files": [{"name": "log.txt"}], "call": "c"})),
        (5, "notice", json!({"text": "Release plan", "callSeconds": 26, "call": "c"})),
    ];
    for (seq, kind, data) in messages {
        let v = json!({"botId": "b", "id": format!("e{seq}"), "seq": seq, "turn": 1, "kind": kind, "data": data});
        let (bot, e) = Entry::parse(&v).expect("a full entry parses");
        app.entries.entry(bot).or_default().insert(e.seq, e);
    }
    let shown: Vec<&str> = app.lane("b").iter().map(|e| e.id.as_str()).collect();
    assert_eq!(shown, ["e4", "e5"], "a call's messages hide, also while it lasts (on another device); a file stays");
    assert!(app.lane("b").last().is_some_and(|e| e.unfolds()), "the call's line opens its conversation");
}

#[tokio::test]
async fn lead_role_opens_from_the_keyboard_and_badge_without_changing_selection() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = test_app();
    for (id, lead) in [("lead", true), ("other", false)] {
        app.bots.insert(id.into(), Bot::parse(&json!({"id": id, "name": id, "lead": lead})).expect("valid bot"));
    }
    app.selected = Some("other".into());
    app.on_key(KeyEvent::from(KeyCode::Char('I')));
    assert!(app.overlays.is_empty(), "ordinary bots have no lead explanation");
    app.selected = Some("lead".into());
    app.on_key(KeyEvent::from(KeyCode::Char('I')));
    assert!(matches!(app.overlays.last(), Some(Overlay::LeadAgent(0))));
    app.on_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.overlays.is_empty());
    app.selected = Some("other".into());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw roster");
    let badge = app.hits.clicks.iter().find(|(_, hit)| matches!(hit, Click::LeadAgent)).expect("lead badge").0;
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: badge.x,
        row: badge.y,
        modifiers: KeyModifiers::NONE,
    });
    assert!(matches!(app.overlays.last(), Some(Overlay::LeadAgent(0))));
    assert_eq!(app.selected.as_deref(), Some("other"));
    app.on_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.overlays.is_empty());
}

#[tokio::test]
async fn context_meter_opens_details_with_keyboard_and_mouse() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = test_app();
    app.bots.insert(
        "b".into(),
        Bot::parse(&json!({"id": "b", "contextUsage": {
            "usedTokens": 160_000, "maxTokens": 200_000, "percent": 80, "level": "warning"
        }}))
        .expect("bot"),
    );
    app.selected = Some("b".into());
    app.on_key(KeyEvent::from(KeyCode::Char('W')));
    assert!(matches!(app.overlays.last(), Some(Overlay::ContextUsage)));
    app.on_key(KeyEvent::from(KeyCode::Esc));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw");
    let meter = app.hits.clicks.iter().find(|(_, hit)| matches!(hit, Click::ContextUsage)).expect("meter").0;
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: meter.x,
        row: meter.y,
        modifiers: KeyModifiers::NONE,
    });
    assert!(matches!(app.overlays.last(), Some(Overlay::ContextUsage)));
    terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("detail");
    let screen = format!("{:?}", terminal.backend().buffer());
    assert!(screen.contains("160000 / 200000 tokens"), "{screen}");
}
