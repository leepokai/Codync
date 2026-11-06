//! The trace pane: one turn's narration, thoughts, tools and plan.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::super::app::{App, Bot, Entry, Focus, Kind, Status, TraceMode, tilde};
use super::super::md::{self, truncate, width as w};
use super::buffer::{hline, put, put_line, rput, u};
use super::style::{spin, theme};
use super::transcript::{name_style, user_line};

pub(super) fn trace(buf: &mut Buffer, r: Rect, app: &mut App) {
    let t = theme();
    app.hits.trace = r;
    let full = app.trace == TraceMode::Full;
    let Some(b) = app.bot().cloned() else { return };
    let turns = app.lane_turns(&b.id);
    let latest = turns.last().copied().unwrap_or(0);
    let turn = app.trace_turn.unwrap_or(latest);
    let live = turn == latest && b.status == Status::Working && b.works_here();
    let nth = turns.iter().position(|x| *x == turn).map_or(0, |i| i + 1);
    let x = put(
        buf,
        r.x + 1,
        r.y,
        6,
        "TRACE",
        if app.focus == Focus::Trace { t.bold } else { t.secondary.add_modifier(Modifier::BOLD) },
    );
    let keys = if full { "{ } turns · esc close" } else { "{ } turns · T full" };
    // The keys hint only when the title leaves it room.
    let right = if r.width >= 64 { rput(buf, r.right() - 1, r.y, keys, t.dim) } else { r.right() };
    let title = if turns.is_empty() {
        format!("· {} · chat", b.name)
    } else {
        format!("· {} · chat · turn {nth} of {}{}", b.name, turns.len(), if live { " · live" } else { "" })
    };
    let room = right.saturating_sub(x + 2);
    put(buf, x + 1, r.y, room, &truncate(&title, usize::from(room)), t.dim);
    hline(buf, r.x, r.y + 1, r.width, t.line);
    let body = Rect::new(r.x, r.y + 2, r.width, r.height.saturating_sub(2));
    let lines = build_trace(app, &b, turn, usize::from(body.width.saturating_sub(1)), full);
    let h = usize::from(body.height);
    let max = lines.len().saturating_sub(h);
    if live && app.focus != Focus::Trace {
        app.trace_scroll = max; // follow along while it works
    }
    app.trace_scroll = app.trace_scroll.min(max);
    for (i, l) in lines.iter().skip(app.trace_scroll).take(h).enumerate() {
        put_line(buf, body.x, body.y + u(i), body.width, l);
    }
    if lines.is_empty() {
        let empty = if turns.is_empty() { "Nothing yet." } else { "Nothing happened in this turn yet." };
        put(buf, body.x + 2, body.y, body.width.saturating_sub(2), empty, t.dim);
    }
}

fn tool_label(kind: &str) -> &str {
    match kind {
        "execute" => "bash",
        "other" => "tool",
        k => k,
    }
}

fn build_trace(app: &App, b: &Bot, turn: i64, width: usize, full: bool) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = vec![];
    let Some(map) = app.entries.get(&b.id) else { return out };
    let mut speaker: Option<&str> = None;
    for e in map.values().filter(|e| e.turn == turn) {
        // In a group, each member's steps under its name.
        if b.group && e.kind != Kind::User {
            let author = e.data["author"].as_str();
            if author.is_some() && author != speaker {
                speaker = author;
                let (name, color) = author.and_then(|a| app.bots.get(a)).map_or_else(
                    || ("A deleted bot".to_owned(), "gray".to_owned()),
                    |a| (a.name.clone(), a.color.clone()),
                );
                out.push(Line::from(Span::styled(format!(" {name}"), name_style(&color))));
            }
        }
        push_step(&mut out, app, e, width, full);
    }
    out
}

/// One entry's lines: a glyph, its kind and text. `full` shows a tool's whole diff and output,
/// otherwise their ends.
pub(super) fn push_step(out: &mut Vec<Line<'static>>, app: &App, e: &Entry, width: usize, full: bool) {
    let t = theme();
    match e.kind {
        Kind::User => out.push(Line::from(vec![
            Span::styled(" › ", t.dim),
            label("you"),
            Span::styled(truncate(&user_line(e), text_rest(width)), t.text),
        ])),
        Kind::Thought | Kind::Agent => push_text(out, e, width, full),
        Kind::Tool => push_tool(out, app, e, width, full),
        Kind::Plan => push_plan(out, e, width),
        Kind::Permission => {
            let st = e.data["status"].as_str().unwrap_or_default();
            out.push(Line::from(vec![
                Span::styled(" ◆ ", t.amber),
                label("asked"),
                Span::styled(
                    truncate(e.data["title"].as_str().unwrap_or_default(), text_rest(width).saturating_sub(12)),
                    t.text,
                ),
                Span::styled(format!("  {st}"), t.dim),
            ]));
        }
        Kind::Notice => out.push(Line::from(vec![
            Span::styled(" · ", t.dim),
            Span::styled(truncate(&e.notice_text(), width.saturating_sub(4)), t.dim),
        ])),
        Kind::Other => {}
    }
}

/// A step's kind, padded to the text column.
fn label(s: &str) -> Span<'static> {
    Span::styled(format!("{s:<8} "), theme().secondary)
}

