//! A bot's background tasks: their own transcripts, the list and the read-only details, and stopping them.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{After, App, Confirm, ConfirmAct, Entry, Overlay, Task};
use super::super::l10n::tr;
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

/// The tasks of a bot that has several, by the bot's id.
pub struct TaskList {
    pub bot: String,
    pub cursor: usize,
}

/// What `taskEntries` said of a task: how it stands once it has left its bot's list.
pub enum Fetched {
    Loading,
    Failed(String),
    Loaded(Task),
}

/// A task's details: its transcript, scrolled from the bottom.
pub struct TaskSheet {
    pub bot: String,
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
    pub fn new(bot: String, task: String) -> Self {
        Self {
            bot,
            task,
            fetched: Fetched::Loading,
            full: false,
            scroll: 0,
            max_scroll: Cell::default(),
            page: Cell::default(),
        }
    }

    /// The task as it stands: the bot's own copy while it goes, else what was fetched.
    pub fn task<'a>(&'a self, app: &'a App) -> Option<&'a Task> {
        app.live_task(&self.bot, &self.task).or(match &self.fetched {
            Fetched::Loaded(task) => Some(task),
            Fetched::Loading | Fetched::Failed(_) => None,
        })
    }
}

impl App {
    fn live_task(&self, bot: &str, task: &str) -> Option<&Task> {
        self.bots.get(bot)?.tasks.iter().find(|t| t.id == task)
    }

    /// The selected bot and the ids of its tasks.
    fn selected_tasks(&self) -> Option<(String, Vec<String>)> {
        self.bot().map(|b| (b.id.clone(), b.tasks.iter().map(|t| t.id.clone()).collect()))
    }

    /// `B`: what the selected bot does in the background: its one task's details, or the list of them.
    pub(in crate::tui) fn open_tasks(&mut self) {
        let Some((bot, mut tasks)) = self.selected_tasks() else { return };
        match (tasks.pop(), tasks.is_empty()) {
            (None, _) => self.flash("Nothing in the background"),
            (Some(only), true) => self.open_task(bot, only),
            (Some(_), false) => self.overlays.push(Overlay::Tasks(TaskList { bot, cursor: 0 })),
        }
    }

    fn open_task(&mut self, bot: String, task: String) {
        self.fetch_task(&bot, &task);
        self.overlays.push(Overlay::Task(TaskSheet::new(bot, task)));
    }

    fn fetch_task(&self, bot: &str, task: &str) {
        self.sheet("taskEntries", json!({"botId": bot, "taskId": task}), Reply::TaskEntries(task.to_owned()));
    }

    /// The open details load again: a new connection brings none of a task's entries.
    pub(in crate::tui) fn reload_task_details(&self) {
        for o in &self.overlays {
            if let Overlay::Task(s) = o {
                self.fetch_task(&s.bot, &s.task);
            }
        }
    }

    /// `taskEntries` answered for `task`: its transcript joins what the stream brought, and its details show how it
    /// stands. A reload that failed leaves what they show.
    pub(in crate::tui) fn task_fetched(&mut self, task: &str, answer: Result<&Value, String>) {
        let loaded = answer.and_then(|v| {
            let found = Task::parse(&v["task"]).ok_or_else(|| tr("Something went wrong").into_owned())?;
            for entry in v["entries"].as_array().into_iter().flatten() {
                self.task_entries.keep(Some(task), entry);
            }
            Ok(found)
        });
        for o in &mut self.overlays {
            if let Overlay::Task(s) = o
                && s.task == task
            {
                match &loaded {
                    Ok(found) => s.fetched = Fetched::Loaded(found.clone()),
                    Err(e) if matches!(s.fetched, Fetched::Loading) => s.fetched = Fetched::Failed(e.clone()),
                    Err(_) => {}
                }
            }
        }
    }

    /// Tasks that left a bot's list are over: open details ask how it ended, and the rest's transcripts go.
    pub(in crate::tui) fn tasks_ended(&mut self, before: &[Task], now: &[Task]) {
        for task in before.iter().filter(|t| !now.iter().any(|n| n.id == t.id)) {
            let open = self.overlays.iter().find_map(|o| match o {
                Overlay::Task(s) if s.task == task.id => Some(s.bot.clone()),
                _ => None,
            });
            match open {
                Some(bot) => self.fetch_task(&bot, &task.id),
                None => self.task_entries.forget(&task.id),
            }
        }
    }

