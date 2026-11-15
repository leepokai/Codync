//! A task's own transcript (the agent session working a bot's item), read from the item's row on the Details page.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Entry, Overlay, WorkItem};
use super::Reply;
use super::scroll::up_from_end;

/// Each task's transcript by task id, apart from every chat: its entries by `seq`, with the `rev` they came at.
#[derive(Default)]
pub struct TaskEntries(HashMap<String, BTreeMap<i64, (i64, Entry)>>);

impl TaskEntries {
    /// Keeps an entry of `task` (its own bot id when the event names none) once: the higher `rev` wins.
    pub(in crate::tui) fn keep(&mut self, task: Option<&str>, entry: &Value) {
        let Some((owner, e)) = Entry::parse(entry) else { return };
        let rev = entry["rev"].as_i64().unwrap_or(0);
        let held = self.0.entry(task.map_or(owner, str::to_owned)).or_default();
        if held.get(&e.seq).is_none_or(|(known, _)| *known <= rev) {
            held.insert(e.seq, (rev, e));
        }
    }

    /// `task`'s entries, oldest first.
    pub fn of(&self, task: &str) -> impl Iterator<Item = &Entry> {
        self.0.get(task).into_iter().flat_map(|held| held.values().map(|(_, e)| e))
    }

    fn forget(&mut self, task: &str) {
        self.0.remove(task);
    }
}

/// How the transcript `taskEntries` answers with stands.
pub enum Fetched {
    Loading,
    Failed(String),
    Loaded,
}

/// The transcript of the task that has or had an item, scrolled from the bottom.
pub struct TaskSheet {
    pub bot: String,
    /// The item its heading tells of.
    pub item: String,
    pub task: String,
    pub fetched: Fetched,
    /// Tool calls show their whole diff and output (`T`).
    pub full: bool,
    /// Lines up from the bottom.
    pub scroll: usize,
    /// Set by the last draw, for clamping the scroll and paging.
    pub max_scroll: Cell<usize>,
    pub page: Cell<usize>,
}

impl TaskSheet {
    pub fn new(bot: String, item: String, task: String) -> Self {
        Self {
            bot,
            item,
            task,
            fetched: Fetched::Loading,
            full: false,
            scroll: 0,
            max_scroll: Cell::default(),
            page: Cell::default(),
        }
    }

    /// Its item as the bot's box has it; none once it was dropped.
    pub fn item<'a>(&self, app: &'a App) -> Option<&'a WorkItem> {
        app.bots.get(&self.bot)?.work.iter().find(|i| i.id == self.item)
    }
}

/// Whether one of `work`'s live items is `task`'s: its entries still come.
fn works(work: &[WorkItem], task: &str) -> bool {
    work.iter().any(|i| i.status.live() && i.task.as_deref() == Some(task))
}

impl App {
    /// Opens the transcript of the task that has or had `item`; an item no task took has none.
    pub(in crate::tui) fn open_task(&mut self, bot: &str, item: &WorkItem) {
        let Some(task) = item.task.clone() else { return };
        self.fetch_task(bot, &task);
        self.overlays.push(Overlay::Task(TaskSheet::new(bot.to_owned(), item.id.clone(), task)));
    }

    fn fetch_task(&self, bot: &str, task: &str) {
        self.sheet("taskEntries", json!({"botId": bot, "taskId": task}), Reply::TaskEntries(task.to_owned()));
    }

    /// The open transcripts load again: a new connection brings none of a task's entries.
    pub(in crate::tui) fn reload_task_details(&self) {
        for o in &self.overlays {
            if let Overlay::Task(s) = o {
                self.fetch_task(&s.bot, &s.task);
            }
        }
    }

    /// `taskEntries` answered for `task`: its entries join what the stream brought. A reload that failed leaves what
    /// the sheet shows.
    pub(in crate::tui) fn task_fetched(&mut self, task: &str, answer: Result<&Value, String>) {
        let loaded = answer.map(|v| {
            for entry in v["entries"].as_array().into_iter().flatten() {
                self.task_entries.keep(Some(task), entry);
            }
        });
        for o in &mut self.overlays {
            if let Overlay::Task(s) = o
                && s.task == task
            {
                match &loaded {
                    Ok(()) => s.fetched = Fetched::Loaded,
                    Err(e) if matches!(s.fetched, Fetched::Loading) => s.fetched = Fetched::Failed(e.clone()),
                    Err(_) => {}
                }
            }
        }
    }

    /// A bot's box changed from `before` to `now`: the transcript of a task that no longer works goes, unless it is
    /// being read.
    pub(in crate::tui) fn tasks_ended(&mut self, before: &[WorkItem], now: &[WorkItem]) {
        for task in before.iter().filter(|i| i.status.live()).filter_map(|i| i.task.as_deref()) {
            let open = self.overlays.iter().any(|o| matches!(o, Overlay::Task(s) if s.task == task));
            if !open && !works(now, task) {
                self.task_entries.forget(task);
            }
        }
    }

