//! A bot's Details page: what it does. Threads (the tasks its threads work on now), the event box (what waits for
//! one), what it does later, its routines, and what ended in the last day (`docs/plans/event-box.md`).

use serde_json::Value;

use super::{App, Buffer, Rect, centered, footer, frame_box, name_of, page_tabs, put, row, rput, s, theme, u};
use crate::tui::app::can_cut_in;
use crate::tui::l10n::{fill, tr};
use crate::tui::manage::{DetailRow, DetailSection, DetailsPage, Page, WorkAct};
use crate::tui::md::truncate;
use crate::tui::view::{when_future, work_row};

/// What the page draws, top to bottom, and the rows it takes.
enum Part<'a> {
    Gap,
    Heading(&'a DetailSection<'a>),
    /// A row the cursor moves over, by its place among them.
    Row(usize, &'a DetailRow<'a>),
    /// A place of the bot not in use.
    Free,
    Note(&'static str),
}

impl Part<'_> {
    fn height(&self) -> usize {
        match self {
            Self::Row(_, DetailRow::FollowUp(_) | DetailRow::Routine(_)) => 2,
            _ => 1,
        }
    }
}

/// The page's parts: each section under its heading, a gap between two.
fn parts<'a>(l: &DetailsPage, sections: &'a [DetailSection<'a>]) -> Vec<Part<'a>> {
    let mut out = vec![];
    let mut at = 0;
    for section in sections {
        if !out.is_empty() {
            out.push(Part::Gap);
        }
        out.push(Part::Heading(section));
        for row in &section.rows {
            out.push(Part::Row(at, row));
            at += 1;
        }
        out.extend((0..section.free).map(|_| Part::Free));
        if section.title == "Routines" && section.rows.is_empty() {
            let none = "No routines. n sets one up here, or ask the bot in chat.";
            out.push(Part::Note(if l.routines.is_some() { none } else { "Loading…" }));
        }
    }
    out
}

/// The keys of the picked row, for the page's last line.
fn keys(picked: Option<&DetailRow<'_>>, full: bool) -> String {
    match picked {
        Some(DetailRow::FollowUp(_)) => "x cancel follow-up · esc".into(),
        Some(DetailRow::Routine(_)) => {
            "n new · ↵ edit · space on/off · r run now · w copy webhook · W new key · x delete · esc".into()
        }
        Some(row) => WorkAct::keys(row, full).into_iter().chain(["esc".into()]).collect::<Vec<_>>().join(" · "),
        None => "n new · esc".into(),
    }
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, l: &DetailsPage) {
    let t = theme();
    let r = centered(area, 90, 34);
    let title = name_of(app, &l.bot);
    let inner = frame_box(buf, r, t.text, t.panel, Some((&title, t.text)));
    let inner = page_tabs(buf, inner, Page::Details);
    let (work, max_threads) = app.box_of(&l.bot);
    let sections = l.sections(work, max_threads);
    let parts = parts(l, &sections);
    // The page can outgrow the sheet: it starts as far down as keeps the picked row in sight.
    let room = usize::from(inner.height.saturating_sub(2));
    let mut end = 0;
    for part in &parts {
        end += part.height();
        if matches!(part, Part::Row(at, _) if *at == l.cursor) {
            break;
        }
    }
    let (mut y, bottom) = (0, end.max(room));
    for part in &parts {
        let top = y;
        y += part.height();
        if top < bottom - room || y > bottom {
            continue;
        }
        let at = inner.y + u(top - (bottom - room));
        match part {
            Part::Gap => {}
            Part::Heading(section) => {
                let x = put(buf, inner.x, at, inner.width, &tr(section.title), t.bold.patch(t.panel));
                put(buf, x + 2, at, inner.right().saturating_sub(x + 2), &section.count, t.dim.patch(t.panel));
            }
            Part::Free => {
                put(buf, inner.x, at, inner.width, &format!("· {}", tr("Free")), t.dim.patch(t.panel));
            }
            Part::Note(note) => {
                put(buf, inner.x, at, inner.width, &tr(note), t.secondary.patch(t.panel));
            }
            Part::Row(i, DetailRow::FollowUp(follow_up)) => follow_up_row(buf, inner, at, *i == l.cursor, follow_up),
            Part::Row(i, DetailRow::Routine(it)) => routine_row(buf, inner, at, *i == l.cursor, it, &l.runs),
            Part::Row(i, item) => {
                let selected = *i == l.cursor;
                let base = row(buf, inner, at, selected);
                work_row(buf, inner, at, (selected, base), app, &l.bot, item);
            }
        }
    }
    let picked = parts.iter().find_map(|part| match part {
        Part::Row(at, row) if *at == l.cursor => Some(*row),
        _ => None,
    });
    footer(buf, inner, None, &keys(picked, can_cut_in(work, max_threads)));
}

/// A follow-up the bot set for itself: a clock where a routine has its on/off dot, and when it happens.
fn follow_up_row(buf: &mut Buffer, inner: Rect, y: u16, selected: bool, follow_up: &Value) {
    let t = theme();
    let base = row(buf, inner, y, selected);
    put(buf, inner.x, y, 2, "◷", t.dim.patch(base));
    let room = usize::from(inner.width.saturating_sub(2));
    let note = s(follow_up, "note").lines().next().unwrap_or_default();
    put(buf, inner.x + 2, y, u(room), &truncate(note, room), t.bold.patch(base));
    let at = follow_up["at"].as_i64().unwrap_or_default();
    let when = if at <= crate::store::now_ms() {
        tr("Follow-up due now").into_owned()
    } else {
        fill(&tr("Follow-up at {time}"), &[("time", &when_future(at))])
    };
    put(buf, inner.x + 2, y + 1, u(room), &truncate(&when, room), t.secondary.patch(t.panel));
}

fn routine_row(buf: &mut Buffer, inner: Rect, y: u16, selected: bool, it: &Value, runs: &[Value]) {
    let t = theme();
    let base = row(buf, inner, y, selected);
    let on = it["enabled"] == true;
    put(buf, inner.x, y, 2, if on { "●" } else { "○" }, if on { t.bold } else { t.dim }.patch(base));
    put(buf, inner.x + 2, y, inner.width / 2, s(it, "name"), t.bold.patch(base));
    // A retry still waiting comes before the schedule's next run.
    let retry = runs
        .iter()
        .find(|r| r["routineId"] == it["id"] && r["status"] == "pending")
        .and_then(|r| r["notBefore"].as_i64());
    let next = retry
        .map(|ms| format!("retry {}", when_future(ms)))
        .or_else(|| it["nextRunAt"].as_i64().map(|ms| format!("next {}", when_future(ms))));
    rput(
        buf,
        inner.right(),
        y,
        &next.unwrap_or_else(|| if on { String::new() } else { "off".into() }),
        t.dim.patch(base),
    );
    let when: Vec<String> = it["triggerDescriptions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|text| tr(text).into_owned())
        .collect();
    let (line, style) = match it["lastError"].as_str() {
        Some(e) => (format!("{} · {e}", when.join(" · ")), t.red),
        None => (when.join(" · "), t.secondary),
    };
    let room = usize::from(inner.width.saturating_sub(2));
    put(buf, inner.x + 2, y + 1, u(room), &truncate(&line, room), style.patch(t.panel));
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tui::app::{Msg, screen, test_app};

    fn page(app: &App, list: &DetailsPage) -> Vec<String> {
        let area = Rect::new(0, 0, 100, 36);
        screen(area, |buf| draw(buf, area, app, list))
    }

    #[tokio::test]
    async fn the_page_lists_follow_ups_above_the_routines_with_a_clock_and_when() {
        let app = test_app();
        let routine = json!({"id": "r", "name": "Morning summary", "enabled": true,
            "triggerDescriptions": ["Every day at 09:00"]});
        let list = DetailsPage {
            bot: "b".into(),
            routines: Some(vec![routine]),
            follow_ups: vec![json!({"id": "f", "at": 0, "note": "Check the build\nthen the deploy"})],
            ..Default::default()
        };
        let lines = page(&app, &list);
        let at = |text: &str| lines.iter().position(|line| line.contains(text));
        let note = at("◷ Check the build").expect("the note's first line, after its clock");
        assert_eq!(at("Later"), Some(note - 1), "under its heading");
        assert!(!lines[note].contains("then the deploy"));
        assert!(lines[note + 1].contains("Follow-up due now"));
        assert!(at("● Morning summary") > Some(note), "the routines come after");
        assert!(at("x cancel follow-up").is_some(), "the keys of what is picked");
        // An idle bot's page has no threads, no box, and no agent or instructions: those are in Settings.
        for gone in ["Threads", "Event box", "Free", "Recent", "i edits", "default model"] {
            assert_eq!(at(gone), None, "{gone}");
        }
    }

    #[tokio::test]
    async fn the_page_is_the_timeline_of_what_the_bot_does() {
        let mut app = test_app();
        let work = json!([
            {"id": "w", "title": "Write docs", "status": "working", "activity": "Reading files", "thread": "thread-w"},
            {"id": "n", "title": "Fix login", "status": "needsYou", "thread": "thread-n"},
            {"id": "q1", "title": "Refactor", "status": "waiting", "wait": {"kind": "next"}},
            {"id": "q2", "title": "Cleanup", "status": "waiting", "wait": {"kind": "behind", "ahead": 1}},
            {"id": "q3", "title": "Notes", "status": "waiting", "wait": {"kind": "after"}},
            {"id": "d", "title": "Old", "status": "done", "thread": "thread-d"},
            {"id": "f", "title": "Broken", "status": "failed", "result": "it could not start"},
        ]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "b", "name": "B", "work": work, "maxThreads": 3}})));
        let mut list = DetailsPage { bot: "b".into(), routines: Some(vec![]), ..Default::default() };
        let lines = page(&app, &list);
        let at = |text: &str| lines.iter().position(|line| line.contains(text)).unwrap_or_else(|| panic!("{text}"));
        let threads = at("Threads  2 / 3");
        assert_eq!(at("◆ Fix login · Needs you"), threads + 1, "what needs you first");
        assert_eq!(at("● Write docs · Reading files"), threads + 2);
        assert_eq!(at("· Free"), threads + 3, "one faint row for the place not in use");
        let waiting = at("Event box  3");
        assert_eq!(waiting, threads + 5);
        assert_eq!(at("1 Refactor · Next up"), waiting + 1);
        assert_eq!(at("2 Cleanup · 1 ahead"), waiting + 2);
        assert_eq!(at("3 Notes · Follows what it works on"), waiting + 3);
        let routines = at("Routines");
        assert_eq!(at("No routines."), routines + 1);
        let recent = at("Recent");
        assert!(recent > routines);
        assert_eq!(at("○ Old · Done"), recent + 1);
        assert_eq!(at("! Broken · it could not start"), recent + 2);
        assert!(lines.iter().any(|line| line.contains("↵ opens · X stops · esc")), "a thread's keys");

        // The picked row's keys: the first in line cuts in only while every place is taken.
        list.cursor = 2;
        assert!(page(&app, &list).iter().any(|line| line.contains("D Drop · esc") && !line.contains("Cut in")));
        list.cursor = 3;
        assert!(page(&app, &list).iter().any(|line| line.contains("F First · D Drop · esc")));
        list.cursor = 6;
        assert!(page(&app, &list).iter().any(|line| line.contains("D Drop · A Try again · esc")));
        let mut full = app.bots["b"].clone();
        full.max_threads = Some(2);
        app.bots.insert("b".into(), full);
        list.cursor = 2;
        let lines = page(&app, &list);
        assert!(lines.iter().any(|line| line.contains("C Cut in · D Drop · esc")), "{lines:?}");
        assert!(
            lines.iter().any(|line| line.contains("Threads  2 / 2")) && !lines.iter().any(|line| line.contains("Free"))
        );
    }

    #[tokio::test]
    async fn a_long_page_scrolls_to_keep_the_picked_row_in_sight() {
        let mut app = test_app();
        let work: Vec<Value> = (0..40)
            .map(|i| json!({"id": format!("q{i}"), "title": format!("Item {i}"), "status": "waiting"}))
            .collect();
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "b", "name": "B", "work": work}})));
        let mut list = DetailsPage { bot: "b".into(), routines: Some(vec![]), ..Default::default() };
        assert!(page(&app, &list).iter().any(|line| line.contains("Threads  0 / 3")), "from the top");
        list.cursor = 39;
        let lines = page(&app, &list);
        assert!(lines.iter().any(|line| line.contains("40 Item 39")), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains("Threads")), "the top scrolled away");
    }
}
