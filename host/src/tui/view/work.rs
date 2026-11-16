//! A bot's event box: the line over the composer, an item's row on the Details page, a task's read-only transcript
//! and the question which item gives way.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::super::app::{App, Background, Bot, Entry, Kind, Source, WorkItem, WorkStatus, word};
use super::super::l10n::{fill, tr};
use super::super::manage::{CutIn, DetailRow, Fetched, TaskSheet, giving_way};
use super::super::md::{self, truncate, width as w};
use super::buffer::{centered, frame_box, hline, put, put_line, restyle, rput, u};
use super::style::theme;
use super::time::{elapsed, span};
use super::trace::push_step;

/// What a task was told shows this many lines until `T` unfolds it.
const TOLD_LINES: usize = 6;

/// Draws what `bot` does in the background on the line over `bottom` (the composer); where that line starts, or
/// `bottom` when it does nothing there.
pub(super) fn work_band(buf: &mut Buffer, r: Rect, bot: &Bot, bottom: u16, typing: bool) -> u16 {
    let Some(background) = Background::of(&bot.work).filter(|_| !bot.group) else { return bottom };
    let t = theme();
    let y = bottom.saturating_sub(1);
    let hint_x = rput(buf, r.right() - 1, y, &band_keys(typing, background.stops.is_some()), t.dim);
    let (glyph, style) = mark(background.needs_you);
    put(buf, r.x + 1, y, 1, glyph, style);
    let room = hint_x.saturating_sub(r.x + 5);
    put(
        buf,
        r.x + 3,
        y,
        room,
        &band_text(&background.what, &elapsed(background.since), usize::from(room)),
        t.secondary,
    );
    y
}

/// A clock while it works, the amber diamond while it waits on the user.
fn mark(needs_you: bool) -> (&'static str, Style) {
    let t = theme();
    if needs_you { ("◆", t.amber) } else { ("◷", t.secondary) }
}

/// The band's keys; typing, they wait for esc. `X` stops the one item a task has.
fn band_keys(typing: bool, stops: bool) -> String {
    let keys = [Some(tr("B opens")), stops.then(|| tr("X stops"))];
    let esc = typing.then_some("esc".into());
    esc.into_iter().chain(keys.into_iter().flatten()).collect::<Vec<_>>().join(" · ")
}

/// `In the background · <what> · <time>` in `room` cells: the middle part gives way.
fn band_text(what: &str, time: &str, room: usize) -> String {
    let head = format!("{} · ", tr("In the background"));
    let tail = if time.is_empty() { String::new() } else { format!(" · {time}") };
    format!("{head}{}{tail}", truncate(what, room.saturating_sub(w(&head) + w(&tail))))
}

/// How long an item has run, or ran; empty for one that never began.
fn time(item: &WorkItem) -> String {
    match (item.started_at, item.ended_at) {
        (Some(start), Some(end)) => span(start, end),
        (start, _) if item.status.live() => elapsed(start),
        _ => String::new(),
    }
}

/// An item's status in a word and its time.
fn standing(item: &WorkItem) -> String {
    let (status, time) = (tr(word(item.status)), time(item));
    if time.is_empty() { status.into_owned() } else { format!("{status} · {time}") }
}

/// What the card of `item`, waiting in `bot`'s chat, asks: an approval's title, a choice's question, the agent a
/// sign-in waits on.
fn asks(app: &App, bot: &str, item: &WorkItem) -> Option<String> {
    let waits = |e: &&Entry| (e.pending() || e.waits_for_sign_in()) && e.data["item"] == item.id.as_str();
    let card = app.entries.get(bot)?.values().rev().find(waits)?;
    if let Some(agent) = card.data["connectionRequest"]["title"].as_str() {
        return Some(fill(&tr("Needs {agent} sign-in"), &[("agent", agent)]));
    }
    card.data["title"].as_str().or_else(|| card.data["question"].as_str()).map(str::to_owned)
}

