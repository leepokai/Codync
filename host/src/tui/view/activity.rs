//! The current activity and how many steps the turn has taken: with thinking details on, `>` (or a
//! click) unfolds them under it; `t` opens Full conversation.

use ratatui::text::{Line, Span};

use super::super::app::{App, Bot, Click, Entry, running_turn, working_steps};
use super::super::l10n::tr;
use super::super::md::truncate;
use super::buffer::u;
use super::style::{spin, theme};
use super::trace::push_step;
use super::transcript::{Built, gap, name_style};

/// The most lines the unfolded steps take; earlier ones stay in Full conversation.
const STEP_LINES: usize = 14;

/// `details`: this terminal's thinking details choice (`super::super::details`).
pub(super) fn activity(out: &mut Built, app: &App, bot: &Bot, entries: &[&Entry], width: usize, details: bool) {
    let t = theme();
    gap(out);
    out.lines.push(Line::from(Span::styled(format!(" {}", bot.name), name_style(&bot.color))));
    let steps = working_steps(entries);
    let open = details
        && app.steps_open.as_ref().is_some_and(|(id, turn)| id == &bot.id && Some(*turn) == running_turn(entries));
    let text = if bot.activity.is_empty() { tr("Working…") } else { tr(&bot.activity) };
    let mut row = vec![
        Span::styled(format!(" {} ", spin(app)), t.secondary),
        Span::styled(truncate(text.as_ref(), width.saturating_sub(8)), t.secondary),
    ];
    let hint = if details {
        out.buttons.push((out.lines.len(), 0, u(width), Click::Steps));
        row.push(Span::styled(if open { "  v" } else { "  >" }, t.dim));
        format!("   {} steps so far · > {} · t full conversation", steps.len(), if open { "hide" } else { "show" })
    } else {
        format!("   {} steps so far · t full conversation", steps.len())
    };
    out.lines.push(Line::from(row));
    out.lines.push(Line::from(Span::styled(hint, t.dim)));
    if open {
        // Claude's thinking text arrives as its thinking ends, so a turn can have no steps yet.
        if steps.is_empty() {
            out.lines.push(Line::from(Span::styled(format!("   {}", tr("Nothing yet.")), t.dim)));
        }
        let mut lines = vec![];
        for e in steps {
            push_step(&mut lines, app, e, width, false);
        }
        let earlier = lines.len().saturating_sub(STEP_LINES);
        if earlier > 0 {
            out.lines.push(Line::from(Span::styled(format!("   … {earlier} earlier lines · t shows them"), t.dim)));
        }
        out.lines.extend(lines.into_iter().skip(earlier));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::{
        app::{Kind, Msg},
        net::Client,
    };
    use serde_json::json;

    fn render(app: &App, entries: &[Entry]) -> (String, usize) {
        render_with(app, entries, true)
    }

    fn render_with(app: &App, entries: &[Entry], details: bool) -> (String, usize) {
        let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
        activity(&mut out, app, &app.bots["bot"], &entries.iter().collect::<Vec<_>>(), 80, details);
        (out.lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"), out.buttons.len())
    }

    #[test]
    fn without_thinking_details_the_line_does_not_unfold() {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "bot", "name": "Bot", "activity": "Terminal"}})));
        let entries = [Entry {
            id: "tool".into(),
            seq: 1,
            turn: 1,
            kind: Kind::Tool,
            data: json!({"title": "rg TODO", "bookkeeping": false}),
            created_at: 0,
        }];
        app.steps_open = Some(("bot".into(), 1));
        let (text, buttons) = render_with(&app, &entries, false);
        assert!(text.contains("1 steps so far · t full conversation"));
        assert!(!text.contains("> show") && !text.contains("> hide") && !text.contains("rg TODO"));
        assert_eq!(buttons, 0);
    }

    #[test]
    fn steps_unfold_under_the_activity_for_the_running_turn() {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {
            "id": "bot", "name": "Bot", "activity": "Searching files…"
        }})));
        let entries = [
            Entry { id: "old".into(), seq: 1, turn: 1, kind: Kind::Tool, data: json!({}), created_at: 0 },
            Entry {
                id: "tool".into(),
                seq: 2,
                turn: 2,
                kind: Kind::Tool,
                data: json!({"title": "rg TODO", "bookkeeping": false}),
                created_at: 0,
            },
            Entry {
                id: "thought".into(),
                seq: 3,
                turn: 2,
                kind: Kind::Thought,
                data: json!({"text": "Looking for the parser"}),
                created_at: 0,
            },
            Entry {
                id: "queued".into(),
                seq: 4,
                turn: 3,
                kind: Kind::User,
                data: json!({"status": "queued"}),
                created_at: 0,
            },
        ];
        let (folded, buttons) = render(&app, &entries);
        assert!(folded.contains("Searching files…"));
        assert!(folded.contains("2 steps so far · > show · t full conversation"));
        assert!(!folded.contains("Looking for the parser") && !folded.contains("rg TODO"));
        assert_eq!(buttons, 1);

        app.steps_open = Some(("bot".into(), 2));
        let (open, _) = render(&app, &entries);
        assert!(open.contains("> hide"));
        assert!(open.contains("rg TODO") && open.contains("Looking for the parser"));

        // A new turn starts folded.
        app.steps_open = Some(("bot".into(), 1));
        assert!(!render(&app, &entries).0.contains("rg TODO"));
    }

    #[test]
    fn a_turn_without_steps_unfolds_to_nothing_yet() {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "bot", "name": "Bot", "activity": "Thinking…"}})));
        let entries =
            [Entry { id: "u".into(), seq: 1, turn: 1, kind: Kind::User, data: json!({"text": "hi"}), created_at: 0 }];
        let (folded, buttons) = render(&app, &entries);
        assert!(folded.contains("Thinking…  >") && folded.contains("> show") && !folded.contains("Nothing yet."));
        assert_eq!(buttons, 1);
        app.steps_open = Some(("bot".into(), 1));
        assert!(render(&app, &entries).0.contains("Nothing yet."));
    }
}
