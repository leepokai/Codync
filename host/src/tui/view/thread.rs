//! A task's thread: its thread's transcript, with a dim line for each thing the thread put in the bot's chat (a card, a
//! file), and the composer that writes to it, or why it takes no message.

use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};

use super::super::app::{App, Entry, Kind, Source, WorkStatus, WorkTask, nested, shared_line, tilde};
use super::super::l10n::tr;
use super::super::manage::{Fetched, ThreadSheet};
use super::super::md::{self, truncate};
use super::buffer::{centered, frame_box, hline, put, put_line, rput, u};
use super::chat::{composer_lines, file_names};
use super::queue::{QueueKeys, queued_rows};
use super::set_cursor;
use super::style::theme;
use super::time::elapsed;
use super::trace::push_nested;
use super::work::{mark, standing};

/// What a thread was told shows this many lines until `T` unfolds it.
const TOLD_LINES: usize = 6;

/// Where a task's thread works, for its transcript's heading in `room` cells: the branch of its worktree while it has
/// one, else its folder, cut in the middle so its last name stays.
fn place(app: &App, task: &WorkTask, room: usize) -> Option<String> {
    let branch = task.branch.as_ref().map(|branch| truncate(&format!("⎇ {branch}"), room));
    branch.or_else(|| task.folder.as_ref().map(|folder| md::truncate_path(&tilde(folder, &app.home), room)))
}

/// Why a task's thread takes no message, where a client says so: its result has an owner already.
fn read_only(task: &WorkTask) -> Option<Cow<'static, str>> {
    Some(match task.source.as_ref()? {
        Source::Routine => tr("Read only: a routine's run"),
        Source::Request(_) => tr("Read only: another bot's request"),
    })
}

/// What the transcript's keys are for.
#[derive(Clone, Copy)]
enum Keys {
    /// The composer's: the message waits behind a turn that runs.
    Typing,
    /// A line of what the thread put in the chat is picked.
    Picked,
    /// The page's, with `tab` when it has such lines.
    Reading { lines: bool },
}

/// The transcript's keys, on its last line.
fn thread_keys(live: bool, keys: Keys) -> String {
    let lines = match keys {
        Keys::Typing => return format!(" ↵ {} · esc", if live { "sends after this turn" } else { "sends" }),
        Keys::Picked => return " ↵ shows it in the chat · tab ⇧tab another · esc".into(),
        Keys::Reading { lines } => lines,
    };
    let stop = live.then(|| tr("X stops"));
    let tab = lines.then_some(Cow::Borrowed("tab picks a line"));
    let keys: Vec<_> = stop.into_iter().chain(tab).chain([tr("T full"), tr("↑↓ scroll · esc close")]).collect();
    format!(" {}", keys.join(" · "))
}

/// The thread's composer over `bottom`, under the messages that wait for their turn, as in a chat. Where it starts.
fn thread_composer(buf: &mut Buffer, inner: Rect, app: &App, s: &ThreadSheet, bottom: u16) -> u16 {
    let t = theme();
    let (lines, cursor) = composer_lines(&s.draft, usize::from(inner.width.saturating_sub(3)));
    let rows = lines.len().clamp(1, 3);
    let first = lines.len().saturating_sub(rows).min(cursor.1.saturating_sub(rows - 1));
    let top = bottom.saturating_sub(u(rows));
    put(buf, inner.x, top, 1, "›", if s.typing { t.bold } else { t.dim }.patch(t.panel));
    if s.draft.text.is_empty() {
        put(buf, inner.x + 2, top, inner.width.saturating_sub(2), &tr("Message this thread"), t.dim.patch(t.panel));
        if !s.typing {
            rput(buf, inner.right(), top, "i to type", t.dim.patch(t.panel));
        }
    } else {
        for (i, line) in lines.iter().skip(first).take(rows).enumerate() {
            put(buf, inner.x + 2, top + u(i), inner.width.saturating_sub(2), line, t.text.patch(t.panel));
        }
    }
    if s.typing {
        set_cursor(Position::new(inner.x + 2 + u(cursor.0), top + u(cursor.1.saturating_sub(first))));
    }
    // Files going with the next message: one row over the field, as in a chat.
    let top = if s.files.is_empty() {
        top
    } else {
        let row = top.saturating_sub(1);
        let hint = if s.typing && s.draft.text.is_empty() { "⌫ removes" } else { "" };
        let room = rput(buf, inner.right(), row, hint, t.dim.patch(t.panel)).saturating_sub(inner.x + 2);
        put(buf, inner.x + 2, row, room, &file_names(&s.files), t.secondary.patch(t.panel));
        row
    };
    let keys = if s.typing { QueueKeys::AfterEsc } else { QueueKeys::Ready };
    let waiting = queued_rows(&app.thread_entries.queued(&s.thread), keys);
    let strip = top.saturating_sub(u(waiting.len()));
    for (i, (text, hint)) in waiting.iter().enumerate() {
        let y = strip + u(i);
        let room = rput(buf, inner.right(), y, hint, t.dim.patch(t.panel)).saturating_sub(inner.x + 1);
        put(buf, inner.x, y, room, &truncate(text, usize::from(room)), t.secondary.patch(t.panel));
    }
    strip
}

