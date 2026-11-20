//! Every bot's work on one page (`E` from the roster): each bot that has a live or waiting task, in the roster's
//! order, with its Threads rows and its numbered Event box rows under its name. The rows and their keys are the
//! Details page's (`work.rs`).

use crossterm::event::{KeyCode, KeyEvent};

use super::super::app::{App, Bot, Overlay, Sections, WorkTask, can_cut_in};
use super::{DetailRow, WorkAct};

/// The page of every bot's work.
#[derive(Default)]
pub struct AllWork {
    /// The row the cursor was last on: where it stays when its row is gone.
    pub cursor: usize,
    /// That row's id (a bot's or a task's): rows come and go with no key pressed, and the cursor stays on its row.
    pub on: Option<String>,
}

/// A row of the page.
pub enum AllWorkRow<'a> {
    /// A bot's name: opens its Details.
    Bot(&'a Bot),
    /// One of that bot's tasks, as on its Details page.
    Task(&'a Bot, DetailRow<'a>),
}

impl AllWorkRow<'_> {
    fn id(&self) -> &str {
        match self {
            Self::Bot(bot) => &bot.id,
            Self::Task(_, row) => row.id(),
        }
    }
}

/// Whether `bot` has a task a thread has or that waits for one: the bots the page lists.
fn has_work(bot: &Bot) -> bool {
    bot.work.iter().any(WorkTask::open)
}

impl AllWork {
    /// The page top to bottom: each of `roster`'s bots with work, then its threads and what waits, numbered.
    pub fn rows<'a>(roster: &[&'a Bot]) -> Vec<AllWorkRow<'a>> {
        let mut out = vec![];
        for &bot in roster.iter().filter(|b| has_work(b)) {
            let Sections { threads, waiting, .. } = Sections::of(&bot.work);
            out.push(AllWorkRow::Bot(bot));
            out.extend(threads.into_iter().map(|task| AllWorkRow::Task(bot, DetailRow::Thread(task))));
            let numbered = waiting.into_iter().enumerate();
            out.extend(numbered.map(|(i, task)| AllWorkRow::Task(bot, DetailRow::Waiting(i + 1, task))));
        }
        out
    }

    /// Where the cursor is among `rows`: on its row while it is there, else where it was, on a row that exists.
    pub fn at(&self, rows: &[AllWorkRow<'_>]) -> usize {
        let kept = self.on.as_deref().and_then(|id| rows.iter().position(|row| row.id() == id));
        kept.unwrap_or(self.cursor).min(rows.len().saturating_sub(1))
    }
}

impl App {
    /// Whether any bot of the roster has a live or waiting task: while one does, `E` is offered.
    pub(in crate::tui) fn any_work(&self) -> bool {
        self.bots.values().any(|b| !b.hidden && has_work(b))
    }

    /// `E`: the page of every bot's work.
    pub(in crate::tui) fn open_all_work(&mut self) {
        if self.any_work() {
            self.overlays.push(Overlay::AllWork(AllWork::default()));
        } else {
            self.flash("No work");
        }
    }

    /// Nothing is live or waiting any more: the page of every bot's work closes.
    pub(in crate::tui) fn close_idle_work_page(&mut self) {
        if !self.any_work() {
            self.overlays.retain(|o| !matches!(o, Overlay::AllWork(_)));
        }
    }