    pub(in crate::tui) fn task_key(&mut self, mut s: TaskSheet, k: KeyEvent) -> Option<Overlay> {
        let live = s.item(self).filter(|i| i.status.live()).cloned();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                // Nothing brings an ended task's entries any more.
                if !self.bots.get(&s.bot).is_some_and(|b| works(&b.work, &s.task)) {
                    self.task_entries.forget(&s.task);
                }
                return None;
            }
            KeyCode::Char('T') => s.full = !s.full,
            KeyCode::Char('X') => {
                if let Some(item) = live {
                    let bot = s.bot.clone();
                    self.overlays.push(Overlay::Task(s));
                    self.confirm_stop(bot, &item);
                    return None;
                }
            }
            KeyCode::Char('r') if matches!(s.fetched, Fetched::Failed(_)) => {
                s.fetched = Fetched::Loading;
                self.fetch_task(&s.bot, &s.task);
            }
            _ => {
                up_from_end(&mut s.scroll, s.max_scroll.get(), s.page.get(), k);
            }
        }
        Some(Overlay::Task(s))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::{Msg, test_app};
    use super::super::work::tests::{bot_with, key};
    use super::*;

    fn task_entry(seq: i64, rev: i64, text: &str) -> Value {
        json!({"id": format!("t{seq}"), "botId": "task-1", "seq": seq, "rev": rev, "kind": "agent", "turn": 1,
            "data": {"text": text}})
    }

    fn texts(app: &App) -> Vec<&str> {
        app.task_entries.of("task-1").map(Entry::text).collect()
    }

    #[tokio::test]
    async fn a_tasks_entries_stay_out_of_the_chat_and_the_higher_rev_wins() {
        let mut app = test_app();
        bot_with(&mut app, &[("1", "working")]);
        for (seq, rev, text) in [(1, 5, "reading"), (2, 6, "half"), (2, 8, "whole")] {
            app.on_msg(Msg::Event(json!({"type": "taskEntry", "rev": rev, "task": "task-1",
                "entry": task_entry(seq, rev, text)})));
        }
        assert!(app.entries.is_empty(), "no chat holds them");
        assert!(app.lane("a").is_empty() && app.lane("task-1").is_empty());
        assert_eq!(texts(&app), ["reading", "whole"]);

        // What `taskEntries` answers joins them: an older copy of a known entry loses, the rest are new. Only its
        // `entries` are read.
        let fetched = json!({"task": {"id": "task-1", "title": "Another", "status": "later"},
            "entries": [task_entry(0, 2, "told"), task_entry(2, 7, "stale")]});
        app.task_fetched("task-1", Ok(&fetched));
        assert_eq!(texts(&app), ["told", "reading", "whole"]);
    }

    #[tokio::test]
    async fn the_transcript_stays_while_it_is_read_and_says_when_it_cannot_load() {
        let mut app = test_app();
        bot_with(&mut app, &[("1", "working"), ("2", "working")]);
        for task in ["task-1", "task-2"] {
            let mut entry = task_entry(1, 1, "step");
            entry["botId"] = task.into();
            app.on_msg(Msg::Event(json!({"type": "taskEntry", "rev": 1, "task": task, "entry": entry})));
        }
        let first = app.bots["a"].work[0].clone();
        app.open_task("a", &first);
        let fetched = |app: &App| match app.overlays.last() {
            Some(Overlay::Task(TaskSheet { fetched: Fetched::Loading, .. })) => "loading",
            Some(Overlay::Task(TaskSheet { fetched: Fetched::Failed(_), .. })) => "failed",
            Some(Overlay::Task(TaskSheet { fetched: Fetched::Loaded, .. })) => "loaded",
            _ => "closed",
        };
        app.task_fetched("task-1", Err("the computer is unreachable".into()));
        assert_eq!(fetched(&app), "failed");
        key(&mut app, 'r');
        assert_eq!(fetched(&app), "loading", "r tries again");
        app.task_fetched("task-1", Ok(&json!({"entries": []})));
        app.task_fetched("task-1", Err("offline".into()));
        assert_eq!(fetched(&app), "loaded", "a failed reload keeps what it shows");

        // Both end: the one being read keeps its transcript, and its heading reads the box.
        bot_with(&mut app, &[("1", "done"), ("2", "stopped")]);
        assert_eq!(app.task_entries.of("task-2").count(), 0);
        assert_eq!(app.task_entries.of("task-1").count(), 1);
        key(&mut app, 'X');
        assert!(matches!(app.overlays.last(), Some(Overlay::Task(_))), "nothing left to stop");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.task_entries.of("task-1").count(), 0, "closed: its transcript goes");

        // A waiting item no task took has no transcript to open.
        bot_with(&mut app, &[("3", "waiting")]);
        let waiting = WorkItem { task: None, ..app.bots["a"].work[0].clone() };
        app.open_task("a", &waiting);
        assert!(app.overlays.is_empty());
    }
}