/// A thread's transcript: its task's title, how it stands and where it works, then its entries in the trace's rows
/// and a line for what it put in the chat, the picked one marked. The bot's own task has a composer under them; a
/// routine's run and another bot's request say why they have none.
pub(super) fn thread_details(buf: &mut Buffer, area: Rect, app: &App, s: &ThreadSheet) {
    let t = theme();
    let r = centered(area, 100, area.height.saturating_sub(4));
    let inner = frame_box(buf, r, t.text, t.panel, None);
    if inner.height < 6 {
        return;
    }
    let task = s.task(app);
    let live = task.is_some_and(|task| task.status.live());
    let talks = task.is_some_and(|task| task.talk);
    if let Some(task) = task {
        put(
            buf,
            inner.x,
            inner.y,
            inner.width,
            &truncate(&task.title, usize::from(inner.width)),
            t.bold.patch(t.panel),
        );
        let (glyph, style) = mark(task.status == WorkStatus::NeedsYou);
        let style = if live { style } else { t.dim };
        let x = put(buf, inner.x, inner.y + 1, 1, if live { glyph } else { "·" }, style.patch(t.panel)) + 1;
        let mut line = standing(task);
        let room = inner.right().saturating_sub(x);
        if let Some(place) = place(app, task, usize::from(room).saturating_sub(md::width(&line) + 3)) {
            line = format!("{line} · {place}");
        }
        put(buf, x, inner.y + 1, room, &line, t.secondary.patch(t.panel));
    } else {
        put(buf, inner.x, inner.y, inner.width, &tr("Work"), t.bold.patch(t.panel));
    }
    hline(buf, inner.x, inner.y + 2, inner.width, t.dim.patch(t.panel));
    let foot = inner.bottom() - 1;
    let keys = if talks && s.typing {
        Keys::Typing
    } else if s.pick.is_some() {
        Keys::Picked
    } else {
        Keys::Reading { lines: !app.shared(&s.bot, &s.thread).is_empty() }
    };
    put(buf, inner.x, foot, inner.width, &thread_keys(live, keys), t.dim.patch(t.panel));
    // Between the transcript and the keys: what waits in the thread over its composer, or why it has none.
    let mut bottom = foot;
    if talks {
        bottom = thread_composer(buf, inner, app, s, foot);
    } else if let Some(why) = task.and_then(read_only) {
        bottom = foot - 1;
        put(buf, inner.x, bottom, inner.width, &format!(" {why}"), t.dim.patch(t.panel));
    }
    // Over them: what the thread's agent runs in the background, as a chat's capsule shows its tasks.
    if let Some((what, since)) = task.and_then(|task| task.background.as_ref()) {
        bottom = bottom.saturating_sub(1);
        let (glyph, style) = mark(false);
        put(buf, inner.x, bottom, 1, glyph, style.patch(t.panel));
        let room =
            rput(buf, inner.right(), bottom, &elapsed(Some(*since)), t.dim.patch(t.panel)).saturating_sub(inner.x + 3);
        let line = format!("{} · {what}", tr("Background"));
        put(buf, inner.x + 2, bottom, room, &truncate(&line, usize::from(room)), t.secondary.patch(t.panel));
    }
    if bottom < foot {
        bottom = bottom.saturating_sub(1);
        hline(buf, inner.x, bottom, inner.width, t.dim.patch(t.panel));
    }

    let body = Rect::new(inner.x, inner.y + 3, inner.width, bottom.saturating_sub(inner.y + 3));
    let (lines, picked) = detail_lines(app, s, usize::from(body.width));
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    s.max_scroll.set(max);
    s.page.set(h);
    let mut start = lines.len().saturating_sub(h + s.scroll.get().min(max));
    // The picked line on screen.
    if let Some(at) = picked.filter(|&at| at < start || at >= start + h) {
        start = at.saturating_sub(h / 2).min(lines.len().saturating_sub(h));
        s.scroll.set(lines.len().saturating_sub(h + start));
    }
    for (i, l) in lines.iter().skip(start).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
    if let Some(at) = picked.filter(|&at| at >= start && at < start + h) {
        put(buf, body.x, body.y + u(at - start), 1, "▌", t.amber.patch(t.panel));
    }
}

