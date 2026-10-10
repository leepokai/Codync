//! A chat's lines: messages, turns, threads and reactions.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::{BTreeSet, HashMap};

use super::super::app::{App, Bot, Click, Entry, Exchange, Item, Kind, Outcome, Status, group};
use super::super::md::{self, truncate, width as w};
use super::buffer::u;
use super::chat::with_bg;
use super::permission::{decided_line, permission_card};
use super::style::{FACE, bot_color, face_style, person, spin, theme};
use super::time::{clock, when};

pub(super) struct Built {
    pub(super) lines: Vec<Line<'static>>,
    /// (line index, x offset, width, click)
    pub(super) buttons: Vec<(usize, u16, u16, Click)>,
    /// Each message's (entry id, first line, end line), for picking one to reply to.
    pub(super) messages: Vec<(String, usize, usize)>,
}

struct TurnInfo {
    steps: usize,
    files: BTreeSet<String>,
    /// The turn's last message to the user: the steps line goes under it.
    last_message: Option<String>,
}

fn turn_info(entries: &[&Entry]) -> HashMap<i64, TurnInfo> {
    let mut m: HashMap<i64, TurnInfo> = HashMap::new();
    for e in entries {
        let ti = m.entry(e.turn).or_insert_with(|| TurnInfo { steps: 0, files: BTreeSet::new(), last_message: None });
        match e.kind {
            Kind::Tool => {
                ti.steps += 1;
                for d in e.data["diffs"].as_array().into_iter().flatten() {
                    if let Some(p) = d["path"].as_str() {
                        ti.files.insert(p.to_owned());
                    }
                }
            }
            Kind::Agent if e.is_final() => ti.last_message = Some(e.id.clone()),
            _ => {}
        }
    }
    m
}

pub(super) fn gap(out: &mut Built) {
    if !out.lines.is_empty() {
        out.lines.push(Line::default());
    }
}

pub(super) fn name_style(color: &str) -> Style {
    let t = theme();
    if t.color { Style::default().fg(bot_color(color)).add_modifier(Modifier::BOLD) } else { t.bold }
}

/// "3 replies", "1 reply".
fn replies(n: i64) -> String {
    if n == 1 { "1 reply".into() } else { format!("{n} replies") }
}

pub(super) fn build_chat(app: &App, b: &Bot, width: usize) -> Built {
    let t = theme();
    let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
    let thread = app.thread.as_deref();
    let entries = app.lane(&b.id);
    let info = turn_info(&entries);
    if entries.is_empty() && thread.is_none() {
        intro(&mut out, app, b, width);
        return out;
    }
    if thread.is_none() && !app.history_complete(&b.id) {
        out.lines.push(Line::from(Span::styled(" ↑ older messages load when you scroll up", t.dim)));
    }
    if let Some(root) = thread {
        // The message the thread is on, then its replies.
        if let Some(e) = app.entries.get(&b.id).and_then(|m| m.values().find(|e| e.id == root)) {
            entry_lines(&mut out, app, b, e, &HashMap::new(), false, width);
        }
        let n = entries.iter().filter(|e| e.is_message()).count();
        let label = if n == 0 {
            " No replies yet ".to_owned()
        } else {
            format!(" {} ", replies(i64::try_from(n).unwrap_or(0)))
        };
        gap(&mut out);
        let side = width.saturating_sub(w(&label) + 4);
        out.lines.push(Line::from(Span::styled(format!(" ───{label}{}", "─".repeat(side)), t.dim)));
    }
    // A bot's messages arrive whole (Grok Bot's `send_message`); what it writes along the way is trace.
    for item in group(&b.id, &entries, thread.is_none()) {
        match item {
            Item::Entry(e) => entry_lines(&mut out, app, b, e, &info, thread.is_none(), width),
            Item::Group(g) => {
                let from = out.lines.len();
                let lead = format!("{} messages with ", g.count);
                bot_message_line(&mut out, app, g.first, &g.peer, &lead, g.failed, from);
            }
        }
    }
    // A turn in flight: who's on it and what they're doing.
    let last_turn = entries.iter().map(|e| e.turn).max().unwrap_or(0);
    // A message doesn't end the turn: the bot may send several.
    if b.status == Status::Working && b.works_in(thread) {
        gap(&mut out);
        out.lines.push(Line::from(Span::styled(format!(" {}", b.name), name_style(&b.color))));
        let act = if b.activity.is_empty() { "Working…".to_owned() } else { b.activity.clone() };
        out.lines.push(Line::from(vec![
            Span::styled(format!(" {} ", spin(app)), t.secondary),
            Span::styled(truncate(&act, width.saturating_sub(4)), t.secondary),
        ]));
        let steps = info.get(&last_turn).map_or(0, |ti| ti.steps);
        if steps > 0 {
            out.lines.push(Line::from(Span::styled(
                format!("   {steps} step{} so far · t shows them live", if steps == 1 { "" } else { "s" }),
                t.dim,
            )));
        }
    } else if matches!(b.status, Status::Working | Status::NeedsInput) && !b.works_in(thread) {
        // Busy somewhere else: another thread here, or a group it's in.
        let place = match b.away() {
            Some(g) => app.author_name(Some(g)),
            None if b.working_thread.is_some() => "a thread".to_owned(),
            None => "the chat".to_owned(),
        };
        gap(&mut out);
        let line = if b.status == Status::NeedsInput {
            Span::styled(format!(" ◆ Needs you in {place} · ! opens it"), t.amber)
        } else {
            Span::styled(format!(" {} Working in {place}", spin(app)), t.secondary)
        };
        out.lines.push(Line::from(line));
    }
    out
}

