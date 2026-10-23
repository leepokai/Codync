//! The current activity and tool count, with a shortcut to Full conversation.

use ratatui::text::{Line, Span};

use super::super::app::{App, Bot, Entry, Kind};
use super::super::l10n::tr;
use super::super::md::truncate;
use super::style::{spin, theme};
use super::transcript::{Built, gap, name_style};

pub(super) fn activity(out: &mut Built, app: &App, bot: &Bot, entries: &[&Entry], width: usize) {
    let t = theme();
    gap(out);
    out.lines.push(Line::from(Span::styled(format!(" {}", bot.name), name_style(&bot.color))));
    let text = if bot.activity.is_empty() { tr("Working…") } else { tr(&bot.activity) };
    out.lines.push(Line::from(vec![
        Span::styled(format!(" {} ", spin(app)), t.secondary),
        Span::styled(truncate(text.as_ref(), width.saturating_sub(4)), t.secondary),
    ]));
    let turn = entries.iter().filter(|e| e.data["status"] != "queued").map(|e| e.turn).max();
    let steps = entries.iter().filter(|entry| Some(entry.turn) == turn && entry.kind == Kind::Tool).count();
    let hint = if steps == 0 {
        "   t full conversation".to_owned()
    } else {
        format!("   {steps} steps so far · t full conversation")
    };
    out.lines.push(Line::from(Span::styled(hint, t.dim)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::{app::Msg, net::Client};
    use serde_json::json;

    #[test]
    fn activity_shows_current_steps_without_a_disclosure() {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {
            "id": "bot", "name": "Bot", "activity": "Searching files…"
        }})));
        let entries = [
            Entry { id: "old".into(), seq: 1, turn: 1, kind: Kind::Tool, data: json!({}), created_at: 0 },
            Entry { id: "tool".into(), seq: 2, turn: 2, kind: Kind::Tool, data: json!({}), created_at: 0 },
            Entry {
                id: "thought".into(),
                seq: 3,
                turn: 2,
                kind: Kind::Thought,
                data: json!({"text": "Private preview text"}),
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
        let mut out = Built { lines: vec![], buttons: vec![], messages: vec![] };
        activity(&mut out, &app, &app.bots["bot"], &entries.iter().collect::<Vec<_>>(), 80);
        let text = out.lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Searching files…"));
        assert!(text.contains("1 steps so far · t full conversation"));
        assert!(!text.contains("Private preview text") && !text.contains("> thinking"));
        assert!(out.buttons.is_empty());
    }
}