/// The transcript's lines, oldest first, with a dim line for what the thread put in the bot's chat (a card, answered or
/// not, a file, a voice note, an artifact; `shared_line`) where it came; with none, that it loads or has nothing yet.
/// A load that failed says so first, with the key that tries again. Also where the picked line is.
fn detail_lines(app: &App, s: &ThreadSheet, width: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let t = theme();
    let mut out: Vec<Line<'static>> = vec![];
    if let Fetched::Failed(error) = &s.fetched {
        out.push(Line::from(Span::styled(format!(" {error} · {}", tr("r retries")), t.red)));
    }
    let own = app.thread_entries.of(&s.thread).map(|e| (e, false));
    let shared = app.shared(&s.bot, &s.thread).into_iter().map(|e| (e, true));
    let mut entries: Vec<(&Entry, bool)> = own.chain(shared).collect();
    // Stable: at the same moment the thread's own entry comes first.
    entries.sort_by_key(|(e, _)| e.created_at);
    let mut picked = None;
    for ((e, shared), depth) in nested(&entries, |(e, _)| e) {
        if shared {
            if s.pick.as_ref() == Some(&e.id) {
                picked = Some(out.len());
            }
            if let Some((mark, text)) = shared_line(e) {
                out.push(Line::from(Span::styled(truncate(&format!(" {mark} {text}"), width), t.dim)));
            }
        } else if e.kind == Kind::User && e.data["from"] == "host" {
            told(&mut out, e, width, s.full);
        } else if e.queued() {
            // The user's message that waits for its turn: over the composer, not here.
        } else {
            push_nested(&mut out, app, e, depth, width, s.full);
        }
    }
    if out.is_empty() {
        let note = if matches!(s.fetched, Fetched::Loading) { tr("Loading…") } else { tr("Nothing yet.") };
        out.push(Line::from(Span::styled(format!(" {note}"), t.dim)));
    }
    (out, picked)
}

