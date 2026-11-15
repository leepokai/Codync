//! A bot's background tasks: the line over the composer, the list of several, and a task's read-only details.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::super::app::{App, Background, Bot, Entry, Kind, Task, TaskStatus};
use super::super::l10n::tr;
use super::super::manage::{Fetched, TaskList, TaskSheet};
use super::super::md::{self, truncate, width as w};
use super::buffer::{centered, frame_box, hline, put, put_line, restyle, rput, u};
use super::style::theme;
use super::time::{elapsed, span};
use super::trace::push_step;

/// What a task was told shows this many lines until `T` unfolds it.
const TOLD_LINES: usize = 6;

/// Draws what `bot` does in the background on the line over `bottom` (the composer); where that line starts, or
/// `bottom` when it does nothing there.
pub(super) fn task_band(buf: &mut Buffer, r: Rect, bot: &Bot, bottom: u16, typing: bool) -> u16 {
    let Some(background) = Background::of(&bot.tasks).filter(|_| !bot.group) else { return bottom };
    let t = theme();
    let y = bottom.saturating_sub(1);
    let hint_x = rput(buf, r.right() - 1, y, &band_keys(typing), t.dim);
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

/// The band's keys; typing, they wait for esc.
fn band_keys(typing: bool) -> String {
    let keys = [tr("B opens"), tr("X stops")];
    let esc = typing.then_some("esc".into());
    esc.into_iter().chain(keys).collect::<Vec<_>>().join(" · ")
}

/// `In the background · <what> · <time>` in `room` cells: the middle part gives way.
fn band_text(what: &str, time: &str, room: usize) -> String {
    let head = format!("{} · ", tr("In the background"));
    let tail = if time.is_empty() { String::new() } else { format!(" · {time}") };
    format!("{head}{}{tail}", truncate(what, room.saturating_sub(w(&head) + w(&tail))))
}

/// A task's status in words and how long it has run, or ran.
fn standing(task: &Task) -> String {
    let time = match (task.started_at, task.ended_at) {
        (Some(start), Some(end)) => span(start, end),
        (start, _) if task.status.live() => elapsed(start),
        _ => String::new(),
    };
    let status = tr(task.status.word());
    if time.is_empty() { status.into_owned() } else { format!("{status} · {time}") }
}

/// The tasks of a bot that has several: title, status and time; a row opens its details.
pub(super) fn task_list(buf: &mut Buffer, area: Rect, app: &App, l: &TaskList) {
    let t = theme();
    let tasks = app.bots.get(&l.bot).map_or(&[][..], |b| b.tasks.as_slice());
    let r = centered(area, 72, u(tasks.len().max(1)) + 5);
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("In the background").as_ref(), t.text)));
    if inner.height < 3 {
        return;
    }
    let keys = format!(" {} · {} · {}", tr("↵ opens"), tr("X stops"), tr("esc close"));
    put(buf, inner.x, inner.bottom() - 1, inner.width, &keys, t.dim.patch(t.panel));
    if tasks.is_empty() {
        put(buf, inner.x, inner.y + 1, inner.width, &tr("Nothing in the background"), t.dim.patch(t.panel));
    }
    let cursor = l.cursor.min(tasks.len().saturating_sub(1));
    for (i, task) in tasks.iter().enumerate().take(usize::from(inner.height.saturating_sub(3))) {
        let y = inner.y + 1 + u(i);
        let bg = if i == cursor { t.sel } else { t.panel };
        if i == cursor {
            restyle(buf, Rect::new(inner.x - 1, y, inner.width + 2, 1), t.sel);
        }
        let (glyph, style) = mark(task.status == TaskStatus::NeedsInput);
        put(buf, inner.x, y, 1, glyph, style.patch(bg));
        let right = rput(buf, inner.right(), y, &standing(task), t.dim.patch(bg));
        let room = right.saturating_sub(inner.x + 3);
        let title = if i == cursor { t.bold } else { t.text };
        put(buf, inner.x + 2, y, room, &truncate(&task.title, usize::from(room)), title.patch(bg));
    }
}

