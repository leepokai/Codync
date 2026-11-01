//! Every routine on the computer, grouped by the bot that runs it (`allRoutines`).

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Overlay};
use super::{Reply, RoutineForm};

pub struct AllRoutines {
    /// The host's routines, as listed; `None` while they load.
    pub items: Option<Vec<Value>>,
    /// Over the routines only, in the order shown (bot headers are skipped).
    pub cursor: usize,
    /// `n` with several bots: which bot the new routine is for (a cursor over `routine_bots`).
    pub chooser: Option<usize>,
}

/// A bot's routines, by name, under the bot's id; bots in `order`, those without routines and
/// routines of bots not in `order` left out.
pub fn grouped<'a>(order: &[&'a str], items: &'a [Value]) -> Vec<(&'a str, Vec<&'a Value>)> {
    order
        .iter()
        .filter_map(|&bot| {
            let mut mine: Vec<&Value> = items.iter().filter(|r| r["botId"].as_str() == Some(bot)).collect();
            mine.sort_by_cached_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
            (!mine.is_empty()).then_some((bot, mine))
        })
        .collect()
}

impl App {
    /// The bots routines can belong to: agent bots in roster order, then hidden ones by name.
    pub(in crate::tui) fn routine_bots(&self) -> Vec<&str> {
        let mut bots: Vec<&str> = self.roster().into_iter().filter(|b| !b.group).map(|b| b.id.as_str()).collect();
        let mut hidden: Vec<_> = self.bots.values().filter(|b| b.hidden && !b.group).collect();
        hidden.sort_by(|a, b| a.name.cmp(&b.name));
        bots.extend(hidden.into_iter().map(|b| b.id.as_str()));
        bots
    }