/// What the thread was told (its task, a later one that follows it): quoted text, not a message of the user's.
fn told(out: &mut Vec<Line<'static>>, e: &Entry, width: usize, full: bool) {
    let t = theme();
    let bar = [Span::styled(" ▏ ", t.dim)];
    let lines = md::wrap(&[Span::styled(e.text().trim().to_owned(), t.secondary)], width, &bar, &bar);
    let hidden = if full { 0 } else { lines.len().saturating_sub(TOLD_LINES) };
    let shown = lines.len() - hidden;
    out.extend(lines.into_iter().take(shown));
    if hidden > 0 {
        out.push(Line::from(Span::styled(format!(" ▏ … {hidden} more lines · T shows everything"), t.dim)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{Msg, screen, test_app};
    use serde_json::json;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    /// Bot `a` with this `work`.
    fn bot(app: &mut App, work: &serde_json::Value) {
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
    }

    #[tokio::test]
    async fn the_transcript_quotes_what_the_thread_was_told_and_shows_its_steps() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1"},
            {"id": "x", "title": "Old", "status": "done", "thread": "x", "startedAt": 1000, "endedAt": 66_000}]),
        );
        let sheet = ThreadSheet::new("a".into(), "i".into(), "thread-1".into());
        assert_eq!(text(&detail_lines(&app, &sheet, 60).0), [" Loading…"]);
        let entry = |seq: i64, kind: &str, data: serde_json::Value| {
            json!({"type": "threadEntry", "rev": seq, "thread": "thread-1", "entry": {"id": format!("t{seq}"),
                "botId": "thread-1", "seq": seq, "rev": seq, "kind": kind, "turn": 1, "data": data}})
        };
        app.on_msg(Msg::Event(entry(1, "user", json!({"from": "host", "text": "Fix the login redirect."}))));
        app.on_msg(Msg::Event(entry(2, "thought", json!({"text": "Where is the redirect?"}))));
        let lines = text(&detail_lines(&app, &sheet, 60).0);
        assert_eq!(lines[0], " ▏ Fix the login redirect.", "plain quoted text, not the user's message");
        assert!(lines[1].contains("thought") && lines[1].contains("Where is the redirect?"), "{lines:?}");

        let over = ThreadSheet::new("a".into(), "x".into(), "x".into());
        let failed = ThreadSheet { fetched: Fetched::Failed("offline".into()), ..over };
        assert_eq!(text(&detail_lines(&app, &failed, 60).0), [" offline · r retries"]);
        assert_eq!(failed.task(&app).map(standing).as_deref(), Some("Done · 1m05s"));
        let empty = ThreadSheet { fetched: Fetched::Loaded, ..failed };
        assert_eq!(text(&detail_lines(&app, &empty, 60).0), [" Nothing yet."]);
    }

    #[tokio::test]
    async fn the_thread_shows_what_its_thread_put_in_the_chat_where_it_came() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "needsYou", "thread": "thread-1", "talk": true}]),
        );
        app.on_msg(Msg::Event(json!({"type": "threadEntry", "rev": 1, "thread": "thread-1", "entry": {"id": "t1",
            "botId": "thread-1", "seq": 1, "rev": 1, "kind": "thought", "turn": 1, "createdAt": 10,
            "data": {"text": "Reading the logs"}}})));
        let thread = json!({"id": "thread-1", "title": "Fix login"});
        let chat = |id: &str, seq: i64, at: i64, kind: &str, data: serde_json::Value| {
            json!({"type": "entry", "entry": {"id": id, "botId": "a", "seq": seq, "kind": kind, "turn": 1,
                "createdAt": at, "data": data}})
        };
        let file = json!({"text": "log.txt (3 bytes)", "final": true, "thread": thread, "task": "i",
            "files": [{"id": "f1", "name": "log.txt", "size": 3}]});
        let card = json!({"status": "pending", "title": "Run tests", "thread": thread, "task": "i",
            "options": [{"optionId": "ok", "name": "Allow", "kind": "allow_once"}]});
        app.on_msg(Msg::Event(chat("file", 1, 5, "agent", file)));
        app.on_msg(Msg::Event(chat("card", 2, 20, "permission", card)));
        app.on_msg(Msg::Event(chat("own", 3, 30, "agent", json!({"text": "The chat's own reply", "final": true}))));
        let mut sheet =
            ThreadSheet { fetched: Fetched::Loaded, ..ThreadSheet::new("a".into(), "i".into(), "thread-1".into()) };
        let (lines, picked) = detail_lines(&app, &sheet, 60);
        let lines = text(&lines);
        let at = |text: &str| lines.iter().position(|line| line == text);
        let (file, card) = (at(" ▤ Sent a file: log.txt"), at(" ◆ Waiting for your approval: Run tests"));
        let step = lines.iter().position(|line| line.contains("Reading the logs"));
        assert!(file.is_some() && file < step && step < card, "one dim line each, in time order: {lines:?}");
        assert!(!lines.iter().any(|line| line.contains("The chat's own reply")), "only what this thread put there");
        assert_eq!(picked, None);

        // `tab` picks the newest line first, then the one before.
        sheet.step_pick(&app, true);
        assert_eq!((sheet.pick.as_deref(), detail_lines(&app, &sheet, 60).1), (Some("card"), card));
        sheet.step_pick(&app, true);
        sheet.step_pick(&app, true);
        assert_eq!(
            (sheet.pick.as_deref(), detail_lines(&app, &sheet, 60).1),
            (Some("file"), file),
            "it stops at the first"
        );
    }

    #[tokio::test]
    async fn the_transcript_heading_names_the_branch_of_its_worktree() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1",
                "worktree": {"branch": "codync/fix-login", "path": "/w/a/i"}},
            {"id": "j", "title": "Docs", "status": "working", "thread": "thread-2"},
            {"id": "k", "title": "Far", "status": "working", "thread": "thread-3",
                "folder": format!("/private/tmp/{}/gitrepo", "x".repeat(120))}]),
        );
        let area = Rect::new(0, 0, 100, 20);
        let heading = |task: &str| {
            let sheet = ThreadSheet::new("a".into(), task.into(), "thread".into());
            let lines = screen(area, |buf| thread_details(buf, area, &app, &sheet));
            let standing = lines.iter().find(|line| line.contains("Working")).expect("how it stands");
            standing.trim().trim_matches('│').trim().to_owned()
        };
        assert_eq!(heading("i"), "◷ Working · ⎇ codync/fix-login");
        assert_eq!(heading("j"), "◷ Working", "a host that names no folder");
        let far = heading("k");
        assert!(
            far.starts_with("◷ Working · /private/tmp/x") && far.ends_with("x…/gitrepo"),
            "its last name stays: {far}"
        );
    }

    #[tokio::test]
    async fn the_transcript_has_a_composer_for_the_bots_own_task_and_says_why_another_has_none() {
        let mut app = test_app();
        app.home = "/Users/me".into();
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true,
                "folder": "/Users/me/code/app"},
            {"id": "r", "title": "Digest", "status": "working", "thread": "thread-2", "source": {"kind": "routine"}},
            {"id": "m", "title": "Review", "status": "done", "thread": "thread-3", "source": {"kind": "request"}}]),
        );
        let entry = |seq: i64, data: serde_json::Value| {
            json!({"type": "threadEntry", "rev": seq, "thread": "thread-1", "entry": {"id": format!("t{seq}"),
                "botId": "thread-1", "seq": seq, "rev": seq, "kind": "user", "turn": seq, "data": data}})
        };
        app.on_msg(Msg::Event(entry(1, json!({"text": "use tabs", "status": "sent"}))));
        app.on_msg(Msg::Event(entry(2, json!({"text": "and the tests", "status": "queued"}))));
        let area = Rect::new(0, 0, 100, 24);
        let draw = |app: &App, sheet: &ThreadSheet| {
            let lines = screen(area, |buf| thread_details(buf, area, app, sheet));
            lines.iter().map(|line| line.trim().trim_matches('│').trim().to_owned()).collect::<Vec<_>>()
        };
        let mut own = ThreadSheet::new("a".into(), "i".into(), "thread-1".into());
        let lines = draw(&app, &own);
        let at = |lines: &[String], text: &str| lines.iter().position(|line| line.contains(text));
        assert!(at(&lines, "◷ Working · ~/code/app").is_some(), "where it works: {lines:?}");
        assert!(at(&lines, "you").is_some_and(|row| lines[row].contains("use tabs")), "what was sent is a step");
        let waits = at(&lines, "› and the tests").expect("what waits, over the composer");
        assert!(lines[waits].ends_with("queued · S sends now · D removes"), "{lines:?}");
        assert!(
            lines[waits + 1].starts_with("› Message this thread") && lines[waits + 1].ends_with("i to type"),
            "{lines:?}"
        );
        assert!(lines[waits + 2].contains("X stops · T full"), "{lines:?}");
        assert_eq!(lines.iter().filter(|line| line.contains("and the tests")).count(), 1, "not among the steps");

        own.typing = true;
        own.draft.insert("one more");
        let lines = draw(&app, &own);
        let composer = at(&lines, "› one more").expect("the draft");
        assert!(lines[composer - 1].ends_with("queued · esc · S sends now · D removes"), "{lines:?}");
        assert_eq!(lines[composer + 1], "↵ sends after this turn · esc");

        // Read-only threads: no composer, one quiet line saying why.
        let run = draw(&app, &ThreadSheet::new("a".into(), "r".into(), "thread-2".into()));
        assert!(
            at(&run, "Read only: a routine's run").is_some() && at(&run, "Message this thread").is_none(),
            "{run:?}"
        );
        let asked = draw(&app, &ThreadSheet::new("a".into(), "m".into(), "thread-3".into()));
        assert!(at(&asked, "Read only: another bot's request").is_some(), "{asked:?}");

        // What its agent runs in the background: one line over the composer, as a chat's capsule.
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true,
                "background": [{"id": "w", "kind": "workflow", "name": "probe", "activity": "Count: two",
                    "startedAt": 0}]}]),
        );
        let lines = draw(&app, &ThreadSheet::new("a".into(), "i".into(), "thread-1".into()));
        let running = at(&lines, "Background · probe · Count: two").expect("the background line");
        assert!(lines[running + 1].starts_with("› and the tests"), "over what waits and the composer: {lines:?}");
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true,
            "background": [{"id": "w", "name": "probe", "startedAt": 0}, {"id": "s", "name": "Sleep", "startedAt": 1}]}]),
        );
        let lines = draw(&app, &ThreadSheet::new("a".into(), "i".into(), "thread-1".into()));
        assert!(at(&lines, "Background · probe, Sleep").is_some(), "several, by name: {lines:?}");
    }
}