/// An empty chat: who this is and how it works.
fn intro(out: &mut Built, app: &App, b: &Bot, width: usize) {
    let t = theme();
    out.lines.push(Line::default());
    if !b.group {
        for row in FACE {
            out.lines.push(Line::from(vec![Span::raw("  "), Span::styled(row, face_style(&b.color))]));
        }
        out.lines.push(Line::default());
    }
    out.lines.push(Line::from(vec![Span::raw("  "), Span::styled(b.name.clone(), t.bold)]));
    let (meta, hello) = if b.group {
        let names: Vec<String> = b.members.iter().map(|m| app.author_name(Some(m))).collect();
        (
            format!("group · {}", names.join(", ")),
            "Everyone answers in turn unless you @mention someone. Each bot works in its own folder with its own tools."
                .to_owned(),
        )
    } else {
        (
            format!("{} · {}", b.backend, b.folder(&app.home)),
            format!("Say what you need. {} works in the background and pings you when it's done or needs you.", b.name),
        )
    };
    out.lines.push(Line::from(vec![Span::raw("  "), Span::styled(meta, t.secondary)]));
    if b.group && !b.description.is_empty() {
        out.lines.extend(md::wrap(
            &[Span::styled(b.description.clone(), t.text)],
            width,
            &[Span::raw("  ")],
            &[Span::raw("  ")],
        ));
    }
    out.lines.push(Line::default());
    out.lines.extend(md::wrap(&[Span::styled(hello, t.secondary)], width, &[Span::raw("  ")], &[Span::raw("  ")]));
}