/// Where an item came from, before its title: the routine mark for a routine's run, the asking bot's name for a
/// request from a bot on this computer; nothing for the bot's own work.
fn source_mark(app: &App, item: &WorkItem) -> Option<String> {
    match item.source.as_ref()? {
        Source::Routine => Some("↻".into()),
        Source::Request(from) => app.bots.get(from.as_deref()?).map(|b| format!("{} ›", truncate(&b.name, 12))),
    }
}

/// An item's mark, and what its row says after the title: a thread that needs the user says what its card asks.
fn mark_and_line(app: &App, bot: &str, row: &DetailRow<'_>, item: &WorkItem) -> (String, Style, String, Style) {
    let t = theme();
    match (row, item.status) {
        (DetailRow::Waiting(place, _), _) => (place.to_string(), t.secondary, item.line(), t.secondary),
        (_, WorkStatus::NeedsYou) => {
            let ask = asks(app, bot, item).filter(|ask| !ask.is_empty());
            ("◆".into(), t.amber, ask.unwrap_or_else(|| item.line()), t.amber)
        }
        (_, WorkStatus::Failed | WorkStatus::Stopped) => ("!".into(), t.red, item.line(), t.red),
        (_, WorkStatus::Done) => ("○".into(), t.dim, item.line(), t.secondary),
        _ => ("●".into(), t.bold, item.line(), t.secondary),
    }
}

/// An item's row on the Details page, on `base` (its background): its mark or place in line, its title, what it
/// does or why it waits, and its time.
pub(in crate::tui) fn work_row(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    (selected, base): (bool, Style),
    app: &App,
    bot: &str,
    row: &DetailRow<'_>,
) {
    let (DetailRow::Thread(item) | DetailRow::Waiting(_, item) | DetailRow::Recent(item)) = row else { return };
    let t = theme();
    let (glyph, glyph_style, line, line_style) = mark_and_line(app, bot, row, item);
    // A place in line past the ninth is two cells wide.
    let mut x = put(buf, inner.x, y, 3, &glyph, glyph_style.patch(base)) + 1;
    if let Some(source) = source_mark(app, item) {
        x = put(buf, x, y, 14, &source, t.dim.patch(base)) + 1;
    }
    let right = rput(buf, inner.right(), y, &time(item), t.dim.patch(base));
    let end = right.saturating_sub(1);
    let title = if selected { t.bold } else { t.text };
    let x = put(
        buf,
        x,
        y,
        end.saturating_sub(x),
        &truncate(&item.title, usize::from(end.saturating_sub(x))),
        title.patch(base),
    );
    let room = usize::from(end.saturating_sub(x + 3));
    if room > 3 {
        put(buf, x, y, 3, " · ", t.dim.patch(base));
        put(buf, x + 3, y, u(room), &truncate(line.lines().next().unwrap_or_default(), room), line_style.patch(base));
    }
}

