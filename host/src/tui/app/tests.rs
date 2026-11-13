use super::*;

#[tokio::test]
async fn routines_keep_the_chat_draft_until_the_user_returns_to_chat() {
    use crate::tui::manage::RoutineList;
    use crossterm::event::{KeyCode, KeyEvent};

    let mut app = test_app();
    app.selected = Some("b".into());
    app.drafts.insert("b".into(), Editor::with("My unfinished message"));
    app.overlays.push(Overlay::Routines(RoutineList {
        bot: "b".into(),
        items: Some(Vec::new()),
        runs: Vec::new(),
        cursor: 0,
    }));

    app.on_key(KeyEvent::from(KeyCode::Char('c')));
    assert!(matches!(app.overlays.last(), Some(Overlay::Routines(_))));
    assert_eq!(app.drafts["b"].text, "My unfinished message");

    app.on_key(KeyEvent::from(KeyCode::Char('n')));
    assert!(matches!(app.overlays.last(), Some(Overlay::Routine(_))));
    assert_eq!(app.drafts["b"].text, "My unfinished message");
}

#[tokio::test]
async fn the_dev_servers_sheet_offers_what_each_server_allows() {
    use crate::tui::manage::DevServersSheet;
    use crossterm::event::{KeyCode, KeyEvent};

    fn sheet(app: &App) -> &DevServersSheet {
        let Some(Overlay::DevServers(sheet)) = app.overlays.last() else { panic!("the sheet is open") };
        sheet
    }

    let failed = json!({"id": "a", "name": "web", "command": "npm run dev", "folder": "/site", "status": "failed",
        "ports": [], "startedAt": null, "exitCode": 3, "managed": true});
    let shell = json!({"id": "detected:9", "name": "node", "command": "node", "folder": null, "status": "running",
        "ports": [{"port": 5173, "local": "http://localhost:5173", "remote": []}], "startedAt": null, "exitCode": null,
        "managed": false});
    let mut app = test_app();
    let bot = json!({"id": "b", "name": "Bot", "devServers": [failed, shell]});
    app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot with servers parses"));
    app.selected = Some("b".into());
    assert_eq!(app.bots["b"].dev_servers.len(), 2, "what the host sends decodes");

    app.open_dev_servers();
    assert_eq!(sheet(&app).selected, 0);

    // Enter on one that failed reads why, and its log says how it ended.
    app.on_key(KeyEvent::from(KeyCode::Enter));
    let logs = sheet(&app).logs.as_ref().expect("the log opened");
    assert_eq!(logs.detail, "npm run dev · Exit code 3");
    app.on_key(KeyEvent::from(KeyCode::Char('r')));
    assert!(sheet(&app).logs.is_some(), "a refresh stays in the log");
    app.on_key(KeyEvent::from(KeyCode::Esc));
    assert!(sheet(&app).logs.is_none(), "esc goes back to the list");

    // One started through the shell has no log to read.
    app.on_key(KeyEvent::from(KeyCode::Down));
    assert_eq!(sheet(&app).selected, 1);
    app.on_key(KeyEvent::from(KeyCode::Char('l')));
    assert!(sheet(&app).logs.is_none());

    // Attached from elsewhere, its address shows instead of a browser opening here.
    app.on_this_computer = false;
    app.on_key(KeyEvent::from(KeyCode::Enter));
    let (hint, _) = app.hint.as_ref().expect("a hint shows");
    assert!(hint.starts_with("http://localhost:5173 · "), "{hint}");

    app.on_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.overlays.is_empty());
}

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
async fn the_settings_form_scrolls_to_keep_the_focused_field_in_sight() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = test_app();
    app.bots.insert("b".into(), Bot::parse(&json!({"id": "b", "name": "B", "backend": "claude"})).expect("valid bot"));
    app.selected = Some("b".into());
    app.edit_bot();
    let mut drawn = |field: usize| {
        let Some(Overlay::Form(f)) = app.overlays.last_mut() else { panic!("the editor opens") };
        f.field = field;
        let mut terminal = Terminal::new(TestBackend::new(100, 44)).expect("terminal");
        terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw the form");
        terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>()
    };
    assert!(!drawn(0).contains("Skills"), "more fields than the form has rows");
    assert!(drawn(FIELDS.len() - 1).contains("Skills"), "scrolled to the focused one");
}

#[tokio::test]
async fn a_long_field_help_wraps_to_a_second_row() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = test_app();
    let bot = json!({"id": "b", "name": "B", "backend": "claude", "compaction": "early",
        "compactionTokens": {"min": 50_000, "max": 960_000, "step": 10_000, "presets": [100_000]}});
    app.bots.insert("b".into(), Bot::parse(&bot).expect("valid bot"));
    app.selected = Some("b".into());
    app.edit_bot();
    let Some(Overlay::Form(f)) = app.overlays.last_mut() else { panic!("the editor opens") };
    f.field = FIELDS.iter().position(|x| *x == Field::Compaction).expect("a compaction field");
    let mut terminal = Terminal::new(TestBackend::new(100, 44)).expect("terminal");
    terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw the form");
    let drawn = terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    assert!(drawn.contains("50K–960K"), "the sizes one can type, after a description longer than a row");
}

#[test]
fn update_notices_name_the_side_that_needs_updating() {
    use crate::compat::Mismatch;
    assert_eq!(
        mismatch_text(&Mismatch::UpdateApp { minimum: "2.46.3".into() }, "Office Mac"),
        "Update this app: the app on this computer needs Codync 2.46.3 or newer (Office Mac requires it)."
    );
    assert_eq!(
        mismatch_text(&Mismatch::UpdateHost { version: "2.20.0".into(), minimum: "2.46.3".into() }, "Office Mac"),
        "Update Codync on Office Mac: it runs 2.20.0; that computer needs Codync 2.46.3 or newer. ^k → Check for updates."
    );
}