/// One chat entry. `main`: the main chat, where a message shows its thread's replies line.
fn entry_lines(
    out: &mut Built,
    app: &App,
    b: &Bot,
    e: &Entry,
    info: &HashMap<i64, TurnInfo>,
    main: bool,
    width: usize,
) {
    let t = theme();
    // In a group each reply is its author's; elsewhere the bot's own.
    let author = if b.group { app.bots.get(e.data["author"].as_str().unwrap_or_default()) } else { Some(b) };
    match e.kind {
        Kind::User => {
            gap(out);
            let from = out.lines.len();
            let mut head = vec![
                Span::styled(" you", t.secondary.add_modifier(Modifier::BOLD)),
                Span::styled(format!(" {}", clock(e.created_at)), t.dim),
            ];
            match e.data["status"].as_str() {
                Some("queued") => head.push(Span::styled(" · queued", t.dim)),
                Some("cancelled") => head.push(Span::styled(" · not sent", t.red)),
                _ => {}
            }
            out.lines.push(Line::from(head));
            let mut body = if e.text().is_empty() {
                vec![]
            } else {
                md::wrap(
                    &[Span::styled(e.text().to_owned(), t.text)],
                    width.saturating_sub(1),
                    &[Span::raw(" › ")],
                    &[Span::raw("   ")],
                )
            };
            for (name, size) in e.attachments() {
                body.push(Line::from(vec![
                    Span::styled(" ▤ ", t.dim),
                    Span::styled(truncate(name, width.saturating_sub(14)), t.text),
                    Span::styled(format!("  {}", file_size(size)), t.dim),
                ]));
            }
            out.lines.extend(with_bg(body, width, t.band));
            reactions(out, e);
            out.messages.push((e.id.clone(), from, out.lines.len()));
            if main {
                thread_line(out, app, e);
            }
        }
        Kind::Agent if e.is_final() => {
            gap(out);
            let from = out.lines.len();
            let (name, color) = match author {
                Some(a) => (a.name.clone(), a.color.as_str()),
                None => ("A deleted bot".to_owned(), "gray"),
            };
            out.lines.push(Line::from(vec![
                Span::styled(format!(" {name}"), name_style(color)),
                Span::styled(format!(" {}", clock(e.created_at)), t.dim),
            ]));
            if let Some(files) = e.data["files"].as_array().filter(|files| !files.is_empty()) {
                for file in files {
                    let name = file["name"].as_str().unwrap_or("file");
                    let size = file["size"].as_u64().unwrap_or(0);
                    out.lines.push(Line::from(Span::styled(
                        format!(" ▤ {} · {}", truncate(name, width.saturating_sub(18)), file_size(size)),
                        t.text,
                    )));
                    out.lines.push(Line::from(Span::styled(
                        "   Select this message, then f to download · f again cancels",
                        t.dim,
                    )));
                }
            } else {
                out.lines.extend(md::render(e.text(), width, 1));
            }
            reactions(out, e);
            out.messages.push((e.id.clone(), from, out.lines.len()));
            if let Some(ti) = info.get(&e.turn).filter(|ti| ti.steps > 0 && ti.last_message.as_ref() == Some(&e.id)) {
                let files = if ti.files.is_empty() {
                    String::new()
                } else {
                    format!(" · {} file{} changed", ti.files.len(), if ti.files.len() == 1 { "" } else { "s" })
                };
                out.lines.push(Line::from(Span::styled(
                    format!("   ↳ {} step{}{files} · o opens them", ti.steps, if ti.steps == 1 { "" } else { "s" }),
                    t.dim,
                )));
            }
            if main {
                thread_line(out, app, e);
            }
        }
        Kind::Permission => {
            gap(out);
            if e.pending() {
                // A card in a group is its asking bot's.
                permission_card(out, app, e, author.unwrap_or(b), width);
            } else {
                decided_line(out, e);
            }
        }
        Kind::Notice => {
            let from = out.lines.len();
            if let Some(x) = Exchange::of(&b.id, e) {
                let lead = format!("{} ", x.verb());
                let failed = matches!(x.outcome, Outcome::Failed(_));
                bot_message_line(out, app, e, x.peer(), &lead, failed, from);
                return;
            }
            let text = if let Some(status) = e.data["connectionRequest"]["status"].as_str() {
                format!(
                    "{} · {} · {}",
                    e.data["connectionRequest"]["title"].as_str().unwrap_or("Connect service"),
                    e.notice_text(),
                    if status == "pending" { "C connects / cancels" } else { status }
                )
            } else {
                e.notice_text()
            };
            gap(out);
            match e.data["style"].as_str() {
                Some("divider") => {
                    let label = format!(" {text} · {} ", clock(e.created_at));
                    let side = width.saturating_sub(w(&label) + 4);
                    out.lines.push(Line::from(Span::styled(format!(" ───{label}{}", "─".repeat(side)), t.dim)));
                }
                Some("error") => out.lines.extend(md::wrap(
                    &[Span::styled(text, t.red)],
                    width,
                    &[Span::styled(" × ", t.red)],
                    &[Span::raw("   ")],
                )),
                _ => out.lines.extend(md::wrap(
                    &[Span::styled(text, t.dim)],
                    width,
                    &[Span::styled(" · ", t.dim)],
                    &[Span::raw("   ")],
                )),
            }
            if e.data["connectionRequest"].is_object() {
                out.messages.push((e.id.clone(), from, out.lines.len()));
            }
        }
        _ => {}
    }
}

/// A bot-to-bot exchange, or a run of them, as one compact line ("· Messaged ▌O▐ Owen",
/// "· 3 messages with ▌O▐ Owen"); click opens the conversation with `peer`.
/// Only a failure is marked: its text lives in the conversation sheet.
fn bot_message_line(out: &mut Built, app: &App, e: &Entry, peer: &str, lead: &str, failed: bool, from: usize) {
    let t = theme();
    gap(out);
    let (name, color) = app.bots.get(peer).map_or(("A deleted bot", "gray"), |p| (p.name.as_str(), p.color.as_str()));
    let mark = if failed { Span::styled(" × ", t.red) } else { Span::styled(" · ", t.dim) };
    let mut line = vec![
        Span::styled(lead.to_owned(), t.secondary),
        person(app, peer),
        Span::raw(" "),
        Span::styled(name.to_owned(), name_style(color)),
    ];
    if failed {
        line.push(Span::styled(" · failed", t.red));
    }
    let width = u(line.iter().map(|s| w(&s.content)).sum());
    out.buttons.push((out.lines.len(), 3, width, Click::BotChat(e.id.clone())));
    line.insert(0, mark);
    out.lines.push(Line::from(line));
    out.messages.push((e.id.clone(), from, out.lines.len()));
}