/// A task's transcript: its item's title, how it stands, then its entries in the trace's rows. Read-only.
pub(super) fn task_details(buf: &mut Buffer, area: Rect, app: &App, s: &TaskSheet) {
    let t = theme();
    let r = centered(area, 100, area.height.saturating_sub(4));
    let inner = frame_box(buf, r, t.text, t.panel, None);
    if inner.height < 6 {
        return;
    }
    let item = s.item(app);
    let live = item.is_some_and(|item| item.status.live());
    if let Some(item) = item {
        put(
            buf,
            inner.x,
            inner.y,
            inner.width,
            &truncate(&item.title, usize::from(inner.width)),
            t.bold.patch(t.panel),
        );
        let (glyph, style) = mark(item.status == WorkStatus::NeedsYou);
        let style = if live { style } else { t.dim };
        let x = put(buf, inner.x, inner.y + 1, 1, if live { glyph } else { "·" }, style.patch(t.panel)) + 1;
        // The branch of its worktree, while it works in one.
        let standing = item.branch.iter().fold(standing(item), |standing, branch| format!("{standing} · ⎇ {branch}"));
        put(buf, x, inner.y + 1, inner.right().saturating_sub(x), &standing, t.secondary.patch(t.panel));
    } else {
        put(buf, inner.x, inner.y, inner.width, &tr("In the background"), t.bold.patch(t.panel));
    }
    hline(buf, inner.x, inner.y + 2, inner.width, t.dim.patch(t.panel));
    let stop = live.then(|| tr("X stops"));
    let keys: Vec<_> = stop.into_iter().chain([tr("T full"), tr("↑↓ scroll · esc close")]).collect();
    put(buf, inner.x, inner.bottom() - 1, inner.width, &format!(" {}", keys.join(" · ")), t.dim.patch(t.panel));

    let body = Rect::new(inner.x, inner.y + 3, inner.width, inner.height - 4);
    let lines = detail_lines(app, s, usize::from(body.width));
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    s.max_scroll.set(max);
    s.page.set(h);
    let start = lines.len().saturating_sub(h + s.scroll.min(max));
    for (i, l) in lines.iter().skip(start).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
}

/// The transcript's lines, oldest first; with none, that it loads or has nothing yet. A load that failed says so
/// first, with the key that tries again.
fn detail_lines(app: &App, s: &TaskSheet, width: usize) -> Vec<Line<'static>> {
    let t = theme();
    let mut out: Vec<Line<'static>> = vec![];
    if let Fetched::Failed(error) = &s.fetched {
        out.push(Line::from(Span::styled(format!(" {error} · {}", tr("r retries")), t.red)));
    }
    for e in app.task_entries.of(&s.task) {
        if e.kind == Kind::User && e.data["from"] == "host" {
            told(&mut out, e, width, s.full);
        } else {
            push_step(&mut out, app, e, width, s.full);
        }
    }
    if out.is_empty() {
        let note = if matches!(s.fetched, Fetched::Loading) { tr("Loading…") } else { tr("Nothing yet.") };
        out.push(Line::from(Span::styled(format!(" {note}"), t.dim)));
    }
    out
}

/// What the task was told (its item, a later one that follows it): quoted text, not a message of the user's.
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