/// The room a step's text has after its glyph and label.
fn text_rest(width: usize) -> usize {
    width.saturating_sub(13)
}

/// A thought or the bot's text: wrapped in full, else its first line.
fn push_text(out: &mut Vec<Line<'static>>, e: &Entry, width: usize, full: bool) {
    let t = theme();
    let (g, lab) = if e.kind == Kind::Thought {
        ("✱", "thought")
    } else if e.is_final() {
        ("●", "reply")
    } else {
        ("✱", "says")
    };
    let s = if e.kind == Kind::Thought { t.dim } else { t.text };
    if full {
        let first = [Span::styled(format!(" {g} "), t.dim), label(lab)];
        let rest = [Span::raw(" ".repeat(12))];
        out.extend(md::wrap(&[Span::styled(e.text().trim().to_owned(), s)], width, &first, &rest));
    } else {
        out.push(Line::from(vec![
            Span::styled(format!(" {g} "), t.dim),
            label(lab),
            Span::styled(truncate(e.text().trim(), text_rest(width)), s),
        ]));
    }
}

/// A tool call: its status, kind and title, then its diffs and output.
fn push_tool(out: &mut Vec<Line<'static>>, app: &App, e: &Entry, width: usize, full: bool) {
    let t = theme();
    let (g, gs) = match e.data["status"].as_str() {
        Some("completed") => ("✓".to_owned(), t.green),
        Some("failed") => ("×".to_owned(), t.red),
        _ => (spin(app).to_owned(), t.secondary),
    };
    let kind = tool_label(e.data["toolKind"].as_str().unwrap_or("tool")).to_owned();
    let title = e.data["title"].as_str().unwrap_or("Tool");
    out.push(Line::from(vec![
        Span::styled(format!(" {g} "), gs),
        label(&kind),
        Span::styled(truncate(title, text_rest(width)), t.text),
    ]));
    let (diff_max, out_max) = if full { (400, 400) } else { (14, 6) };
    for d in e.data["diffs"].as_array().into_iter().flatten() {
        push_diff(out, app, d, width, diff_max);
    }
    push_output(out, e.data["output"].as_str().unwrap_or_default(), width, out_max);
}

/// A file's diff: its path and counts, then its first `max` patch lines.
fn push_diff(out: &mut Vec<Line<'static>>, app: &App, d: &serde_json::Value, width: usize, max: usize) {
    let t = theme();
    let stat = format!("+{} −{}", d["added"].as_u64().unwrap_or(0), d["removed"].as_u64().unwrap_or(0));
    let path = tilde(d["path"].as_str().unwrap_or_default(), &app.home);
    out.push(Line::from(vec![
        Span::raw("   "),
        Span::styled(truncate(&path, width.saturating_sub(12)), t.bold),
        Span::styled(format!("  {stat}"), t.secondary),
    ]));
    let start = d["startLine"].as_u64().unwrap_or(1);
    out.push(Line::from(Span::styled(format!("   @@ line {start} @@"), t.dim)));
    let patch = d["patch"].as_str().unwrap_or_default();
    let n = patch.lines().count();
    for l in patch.lines().take(max) {
        let s = if l.starts_with('+') {
            t.added
        } else if l.starts_with('-') {
            t.removed
        } else {
            t.secondary
        };
        let body = truncate(l, width.saturating_sub(4));
        let pad = width.saturating_sub(4 + w(&body));
        out.push(Line::from(vec![Span::raw("   "), Span::styled(format!("{body}{}", " ".repeat(pad)), s)]));
    }
    if n > max {
        out.push(Line::from(Span::styled(format!("   … {} more lines · T shows everything", n - max), t.dim)));
    }
}

/// A tool's output: its last `max` lines.
fn push_output(out: &mut Vec<Line<'static>>, output: &str, width: usize, max: usize) {
    let t = theme();
    let lines: Vec<&str> = output.trim_end().lines().collect();
    let skip = lines.len().saturating_sub(max);
    if skip > 0 {
        out.push(Line::from(Span::styled(format!("           … {skip} earlier lines"), t.dim)));
    }
    for l in &lines[skip..] {
        out.push(Line::from(vec![
            Span::raw("           "),
            Span::styled(truncate(l, width.saturating_sub(12)), t.dim),
        ]));
    }
}

/// A plan: how many of its items are done, then each item.
fn push_plan(out: &mut Vec<Line<'static>>, e: &Entry, width: usize) {
    let t = theme();
    let items = e.data["entries"].as_array().cloned().unwrap_or_default();
    let done = items.iter().filter(|i| i["status"] == "completed").count();
    out.push(Line::from(vec![
        Span::styled(" ☐ ", t.secondary),
        label("plan"),
        Span::styled(format!("{done}/{}", items.len()), t.text),
    ]));
    for i in &items {
        let (g, s) = match i["status"].as_str() {
            Some("completed") => ("✓", t.green),
            Some("in_progress") => ("▸", t.bold),
            _ => ("☐", t.dim),
        };
        out.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(format!("{g} "), s),
            Span::styled(truncate(i["content"].as_str().unwrap_or_default(), width.saturating_sub(6)), t.secondary),
        ]));
    }
}