    /// `X`: stops the selected bot's background work, after asking once.
    pub(in crate::tui) fn ask_stop_tasks(&mut self) {
        let Some((bot, mut tasks)) = self.selected_tasks() else { return };
        match (tasks.pop(), tasks.is_empty()) {
            (None, _) => self.flash("Nothing in the background"),
            (only, true) => self.confirm_stop(bot, only),
            (Some(_), false) => self.confirm_stop(bot, None),
        }
    }

    /// Asks before stopping `task` of `bot`, or all of its tasks.
    fn confirm_stop(&mut self, bot: String, task: Option<String>) {
        self.overlays.push(Overlay::Confirm(Confirm {
            title: tr("Stop the background work?").into_owned(),
            detail: "What it already changed stays.".into(),
            note: String::new(),
            button: "Stop",
            act: ConfirmAct::StopTasks { bot, task },
        }));
    }

    pub(in crate::tui) fn stop_tasks(&mut self, bot: &str, task: Option<&str>) {
        let mut body = json!({"botId": bot});
        if let Some(task) = task {
            body["taskId"] = task.into();
        }
        self.call("stopTask", body, After::Nothing);
        self.flash("Stopping…");
    }

    pub(in crate::tui) fn task_list_key(&mut self, mut l: TaskList, k: KeyEvent) -> Option<Overlay> {
        let tasks = self.bots.get(&l.bot).map_or(&[][..], |b| b.tasks.as_slice());
        let last = tasks.len().saturating_sub(1);
        l.cursor = l.cursor.min(last);
        let picked = tasks.get(l.cursor).map(|t| t.id.clone());
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => l.cursor = (l.cursor + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => l.cursor = l.cursor.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(task) = picked {
                    self.open_task(l.bot.clone(), task);
                }
            }
            KeyCode::Char('X') if picked.is_some() => self.confirm_stop(l.bot.clone(), picked),
            _ => {}
        }
        Some(Overlay::Tasks(l))
    }

    pub(in crate::tui) fn task_key(&mut self, mut s: TaskSheet, k: KeyEvent) -> Option<Overlay> {
        let live = self.live_task(&s.bot, &s.task).is_some();
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                // Nothing brings an ended task's entries any more.
                if !live {
                    self.task_entries.forget(&s.task);
                }
                return None;
            }
            KeyCode::Char('T') => s.full = !s.full,
            KeyCode::Char('X') if live => self.confirm_stop(s.bot.clone(), Some(s.task.clone())),
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
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(app: &mut App, c: char) {
        let modifiers = if c.is_uppercase() { KeyModifiers::SHIFT } else { KeyModifiers::NONE };
        app.on_key(KeyEvent::new(KeyCode::Char(c), modifiers));
    }

    fn hint(app: &mut App) -> String {
        app.hint.take().map(|(text, _)| text).unwrap_or_default()
    }

    /// A bot `a`, selected, with these tasks (id, status).
    fn bot_with(app: &mut App, tasks: &[(&str, &str)]) {
        let tasks: Vec<Value> =
            tasks.iter().map(|(id, status)| json!({"id": id, "title": "Fix login", "status": status})).collect();
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "tasks": tasks}})));
        app.selected = Some("a".into());
    }

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
        bot_with(&mut app, &[("task-1", "working")]);
        for (seq, rev, text) in [(1, 5, "reading"), (2, 6, "half"), (2, 8, "whole")] {
            app.on_msg(Msg::Event(json!({"type": "taskEntry", "rev": rev, "task": "task-1",
                "entry": task_entry(seq, rev, text)})));
        }
        assert!(app.entries.is_empty(), "no chat holds them");
        assert!(app.lane("a").is_empty() && app.lane("task-1").is_empty());
        assert_eq!(texts(&app), ["reading", "whole"]);

        // What `taskEntries` answers joins them: an older copy of a known entry loses, the rest are new.
        let fetched = json!({"task": {"id": "task-1", "title": "Fix login", "status": "working", "startedAt": 1},
            "entries": [task_entry(0, 2, "told"), task_entry(2, 7, "stale")]});
        app.task_fetched("task-1", Ok(&fetched));
        assert_eq!(texts(&app), ["told", "reading", "whole"]);
    }

    #[tokio::test]
    async fn b_opens_the_details_or_the_list_and_x_asks_before_it_stops() {
        let mut app = test_app();
        bot_with(&mut app, &[]);
        key(&mut app, 'B');
        assert_eq!(hint(&mut app), "Nothing in the background");
        key(&mut app, 'X');
        assert_eq!(hint(&mut app), "Nothing in the background");
        assert!(app.overlays.is_empty(), "nothing to stop, nothing asked");

        bot_with(&mut app, &[("task-1", "working")]);
        // Typing: the letters are the message's.
        app.typing = true;
        key(&mut app, 'B');
        key(&mut app, 'X');
        assert!(app.overlays.is_empty());
        assert_eq!(app.drafts["a"].text, "BX");
        app.typing = false;

        key(&mut app, 'B');
        assert!(matches!(app.overlays.last(), Some(Overlay::Task(s)) if s.task == "task-1"));
        key(&mut app, 'X');
        let asked = |app: &App| match app.overlays.last() {
            Some(Overlay::Confirm(Confirm { act: ConfirmAct::StopTasks { task, .. }, .. })) => Some(task.clone()),
            _ => None,
        };
        assert_eq!(asked(&app), Some(Some("task-1".to_owned())), "from the details: that task");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(matches!(app.overlays.last(), Some(Overlay::Task(_))), "cancelled: the details stay");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.overlays.is_empty());

        key(&mut app, 'X');
        assert_eq!(asked(&app), Some(Some("task-1".to_owned())), "one task: by its id");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.overlays.is_empty());
        assert_eq!(hint(&mut app), "Stopping…");

        bot_with(&mut app, &[("task-1", "working"), ("task-2", "needsInput")]);
        key(&mut app, 'X');
        assert_eq!(asked(&app), Some(None), "several: all of the bot's");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        key(&mut app, 'B');
        assert!(matches!(app.overlays.last(), Some(Overlay::Tasks(_))), "several: the list first");
        key(&mut app, 'j');
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(app.overlays.last(), Some(Overlay::Task(s)) if s.task == "task-2"));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(matches!(app.overlays.last(), Some(Overlay::Tasks(_))), "esc goes back to the list");
    }

    #[tokio::test]
    async fn the_details_learn_how_a_task_ended_and_say_when_they_cannot_load() {
        let mut app = test_app();
        bot_with(&mut app, &[("task-1", "working"), ("task-2", "working")]);
        for task in ["task-1", "task-2"] {
            let mut entry = task_entry(1, 1, "step");
            entry["botId"] = task.into();
            app.on_msg(Msg::Event(json!({"type": "taskEntry", "rev": 1, "task": task, "entry": entry})));
        }
        app.open_task("a".into(), "task-1".into());
        let sheet = |app: &App| match app.overlays.last() {
            Some(Overlay::Task(s)) => s.task(app).map(|t| t.status),
            _ => None,
        };
        app.task_fetched("task-1", Err("the computer is unreachable".into()));
        assert!(matches!(app.overlays.last(), Some(Overlay::Task(TaskSheet { fetched: Fetched::Failed(_), .. }))));
        key(&mut app, 'r');
        assert!(matches!(app.overlays.last(), Some(Overlay::Task(TaskSheet { fetched: Fetched::Loading, .. }))));
        assert_eq!(sheet(&app), Some(crate::tui::app::TaskStatus::Working), "the bot's own copy while it goes");

        // Both end: the open one keeps its transcript and shows what the host says of it.
        bot_with(&mut app, &[]);
        assert_eq!(app.task_entries.of("task-2").count(), 0);
        assert_eq!(app.task_entries.of("task-1").count(), 1);
        assert_eq!(sheet(&app), None);
        let done = json!({"task": {"id": "task-1", "title": "Fix login", "status": "done", "startedAt": 1,
            "endedAt": 61_000}, "entries": []});
        app.task_fetched("task-1", Ok(&done));
        assert_eq!(sheet(&app), Some(crate::tui::app::TaskStatus::Done));
        app.task_fetched("task-1", Err("offline".into()));
        assert_eq!(sheet(&app), Some(crate::tui::app::TaskStatus::Done), "a failed reload keeps what it shows");
        key(&mut app, 'X');
        assert!(matches!(app.overlays.last(), Some(Overlay::Task(_))), "nothing left to stop");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.task_entries.of("task-1").count(), 0, "closed: its transcript goes");
    }
}