    /// The routines as the sheet shows them, grouped by bot.
    pub(in crate::tui) fn all_routines_grouped<'a>(&'a self, items: &'a [Value]) -> Vec<(&'a str, Vec<&'a Value>)> {
        grouped(&self.routine_bots(), items)
    }

    pub(in crate::tui) fn open_all_routines(&mut self) {
        self.sheet("allRoutines", json!({}), Reply::AllRoutines);
        self.overlays.push(Overlay::AllRoutines(AllRoutines { items: None, cursor: 0, chooser: None }));
    }

    pub(in crate::tui) fn all_routines_key(&mut self, mut l: AllRoutines, k: KeyEvent) -> Option<Overlay> {
        if let Some(i) = l.chooser {
            return self.routine_bot_key(l, i, k);
        }
        let (n, current) = l.items.as_deref().map_or((0, None), |items| {
            let rows: Vec<&Value> = self.all_routines_grouped(items).into_iter().flat_map(|(_, v)| v).collect();
            (rows.len(), rows.get(l.cursor).map(|r| (*r).clone()))
        });
        match (k.code, current) {
            (KeyCode::Esc | KeyCode::Char('q' | 'O'), _) => return None,
            (KeyCode::Down | KeyCode::Char('j'), _) => l.cursor = (l.cursor + 1).min(n.saturating_sub(1)),
            (KeyCode::Up | KeyCode::Char('k'), _) => l.cursor = l.cursor.saturating_sub(1),
            (KeyCode::Char('n'), _) => match self.routine_bots().as_slice() {
                [] => self.flash("Routines belong to a bot, not a group"),
                [bot] => {
                    let form = RoutineForm::new((*bot).to_owned());
                    self.overlays.push(Overlay::AllRoutines(l));
                    self.overlays.push(Overlay::Routine(Box::new(form)));
                    return None;
                }
                _ => l.chooser = Some(0),
            },
            (KeyCode::Enter | KeyCode::Char('e'), Some(r)) => {
                let form = RoutineForm::editing(super::s(&r, "botId"), &r);
                self.overlays.push(Overlay::AllRoutines(l));
                self.overlays.push(Overlay::Routine(Box::new(form)));
                return None;
            }
            (KeyCode::Char(' '), Some(r)) => {
                let on = self.switch_routine(&super::s(&r, "botId"), &r, Reply::AllRoutines);
                let item = l.items.as_mut().and_then(|v| v.iter_mut().find(|x| x["id"] == r["id"]));
                if let Some(item) = item {
                    item["enabled"] = on.into();
                }
            }
            _ => {}
        }
        Some(Overlay::AllRoutines(l))
    }

    /// Keys in "Which bot runs it?": ↵ opens the new routine form for the bot picked.
    fn routine_bot_key(&mut self, mut l: AllRoutines, i: usize, k: KeyEvent) -> Option<Overlay> {
        let bots = self.routine_bots();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => l.chooser = None,
            KeyCode::Down | KeyCode::Char('j') => l.chooser = Some((i + 1).min(bots.len().saturating_sub(1))),
            KeyCode::Up | KeyCode::Char('k') => l.chooser = Some(i.saturating_sub(1)),
            KeyCode::Enter => {
                if let Some(bot) = bots.get(i) {
                    let form = RoutineForm::new((*bot).to_owned());
                    l.chooser = None;
                    self.overlays.push(Overlay::AllRoutines(l));
                    self.overlays.push(Overlay::Routine(Box::new(form)));
                    return None;
                }
            }
            _ => {}
        }
        Some(Overlay::AllRoutines(l))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent};

    use crate::tui::app::{Bot, test_app};

    fn routine(id: &str, bot: &str, name: &str) -> Value {
        json!({"id": id, "botId": bot, "name": name, "enabled": true})
    }

    #[test]
    fn routines_group_by_bot_in_roster_order_and_by_name() {
        let items = [
            routine("1", "b", "zeta"),
            routine("2", "a", "Morning"),
            routine("3", "b", "alpha"),
            routine("4", "gone", "orphan"),
        ];
        let g = grouped(&["b", "a", "c"], &items);
        let shown: Vec<(&str, Vec<&str>)> =
            g.iter().map(|(bot, v)| (*bot, v.iter().map(|r| r["id"].as_str().unwrap_or_default()).collect())).collect();
        assert_eq!(shown, [("b", vec!["3", "1"]), ("a", vec!["2"])], "unknown bots and empty ones are left out");
    }

    #[tokio::test]
    async fn n_asks_which_bot_only_when_there_are_several() {
        let mut app = test_app();
        for (id, kind) in [("a", "agent"), ("g", "group")] {
            let bot = Bot::parse(&json!({"id": id, "name": id, "kind": kind})).expect("a bot with an id parses");
            app.bots.insert(id.into(), bot);
        }
        app.overlays.push(Overlay::AllRoutines(AllRoutines { items: Some(Vec::new()), cursor: 0, chooser: None }));
        app.on_key(KeyEvent::from(KeyCode::Char('n')));
        assert!(matches!(app.overlays.last(), Some(Overlay::Routine(f)) if f.bot == "a"), "one bot: straight to it");

        app.overlays.clear();
        app.bots.insert("b".into(), Bot::parse(&json!({"id": "b", "name": "b"})).expect("a bot with an id parses"));
        app.overlays.push(Overlay::AllRoutines(AllRoutines { items: Some(Vec::new()), cursor: 0, chooser: None }));
        app.on_key(KeyEvent::from(KeyCode::Char('n')));
        assert!(matches!(app.overlays.last(), Some(Overlay::AllRoutines(l)) if l.chooser == Some(0)));
        app.on_key(KeyEvent::from(KeyCode::Char('j')));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let picked = app.routine_bots()[1].to_owned();
        assert!(matches!(app.overlays.last(), Some(Overlay::Routine(f)) if f.bot == picked));
        assert!(matches!(app.overlays.first(), Some(Overlay::AllRoutines(l)) if l.chooser.is_none()));
    }

    #[tokio::test]
    async fn the_sheet_shows_each_bots_routines_under_its_name() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut app = test_app();
        app.bots.insert("a".into(), Bot::parse(&json!({"id": "a", "name": "Scout"})).expect("a bot with an id parses"));
        let mut broken = routine("1", "a", "Digest");
        broken["lastError"] = "bad cron".into();
        let mut daily = routine("2", "a", "Backup");
        daily["triggerDescriptions"] = json!(["daily 09:00"]);
        app.overlays.push(Overlay::AllRoutines(AllRoutines {
            items: Some(vec![broken, daily]),
            cursor: 0,
            chooser: None,
        }));
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw");
        let screen: String = terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for text in ["Scout", "Backup", "daily 09:00", "Digest", "Schedule needs attention"] {
            assert!(screen.contains(text), "{text} is on screen");
        }
        assert!(screen.find("Backup") < screen.find("Digest"), "by name");
    }
}