/// A task's details: its title, how it stands, then its own transcript in the trace's rows. Read-only.
pub(super) fn task_details(buf: &mut Buffer, area: Rect, app: &App, s: &TaskSheet) {
    let t = theme();
    let r = centered(area, 100, area.height.saturating_sub(4));
    let inner = frame_box(buf, r, t.text, t.panel, None);
    if inner.height < 6 {
        return;
    }
    let task = s.task(app);
    let live = task.is_some_and(|task| task.status.live());
    if let Some(task) = task {
        put(
            buf,
            inner.x,
            inner.y,
            inner.width,
            &truncate(&task.title, usize::from(inner.width)),
            t.bold.patch(t.panel),
        );
        let (glyph, style) = mark(task.status == TaskStatus::NeedsInput);
        let style = if live { style } else { t.dim };
        let x = put(buf, inner.x, inner.y + 1, 1, if live { glyph } else { "·" }, style.patch(t.panel)) + 1;
        put(buf, x, inner.y + 1, inner.right().saturating_sub(x), &standing(task), t.secondary.patch(t.panel));
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

/// What the task was told (its brief, a later message from its bot): quoted text, not a message of the user's.
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

    #[test]
    fn the_band_says_what_runs_and_cuts_the_middle_to_fit() {
        assert_eq!(band_text("Reading files", "1m05s", 60), "In the background · Reading files · 1m05s");
        assert_eq!(band_text("Needs you", "", 60), "In the background · Needs you");
        assert_eq!(band_text("Reading the whole repository", "12s", 36), "In the background · Reading t… · 12s");
        assert_eq!(band_keys(false), "B opens · X stops");
        assert_eq!(band_keys(true), "esc · B opens · X stops", "typing: the keys wait for esc");
    }

    #[tokio::test]
    async fn the_band_sits_over_the_composer_under_the_queued_messages() {
        let mut app = test_app();
        let tasks = json!([{"id": "task-1", "title": "Fix login", "status": "needsInput", "activity": "Editing"}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "tasks": tasks}})));
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": {"id": "q", "botId": "a", "seq": 1, "kind": "user",
            "turn": 1, "data": {"text": "and the tests", "status": "queued"}}})));
        app.host_version = "2.39.0".into();
        let bot = app.bots["a"].clone();
        let area = Rect::new(0, 0, 70, 4);
        let mut top = 0;
        let rows = screen(area, |buf| {
            let band = task_band(buf, area, &bot, 4, false);
            top = super::super::queue::queue_band(buf, area, &app, &bot, band, false);
        });
        assert_eq!(top, 2, "one row each");
        assert!(rows[2].starts_with(" › and the tests"), "{rows:?}");
        assert!(rows[3].starts_with(" ◆ In the background · Needs you"), "{rows:?}");
        assert!(rows[3].ends_with("B opens · X stops "), "{rows:?}");

        // Nothing in the background, and never in a group: no row.
        let idle = Bot::parse(&json!({"id": "b", "name": "B"})).expect("a bot");
        let mut group = bot.clone();
        group.group = true;
        screen(area, |buf| {
            assert_eq!(task_band(buf, area, &idle, 4, false), 4);
            assert_eq!(task_band(buf, area, &group, 4, false), 4);
        });
    }

    #[tokio::test]
    async fn the_details_quote_what_the_task_was_told_and_show_its_steps() {
        let mut app = test_app();
        let tasks = json!([{"id": "task-1", "title": "Fix login", "status": "working"}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "tasks": tasks}})));
        let sheet = TaskSheet::new("a".into(), "task-1".into());
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

        let failed = TaskSheet { fetched: Fetched::Failed("offline".into()), ..TaskSheet::new("a".into(), "x".into()) };
        assert_eq!(text(&detail_lines(&app, &failed, 60)), [" offline · r retries"]);
        let over = Task::parse(&json!({"id": "x", "status": "done", "startedAt": 1000, "endedAt": 66_000}));
        assert_eq!(over.as_ref().map(standing).as_deref(), Some("Done · 1m05s"));
        let empty = TaskSheet { fetched: Fetched::Loaded(over.expect("a task")), ..failed };
        assert_eq!(text(&detail_lines(&app, &empty, 60)), [" Nothing yet."]);
    }
}