/// A message's reactions, under it.
fn reactions(out: &mut Built, e: &Entry) {
    let r: Vec<&str> = e.data["reactions"].as_array().into_iter().flatten().filter_map(|r| r.as_str()).collect();
    if !r.is_empty() {
        out.lines.push(Line::from(Span::raw(format!("   {}", r.join(" ")))));
    }
}

/// A user message on one line: its text, then its files.
pub(super) fn user_line(e: &Entry) -> String {
    let files = e.attachments().iter().map(|(n, _)| format!("▤ {n}")).collect::<Vec<_>>().join("  ");
    [e.text(), files.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("  ")
}

fn file_size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{} KB", bytes / 1024),
        _ => format!("{}.{} MB", bytes / 1_048_576, bytes % 1_048_576 * 10 / 1_048_576),
    }
}

/// Under a message with a thread: how many replies, how recently (click opens it).
fn thread_line(out: &mut Built, app: &App, e: &Entry) {
    let Some(text) = thread_summary(&e.data["thread"]) else { return };
    let mut line = vec![Span::raw("   "), Span::styled(text, theme().secondary)];
    // Who replied: one cell each, first reply first (the host keeps them distinct).
    let authors: Vec<&str> =
        e.data["thread"]["authors"].as_array().into_iter().flatten().filter_map(|a| a.as_str()).collect();
    for a in authors.iter().take(5) {
        line.push(Span::raw(" "));
        line.push(person(app, a));
    }
    if authors.len() > 5 {
        line.push(Span::styled(format!(" +{}", authors.len() - 5), theme().dim));
    }
    let width = u(line.iter().skip(1).map(|s| w(&s.content)).sum());
    out.buttons.push((out.lines.len(), 3, width, Click::Thread(e.id.clone())));
    out.lines.push(Line::from(line));
}

/// "↳ 3 replies · 12:31" (or "· 1 new") from the host's `data.thread`, or nothing without replies.
fn thread_summary(v: &serde_json::Value) -> Option<String> {
    let n = v["count"].as_i64().filter(|&n| n > 0)?;
    let at = match v["unread"].as_i64().unwrap_or(0) {
        0 => when(v["lastAt"].as_i64().unwrap_or(0)),
        unread => format!("{unread} new"),
    };
    Some(if at.is_empty() { format!("↳ {}", replies(n)) } else { format!("↳ {} · {at}", replies(n)) })
}

#[cfg(test)]
mod tests {
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

    fn app_with_bots() -> App {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(super::super::super::net::Client::new("http://127.0.0.1:1", None), tx, String::new());
        for (id, name) in [("a", "Egan"), ("b", "Owen")] {
            app.on_msg(super::super::super::app::Msg::Event(
                serde_json::json!({"type":"bot","bot":{"id":id,"name":name}}),
            ));
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
            thread_id: None,
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
            entry_lines(&mut out, &app, bot, e, &HashMap::new(), true, 80);
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
            for item in group(&bot.id, refs, true) {
                match item {
                    Item::Entry(e) => entry_lines(&mut out, &app, bot, e, &HashMap::new(), true, 80),
                    Item::Group(g) => {
                        let lead = format!("{} messages with ", g.count);
                        bot_message_line(&mut out, &app, g.first, &g.peer, &lead, g.failed, 0);
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
    fn thread_summary_counts_replies() {
        assert_eq!(thread_summary(&serde_json::json!({"count": 0})), None);
        assert_eq!(thread_summary(&serde_json::Value::Null), None);
        assert_eq!(thread_summary(&serde_json::json!({"count": 1})).as_deref(), Some("↳ 1 reply"));
        assert_eq!(thread_summary(&serde_json::json!({"count": 3, "lastAt": 0})).as_deref(), Some("↳ 3 replies"));
        assert_eq!(
            thread_summary(&serde_json::json!({"count": 3, "lastAt": 0, "unread": 1})).as_deref(),
            Some("↳ 3 replies · 1 new")
        );
    }
}