/// The question a first-in-line item asks when every place is taken: one row per thread that may give way (the
/// bot's own work), the picked one does.
pub(super) fn cut_in(buf: &mut Buffer, area: Rect, app: &App, c: &CutIn) {
    let t = theme();
    let work = app.box_of(&c.bot).0;
    let threads = giving_way(work);
    let r = centered(area, 76, u(threads.len()) + 8);
    let cutting = work.iter().find(|i| i.id == c.item).map_or("", |i| i.title.as_str());
    let title = fill(&tr("Let \"{title}\" cut in"), &[("title", &truncate(cutting, 40))]);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    if inner.height < 5 {
        return;
    }
    let why = tr("Every place is taken. Pick what gives way: its session closes and it starts again later.");
    let why =
        md::wrap(&[Span::styled(why.into_owned(), t.secondary.patch(t.panel))], usize::from(inner.width), &[], &[]);
    let mut y = inner.y;
    for l in why.iter().take(2) {
        put_line(buf, inner.x, y, inner.width, l);
        y += 1;
    }
    y += 1;
    let cursor = c.cursor.min(threads.len().saturating_sub(1));
    for (i, item) in threads.into_iter().enumerate().take(usize::from(inner.bottom().saturating_sub(y + 2))) {
        let selected = i == cursor;
        if selected {
            restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
        }
        let base = if selected { t.sel } else { t.panel };
        work_row(buf, inner, y, (selected, base), app, &c.bot, &DetailRow::Thread(item));
        y += 1;
    }
    let keys = format!(" ↵ {} · esc {}", tr("Cut in"), tr("Cancel"));
    put(buf, inner.x, inner.bottom() - 1, inner.width, &keys, t.dim.patch(t.panel));
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

    #[test]
    fn the_band_says_what_runs_and_cuts_the_middle_to_fit() {
        assert_eq!(band_text("Reading files", "1m05s", 60), "In the background · Reading files · 1m05s");
        assert_eq!(band_text("Needs you · 2 working", "", 60), "In the background · Needs you · 2 working");
        assert_eq!(band_text("Reading the whole repository", "12s", 36), "In the background · Reading t… · 12s");
        assert_eq!(band_keys(false, true), "B opens · X stops");
        assert_eq!(band_keys(false, false), "B opens", "several items, or none a task has: the page stops them");
        assert_eq!(band_keys(true, true), "esc · B opens · X stops", "typing: the keys wait for esc");
    }

    #[tokio::test]
    async fn the_band_sits_over_the_composer_under_the_queued_messages() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "needsYou", "activity": "Editing"},
            {"id": "j", "title": "Docs", "status": "waiting"}]),
        );
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": {"id": "q", "botId": "a", "seq": 1, "kind": "user",
            "turn": 1, "data": {"text": "and the tests", "status": "queued"}}})));
        app.host_version = "2.39.0".into();
        let bot = app.bots["a"].clone();
        let area = Rect::new(0, 0, 70, 4);
        let mut top = 0;
        let rows = screen(area, |buf| {
            let band = work_band(buf, area, &bot, 4, false);
            top = super::super::queue::queue_band(buf, area, &app, &bot, band, false);
        });
        assert_eq!(top, 2, "one row each");
        assert!(rows[2].starts_with(" › and the tests"), "{rows:?}");
        assert!(rows[3].starts_with(" ◆ In the background · Needs you · 1 waiting"), "{rows:?}");
        assert!(rows[3].ends_with("B opens · X stops "), "{rows:?}");

        // Nothing live or waiting, and never in a group: no row.
        let idle =
            Bot::parse(&json!({"id": "b", "name": "B", "work": [{"id": "i", "status": "done"}]})).expect("a bot");
        let mut group = bot.clone();
        group.group = true;
        screen(area, |buf| {
            assert_eq!(work_band(buf, area, &idle, 4, false), 4);
            assert_eq!(work_band(buf, area, &group, 4, false), 4);
        });
    }

    #[tokio::test]
    async fn a_row_shows_its_mark_title_line_and_time() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([
                {"id": "n", "title": "Fix login", "status": "needsYou", "task": "task-n"},
                {"id": "w", "title": "Write docs", "status": "working", "activity": "Reading files"},
                {"id": "q", "title": "Refactor", "status": "waiting", "wait": {"kind": "behind", "ahead": 1}},
                {"id": "d", "title": "Old", "status": "done", "startedAt": 1000, "endedAt": 66_000, "result": "All good."},
                {"id": "f", "title": "Broken", "status": "failed", "result": "it could not start: no agent\nmore"},
            ]),
        );
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": {"id": "card", "botId": "a", "seq": 1,
            "kind": "permission", "turn": 1, "data": {"status": "pending", "title": "Run tests", "options": [],
            "task": {"id": "task-n", "title": "Fix login"}, "item": "n"}}})));
        let work = app.bots["a"].work.clone();
        let area = Rect::new(0, 0, 60, 5);
        let rows = [
            DetailRow::Thread(&work[0]),
            DetailRow::Thread(&work[1]),
            DetailRow::Waiting(2, &work[2]),
            DetailRow::Recent(&work[3]),
            DetailRow::Recent(&work[4]),
        ];
        let lines = screen(area, |buf| {
            for (y, row) in rows.iter().enumerate() {
                work_row(buf, area, u(y), (false, Style::new()), &app, "a", row);
            }
        });
        let lines: Vec<&str> = lines.iter().map(|l| l.trim_end()).collect();
        assert_eq!(lines[0], "◆ Fix login · Run tests", "what its card asks");
        assert_eq!(lines[1], "● Write docs · Reading files");
        assert_eq!(lines[2], "2 Refactor · 1 ahead");
        assert!(lines[3].starts_with("○ Old · Done") && lines[3].ends_with("1m05s"), "{lines:?}");
        assert_eq!(lines[4], "! Broken · it could not start: no agent", "why, on one line");
    }

    #[tokio::test]
    async fn a_row_says_where_its_item_came_from_and_what_a_choice_card_asks() {
        let mut app = test_app();
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "mika", "name": "Mika"}})));
        bot(
            &mut app,
            &json!([
                {"id": "r", "title": "Morning summary", "status": "working", "source": {"kind": "routine", "id": "r1"}},
                {"id": "m", "title": "Review the PR", "status": "needsYou", "source": {"kind": "request", "id": "mika"}},
                {"id": "e", "title": "Elsewhere", "status": "waiting", "source": {"kind": "request"}},
                {"id": "g", "title": "Gone", "status": "waiting", "source": {"kind": "request", "id": "deleted"}},
                {"id": "o", "title": "Own", "status": "working", "source": {"kind": "later"}},
                {"id": "s", "title": "Login", "status": "needsYou"},
            ]),
        );
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": {"id": "sign-in", "botId": "a", "seq": 2,
            "kind": "notice", "turn": 1, "data": {"text": "Needs you to sign in.", "item": "s",
            "connectionRequest": {"kind": "agent", "status": "pending", "title": "Kimi Code", "botId": "a"}}}})));
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": {"id": "card", "botId": "a", "seq": 1,
            "kind": "notice", "turn": 1, "data": {"decision": true, "status": "pending", "question": "Squash or merge?",
            "choices": ["Squash", "Merge"], "task": {"id": "task-m", "title": "Review the PR"}, "item": "m"}}})));
        let work = app.bots["a"].work.clone();
        let rows = [
            DetailRow::Thread(&work[0]),
            DetailRow::Thread(&work[1]),
            DetailRow::Waiting(1, &work[2]),
            DetailRow::Waiting(2, &work[3]),
            DetailRow::Thread(&work[4]),
            DetailRow::Thread(&work[5]),
        ];
        let area = Rect::new(0, 0, 60, 6);
        let lines = screen(area, |buf| {
            for (y, row) in rows.iter().enumerate() {
                work_row(buf, area, u(y), (false, Style::new()), &app, "a", row);
            }
        });
        let lines: Vec<&str> = lines.iter().map(|l| l.trim_end()).collect();
        assert_eq!(lines[0], "● ↻ Morning summary · Working", "a routine's run");
        assert_eq!(lines[1], "◆ Mika › Review the PR · Squash or merge?", "the asking bot, and the choice's question");
        assert_eq!(lines[2], "1 Elsewhere · Waiting", "a bot on another computer: nothing");
        assert_eq!(lines[3], "2 Gone · Waiting", "a bot no longer here: nothing");
        assert_eq!(lines[4], "● Own · Working", "a source this terminal doesn't know reads as the bot's own");
        assert_eq!(lines[5], "◆ Login · Needs Kimi Code sign-in", "the agent its sign-in card in the chat waits on");
    }

    #[tokio::test]
    async fn the_transcript_quotes_what_the_task_was_told_and_shows_its_steps() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "working", "task": "task-1"},
            {"id": "x", "title": "Old", "status": "done", "task": "x", "startedAt": 1000, "endedAt": 66_000}]),
        );
        let sheet = TaskSheet::new("a".into(), "i".into(), "task-1".into());
        assert_eq!(text(&detail_lines(&app, &sheet, 60)), [" Loading…"]);
        let entry = |seq: i64, kind: &str, data: serde_json::Value| {
            json!({"type": "taskEntry", "rev": seq, "task": "task-1", "entry": {"id": format!("t{seq}"),
                "botId": "task-1", "seq": seq, "rev": seq, "kind": kind, "turn": 1, "data": data}})
        };
        app.on_msg(Msg::Event(entry(1, "user", json!({"from": "host", "text": "Fix the login redirect."}))));
        app.on_msg(Msg::Event(entry(2, "thought", json!({"text": "Where is the redirect?"}))));
        let lines = text(&detail_lines(&app, &sheet, 60));
        assert_eq!(lines[0], " ▏ Fix the login redirect.", "plain quoted text, not the user's message");
        assert!(lines[1].contains("thought") && lines[1].contains("Where is the redirect?"), "{lines:?}");

        let over = TaskSheet::new("a".into(), "x".into(), "x".into());
        let failed = TaskSheet { fetched: Fetched::Failed("offline".into()), ..over };
        assert_eq!(text(&detail_lines(&app, &failed, 60)), [" offline · r retries"]);
        assert_eq!(failed.item(&app).map(standing).as_deref(), Some("Done · 1m05s"));
        let empty = TaskSheet { fetched: Fetched::Loaded, ..failed };
        assert_eq!(text(&detail_lines(&app, &empty, 60)), [" Nothing yet."]);
    }

    #[tokio::test]
    async fn cutting_in_asks_which_thread_gives_way() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([
                {"id": "w", "title": "Write docs", "status": "working", "activity": "Reading files"},
                {"id": "n", "title": "Fix login", "status": "needsYou"},
                {"id": "q", "title": "Refactor", "status": "waiting"},
            ]),
        );
        let area = Rect::new(0, 0, 90, 20);
        let ask = CutIn { bot: "a".into(), item: "q".into(), cursor: 0 };
        let lines = screen(area, |buf| cut_in(buf, area, &app, &ask));
        let at = |text: &str| lines.iter().position(|line| line.contains(text));
        assert!(at("Let \"Refactor\" cut in").is_some(), "{lines:?}");
        assert!(at("Every place is taken. Pick what gives way:").is_some(), "{lines:?}");
        let first = at("◆ Fix login · Needs you").expect("what needs you first");
        assert_eq!(at("● Write docs · Reading files"), Some(first + 1));
        assert!(at("↵ Cut in · esc Cancel").is_some());

        // A routine's run and another bot's request are stopped, never set aside: the dialog doesn't list them.
        bot(
            &mut app,
            &json!([
                {"id": "w", "title": "Write docs", "status": "working"},
                {"id": "r", "title": "Morning summary", "status": "working", "source": {"kind": "routine", "id": "r1"}},
                {"id": "m", "title": "Review the PR", "status": "working", "source": {"kind": "request"}},
                {"id": "q", "title": "Refactor", "status": "waiting"},
            ]),
        );
        let lines = screen(area, |buf| cut_in(buf, area, &app, &ask));
        let listed = |text: &str| lines.iter().any(|line| line.contains(text));
        assert!(listed("● Write docs") && !listed("Morning summary") && !listed("Review the PR"), "{lines:?}");
    }

    #[tokio::test]
    async fn the_transcript_heading_names_the_branch_of_its_worktree() {
        let mut app = test_app();
        bot(
            &mut app,
            &json!([{"id": "i", "title": "Fix login", "status": "working", "task": "task-1",
                "worktree": {"branch": "codync/fix-login", "path": "/w/a/i"}},
            {"id": "j", "title": "Docs", "status": "working", "task": "task-2"}]),
        );
        let area = Rect::new(0, 0, 100, 20);
        let heading = |item: &str| {
            let sheet = TaskSheet::new("a".into(), item.into(), "task".into());
            let lines = screen(area, |buf| task_details(buf, area, &app, &sheet));
            let standing = lines.iter().find(|line| line.contains("Working")).expect("how it stands");
            standing.trim().trim_matches('│').trim().to_owned()
        };
        assert_eq!(heading("i"), "◷ Working · ⎇ codync/fix-login");
        assert_eq!(heading("j"), "◷ Working", "the first task alone keeps the bot's folder");
    }
}