    pub(in crate::tui) fn all_work_key(&mut self, mut p: AllWork, k: KeyEvent) -> Option<Overlay> {
        let roster = self.roster();
        let rows = AllWork::rows(&roster);
        let at = p.at(&rows);
        let step = match k.code {
            KeyCode::Esc | KeyCode::Char('q' | 'E') => return None,
            KeyCode::Down | KeyCode::Char('j') => Some((at + 1).min(rows.len().saturating_sub(1))),
            KeyCode::Up | KeyCode::Char('k') => Some(at.saturating_sub(1)),
            _ => None,
        };
        // The cursor is kept by its row's id, also when the key isn't a move.
        let now = step.unwrap_or(at);
        (p.cursor, p.on) = (now, rows.get(now).map(|row| row.id().to_owned()));
        if step.is_some() {
            return Some(Overlay::AllWork(p));
        }
        match rows.get(at) {
            Some(AllWorkRow::Bot(bot)) if k.code == KeyCode::Enter => {
                let bot = bot.id.clone();
                self.overlays.push(Overlay::AllWork(p));
                self.select(&bot);
                self.open_details();
                None
            }
            Some(AllWorkRow::Task(bot, row)) => {
                let full = can_cut_in(&bot.work, bot.max_threads);
                let found = WorkAct::of(k.code, row, full).zip(row.task().cloned());
                let Some((act, task)) = found else { return Some(Overlay::AllWork(p)) };
                let bot = bot.id.clone();
                self.work_act(&bot, act, &task, Overlay::AllWork(p))
            }
            _ => Some(Overlay::AllWork(p)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::{Confirm, ConfirmAct, Focus, Msg, test_app};
    use super::super::work::tests::key;
    use super::*;
    use serde_json::{Value, json};

    /// Bot `id` at roster place `position` with these tasks (id, status).
    fn bot(app: &mut App, id: &str, position: i64, tasks: &[(&str, &str)]) {
        let work: Vec<Value> = tasks
            .iter()
            .map(|(task, status)| json!({"id": task, "title": task, "status": status, "thread": format!("thread-{task}")}))
            .collect();
        let bot = json!({"id": id, "name": id.to_uppercase(), "position": position, "work": work, "maxThreads": 1});
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": bot})));
    }

    fn listed(app: &App) -> Vec<String> {
        let rows = AllWork::rows(&app.roster());
        rows.iter()
            .map(|row| match row {
                AllWorkRow::Bot(bot) => bot.name.clone(),
                AllWorkRow::Task(_, DetailRow::Waiting(place, task)) => format!("{place}:{}", task.id),
                AllWorkRow::Task(_, row) => row.id().to_owned(),
            })
            .collect()
    }

    /// The id of the row the open page's cursor is on.
    fn on_row(app: &App) -> String {
        let Some(Overlay::AllWork(p)) = app.overlays.last() else { panic!("the page of all work is open") };
        let rows = AllWork::rows(&app.roster());
        rows[p.at(&rows)].id().to_owned()
    }

    fn enter(app: &mut App) {
        app.on_key(KeyEvent::from(KeyCode::Enter));
    }

    #[tokio::test]
    async fn the_page_lists_each_bot_with_work_in_the_rosters_order() {
        let mut app = test_app();
        bot(&mut app, "c", 3, &[("c1", "waiting"), ("c2", "waiting")]);
        bot(&mut app, "a", 1, &[("a1", "working"), ("a2", "needsYou"), ("a3", "waiting"), ("a4", "done")]);
        bot(&mut app, "b", 2, &[("b1", "done"), ("b2", "failed")]);
        assert_eq!(listed(&app), ["A", "a2", "a1", "1:a3", "C", "1:c1", "2:c2"], "no idle bot, no ended task");
        assert!(app.any_work());

        // `E` opens it only while some bot has such tasks, and it closes when the last one is over.
        key(&mut app, 'E');
        assert_eq!(on_row(&app), "a", "on the first bot");
        bot(&mut app, "a", 1, &[("a4", "done")]);
        assert!(matches!(app.overlays.last(), Some(Overlay::AllWork(_))), "another bot still has work");
        bot(&mut app, "c", 3, &[("c1", "stopped")]);
        assert!(app.overlays.is_empty(), "nothing left: the page closes");
        assert!(!app.any_work());
        key(&mut app, 'E');
        assert!(app.overlays.is_empty());
        assert_eq!(app.hint.take().map(|(text, _)| text).as_deref(), Some("No work"));
    }

    #[tokio::test]
    async fn the_rows_take_the_details_keys_and_a_heading_opens_its_bots_details() {
        let mut app = test_app();
        bot(&mut app, "a", 1, &[("a1", "working"), ("a3", "waiting"), ("a5", "waiting")]);
        bot(&mut app, "c", 3, &[("c1", "needsYou")]);
        key(&mut app, 'E');
        key(&mut app, 'X');
        assert!(matches!(app.overlays.last(), Some(Overlay::AllWork(_))), "a heading stops nothing");

        // A thread: its transcript, and X asks before it stops, of the row's own bot.
        key(&mut app, 'j');
        enter(&mut app);
        assert!(matches!(app.overlays.pop(), Some(Overlay::Thread(s)) if s.bot == "a" && s.thread == "thread-a1"));
        key(&mut app, 'X');
        assert!(matches!(
            app.overlays.pop(),
            Some(Overlay::Confirm(Confirm { act: ConfirmAct::StopWork { bot, task }, .. })) if bot == "a" && task == "a1"
        ));
        // The first in line cuts in (the bot's one place is taken); the next moves first or is dropped.
        key(&mut app, 'j');
        key(&mut app, 'F');
        assert!(matches!(app.overlays.last(), Some(Overlay::AllWork(_))), "the first is first already");
        key(&mut app, 'C');
        assert!(matches!(app.overlays.pop(), Some(Overlay::CutIn(c)) if c.bot == "a" && c.task == "a3"));
        key(&mut app, 'j');
        for c in ['C', 'F', 'D', 'A'] {
            key(&mut app, c);
            assert_eq!(on_row(&app), "a5", "{c}: the page stays, on its row");
        }
        // Rows come and go: the cursor stays on its row.
        bot(&mut app, "a", 1, &[("a1", "working"), ("a5", "waiting")]);
        assert_eq!(on_row(&app), "a5");
        bot(&mut app, "a", 1, &[("a1", "working")]);
        assert_eq!(on_row(&app), "c1", "its row is gone: a row that exists");
        key(&mut app, 'k');

        // A heading opens that bot's Details, over the page; esc comes back.
        enter(&mut app);
        assert!(matches!(app.overlays.last(), Some(Overlay::Details(l)) if l.bot == "c"));
        assert_eq!(app.selected.as_deref(), Some("c"));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(on_row(&app), "c");
        // What needs you leads to its bot's chat, where the card waits.
        key(&mut app, 'j');
        app.selected = Some("a".into());
        enter(&mut app);
        assert!(app.overlays.is_empty());
        assert_eq!((app.selected.as_deref(), app.focus, app.chat_page), (Some("c"), Focus::Chat, true));
    }
}
