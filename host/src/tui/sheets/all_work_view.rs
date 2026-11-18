//! Every bot's work on one page: each bot's name, under it the rows of its Details page's Threads and Event box.

use super::{App, Buffer, Rect, centered, footer, frame_box, put, row, theme, u};
use crate::tui::app::{can_cut_in, counts};
use crate::tui::l10n::tr;
use crate::tui::manage::{AllWork, AllWorkRow, WorkAct};
use crate::tui::md::truncate;
use crate::tui::view::work_row;

/// The keys of the picked row, for the page's last line.
fn keys(picked: Option<&AllWorkRow<'_>>) -> String {
    let own = match picked {
        Some(AllWorkRow::Bot(_)) => vec![tr("↵ opens").into_owned()],
        Some(AllWorkRow::Item(bot, row)) => WorkAct::keys(row, can_cut_in(&bot.work, bot.max_agents)),
        None => vec![],
    };
    own.into_iter().chain(["esc".into()]).collect::<Vec<_>>().join(" · ")
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, p: &AllWork) {
    let t = theme();
    let r = centered(area, 90, 34);
    let inner = frame_box(buf, r, t.text, t.panel, Some((tr("Work").as_ref(), t.text)));
    let roster = app.roster();
    let rows = AllWork::rows(&roster);
    let cursor = p.at(&rows);
    // One line a row, a blank one over each bot but the first: where each row is, and the picked one's line.
    let mut lines = vec![];
    for (i, row) in rows.iter().enumerate() {
        if i > 0 && matches!(row, AllWorkRow::Bot(_)) {
            lines.push(None);
        }
        lines.push(Some(i));
    }
    // The page can outgrow the sheet: it starts as far down as keeps the picked row in sight.
    let room = usize::from(inner.height.saturating_sub(2));
    let picked_line = lines.iter().position(|line| *line == Some(cursor)).unwrap_or(0);
    let top = (picked_line + 1).saturating_sub(room);
    for (y, line) in lines.iter().skip(top).take(room).enumerate() {
        let Some(i) = *line else { continue };
        let (y, selected) = (inner.y + u(y), i == cursor);
        let base = row(buf, inner, y, selected);
        match &rows[i] {
            AllWorkRow::Bot(bot) => {
                let name = truncate(&bot.name, usize::from(inner.width / 2));
                let x = put(buf, inner.x, y, inner.width, &name, t.bold.patch(base));
                put(buf, x + 2, y, inner.right().saturating_sub(x + 2), &counts(&bot.work), t.dim.patch(base));
            }
            AllWorkRow::Item(bot, item) => {
                let under = Rect::new(inner.x + 2, inner.y, inner.width.saturating_sub(2), inner.height);
                work_row(buf, under, y, (selected, base), app, &bot.id, item);
            }
        }
    }
    footer(buf, inner, None, &keys(rows.get(cursor)));
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::tui::app::{Msg, screen, test_app};

    fn bot(app: &mut App, id: &str, position: i64, work: &Value) {
        let bot = json!({"id": id, "name": id.to_uppercase(), "position": position, "work": work, "maxAgents": 1});
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": bot})));
    }

    fn page(app: &App, p: &AllWork) -> Vec<String> {
        let area = Rect::new(0, 0, 100, 36);
        screen(area, |buf| draw(buf, area, app, p))
    }

    #[tokio::test]
    async fn the_page_heads_each_bots_threads_and_numbered_box_with_its_name() {
        let mut app = test_app();
        bot(
            &mut app,
            "mika",
            2,
            &json!([{"id": "m1", "title": "Deploy", "status": "waiting", "wait": {"kind": "next"}}]),
        );
        bot(
            &mut app,
            "ada",
            1,
            &json!([
                {"id": "w", "title": "Write docs", "status": "working", "activity": "Reading files", "task": "task-w"},
                {"id": "n", "title": "Review the PR", "status": "needsYou", "source": {"kind": "request", "id": "mika"}},
                {"id": "q1", "title": "Refactor", "status": "waiting", "wait": {"kind": "next"}},
                {"id": "q2", "title": "Summary", "status": "waiting", "wait": {"kind": "behind", "ahead": 1},
                    "source": {"kind": "routine", "id": "r"}},
                {"id": "d", "title": "Old", "status": "done"},
            ]),
        );
        bot(&mut app, "idle", 0, &json!([{"id": "x", "title": "Over", "status": "failed"}]));
        let mut p = AllWork::default();
        let lines = page(&app, &p);
        let at = |text: &str| lines.iter().position(|line| line.contains(text)).unwrap_or_else(|| panic!("{text}"));
        assert!(lines[0].contains("─ Work ─"), "{lines:?}");
        let ada = at("ADA  1 working · 2 waiting");
        assert_eq!(at("◆ MIKA › Review the PR · Needs you"), ada + 1, "what needs you first, with who asked");
        assert_eq!(at("● Write docs · Reading files"), ada + 2);
        assert_eq!(at("1 Refactor · Next up"), ada + 3);
        assert_eq!(at("2 ↻ Summary · 1 ahead"), ada + 4, "a routine's run");
        assert_eq!(at("MIKA  1 waiting"), ada + 6, "a gap, then the next bot of the roster");
        assert_eq!(at("1 Deploy · Next up"), ada + 7);
        assert!(!lines.iter().any(|l| l.contains("IDLE") || l.contains("Old") || l.contains("Over")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("↵ opens · esc")), "a heading's key");

        // The picked row's keys are its Details page's.
        let foot = |p: &AllWork| page(&app, p).into_iter().rev().find(|l| l.contains("esc")).unwrap_or_default();
        p.on = Some("w".into());
        assert!(foot(&p).contains("↵ opens · X stops · esc"));
        p.on = Some("q1".into());
        assert!(foot(&p).contains("C Cut in · D Drop · esc"), "its one place is taken, and its own work gives way");
        p.on = Some("q2".into());
        assert!(foot(&p).contains("F First · D Drop · esc"));
        p.on = Some("m1".into());
        assert!(foot(&p).contains("D Drop · esc") && !foot(&p).contains("Cut in"), "nothing of that bot's works");
    }

    #[tokio::test]
    async fn a_long_page_scrolls_to_keep_the_picked_row_in_sight() {
        let mut app = test_app();
        let work: Vec<Value> = (0..50)
            .map(|i| json!({"id": format!("q{i}"), "title": format!("Item {i}"), "status": "waiting"}))
            .collect();
        bot(&mut app, "ada", 1, &Value::Array(work));
        let lines = page(&app, &AllWork { cursor: 0, on: Some("q49".into()) });
        assert!(
            lines.iter().any(|l| l.contains("50 Item 49")) && !lines.iter().any(|l| l.contains("ADA")),
            "{lines:?}"
        );
    }
}
