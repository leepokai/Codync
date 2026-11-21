//! The Details page's rows (what the bot does: its threads, its event box, what it does later, its routines, what
//! ended) and what the keys do to a task of the box (`docs/plans/event-box.md`).

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

#[cfg(test)]
use super::super::app::Source;
use super::super::app::{
    After, App, Background, Confirm, ConfirmAct, Focus, Overlay, Sections, WorkStatus, WorkTask, can_cut_in,
    threads_at_once,
};
use super::super::l10n::tr;
use super::DetailsPage;

/// A row of the Details page.
pub enum DetailRow<'a> {
    /// A task a thread has: it needs the user, or works.
    Thread(&'a WorkTask),
    /// A waiting task and its place in line, from 1.
    Waiting(usize, &'a WorkTask),
    FollowUp(&'a Value),
    Routine(&'a Value),
    /// A task that ended in the last day.
    Recent(&'a WorkTask),
}

impl<'a> DetailRow<'a> {
    pub fn id(&self) -> &'a str {
        match self {
            Self::Thread(task) | Self::Waiting(_, task) | Self::Recent(task) => &task.id,
            Self::FollowUp(v) | Self::Routine(v) => v["id"].as_str().unwrap_or_default(),
        }
    }

    pub(super) fn task(&self) -> Option<&'a WorkTask> {
        match self {
            Self::Thread(task) | Self::Waiting(_, task) | Self::Recent(task) => Some(task),
            Self::FollowUp(_) | Self::Routine(_) => None,
        }
    }
}

/// A section of the Details page: its heading and rows.
pub struct DetailSection<'a> {
    /// A catalog phrase.
    pub title: &'static str,
    /// What the heading carries after its title: `2 / 3` threads of the most allowed, how many wait.
    pub count: String,
    pub rows: Vec<DetailRow<'a>>,
    /// Threads: the bot's threads not in use, one faint row each.
    pub free: usize,
}

impl DetailsPage {
    /// The page top to bottom, empty sections left out: Threads (only while a task is live or waiting), Event
    /// box, Later, Routines (always: it says when there are none), Recent.
    pub fn sections<'a>(&'a self, work: &'a [WorkTask], max_threads: Option<u8>) -> Vec<DetailSection<'a>> {
        let Sections { threads, waiting, recent } = Sections::of(work);
        let (running, waits, most) = (threads.len(), waiting.len(), threads_at_once(max_threads));
        // A thread waiting on the user takes no seat.
        let taken = work.iter().filter(|i| i.takes_seat()).count();
        let section = |title, count, rows| DetailSection { title, count, rows, free: 0 };
        let mut out = vec![];
        if running + waits > 0 {
            let rows = threads.into_iter().map(DetailRow::Thread).collect();
            let free = most.saturating_sub(taken);
            out.push(DetailSection { title: "Threads", count: format!("{taken} / {most}"), rows, free });
        }
        if waits > 0 {
            let rows = waiting.into_iter().enumerate().map(|(i, task)| DetailRow::Waiting(i + 1, task)).collect();
            out.push(section("Event box", waits.to_string(), rows));
        }
        if !self.follow_ups.is_empty() {
            out.push(section("Later", String::new(), self.follow_ups.iter().map(DetailRow::FollowUp).collect()));
        }
        out.push(section("Routines", String::new(), self.routines.iter().flatten().map(DetailRow::Routine).collect()));
        if !recent.is_empty() {
            out.push(section("Recent", String::new(), recent.into_iter().map(DetailRow::Recent).collect()));
        }
        out
    }

    /// Every row the cursor moves over, top to bottom.
    pub fn rows<'a>(&'a self, work: &'a [WorkTask], max_threads: Option<u8>) -> Vec<DetailRow<'a>> {
        self.sections(work, max_threads).into_iter().flat_map(|s| s.rows).collect()
    }

    /// The row the cursor is on.
    pub fn picked<'a>(&'a self, work: &'a [WorkTask], max_threads: Option<u8>) -> Option<DetailRow<'a>> {
        self.rows(work, max_threads).into_iter().nth(self.cursor)
    }

    /// Rows come and go with no key pressed: the cursor stays on the row `picked` (an id) while it is there, else
    /// where it was, on a row that exists.
    pub(in crate::tui) fn keep_cursor(&mut self, picked: Option<&str>, work: &[WorkTask], max_threads: Option<u8>) {
        let rows = self.rows(work, max_threads);
        let kept = picked.and_then(|id| rows.iter().position(|row| row.id() == id));
        self.cursor = kept.unwrap_or(self.cursor).min(rows.len().saturating_sub(1));
    }
}

/// What a key does to the task on a row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorkAct {
    /// Its thread's transcript; for one that needs the user, the chat at its card.
    Open,
    Stop,
    First,
    CutIn,
    Drop,
    Retry,
}

impl WorkAct {
    /// Each act's key and what the page's last line calls it (catalog phrases).
    const KEYS: [(Self, KeyCode, &str, &str); 6] = [
        (Self::Open, KeyCode::Enter, "↵ opens", ""),
        (Self::Stop, KeyCode::Char('X'), "X stops", ""),
        (Self::First, KeyCode::Char('F'), "F", "First"),
        (Self::CutIn, KeyCode::Char('C'), "C", "Cut in"),
        (Self::Drop, KeyCode::Char('D'), "D", "Drop"),
        (Self::Retry, KeyCode::Char('A'), "A", "Try again"),
    ];

    /// Whether `row` offers it; `cut_in`: the first in line may cut in (`can_cut_in`).
    fn offered(self, row: &DetailRow<'_>, cut_in: bool) -> bool {
        match (self, row) {
            (Self::Open, DetailRow::Thread(task) | DetailRow::Recent(task)) => {
                task.thread.is_some() || task.status == WorkStatus::NeedsYou
            }
            (Self::Stop, DetailRow::Thread(_)) | (Self::Drop, DetailRow::Waiting(..) | DetailRow::Recent(_)) => true,
            (Self::First, DetailRow::Waiting(place, _)) => *place > 1,
            (Self::CutIn, DetailRow::Waiting(1, _)) => cut_in,
            (Self::Retry, DetailRow::Recent(task)) => task.can_retry(),
            _ => false,
        }
    }

    /// What `key` does on `row`.
    pub(super) fn of(key: KeyCode, row: &DetailRow<'_>, cut_in: bool) -> Option<Self> {
        Self::KEYS.iter().find(|(act, code, ..)| *code == key && act.offered(row, cut_in)).map(|(act, ..)| *act)
    }

    /// The keys `row` offers, as the page's last line lists them; empty on a row that isn't a task's.
    pub fn keys(row: &DetailRow<'_>, cut_in: bool) -> Vec<String> {
        let offered = Self::KEYS.iter().filter(|(act, ..)| act.offered(row, cut_in));
        offered.map(|(_, _, key, word)| [tr(key), tr(word)].join(" ").trim().to_owned()).collect()
    }

    /// The host call of an act that needs no question first.
    fn method(self) -> Option<&'static str> {
        match self {
            Self::First => Some("workFirst"),
            Self::Drop => Some("dropWork"),
            Self::Retry => Some("retryWork"),
            Self::Open | Self::Stop | Self::CutIn => None,
        }
    }
}

/// The question a first-in-line task asks when every thread is busy: which working task gives way.
pub struct CutIn {
    pub bot: String,
    /// The waiting task that cuts in.
    pub task: String,
    /// Over the tasks that give way.
    pub cursor: usize,
}

/// The threads that may give way to a task that cuts in, in the Threads section's order: only work the bot added.
pub fn giving_way(work: &[WorkTask]) -> Vec<&WorkTask> {
    Sections::of(work).threads.into_iter().filter(|i| i.gives_way()).collect()
}

/// `workCutIn`: `task` cuts in and `instead_of` gives way (none: the host picks the one that started last).
fn cut_in_body(bot: &str, task: &str, instead_of: Option<&str>) -> Value {
    let mut body = json!({"botId": bot, "taskId": task});
    if let Some(gives_way) = instead_of {
        body["insteadOf"] = gives_way.into();
    }
    body
}

impl App {
    /// `bot`'s box and the most agents it runs at once.
    pub(in crate::tui) fn box_of(&self, bot: &str) -> (&[WorkTask], Option<u8>) {
        self.bots.get(bot).map_or((&[], None), |b| (&b.work, b.max_threads))
    }

    /// A bot's box changed: the cursor of its open Details page stays on its row.
    pub(in crate::tui) fn keep_details_cursor(&mut self, bot: &str, before: (&[WorkTask], Option<u8>)) {
        let Some(b) = self.bots.get(bot) else { return };
        for o in &mut self.overlays {
            if let Overlay::Details(l) = o
                && l.bot == bot
            {
                let picked = l.picked(before.0, before.1).map(|row| row.id().to_owned());
                l.keep_cursor(picked.as_deref(), &b.work, b.max_threads);
            }
        }
    }

    /// A key on the Details page's picked row, when it is a task's and the key is one of its own: what became of the
    /// page. `Err` hands the page back for the page's other keys.
    pub(super) fn work_key(&mut self, l: DetailsPage, k: KeyEvent) -> Result<Option<Overlay>, DetailsPage> {
        let (work, max_threads) = self.box_of(&l.bot);
        let picked = l.picked(work, max_threads);
        let act = picked.as_ref().and_then(|row| WorkAct::of(k.code, row, can_cut_in(work, max_threads)));
        let (Some(act), Some(task)) = (act, picked.and_then(|row| row.task().cloned())) else { return Err(l) };
        let bot = l.bot.clone();
        Ok(self.work_act(&bot, act, &task, Overlay::Details(l)))
    }

    /// Does `act` to `task` of `bot` from `page` (the bot's Details, or the page of every bot's work): what became
    /// of the page.
    pub(super) fn work_act(&mut self, bot: &str, act: WorkAct, task: &WorkTask, page: Overlay) -> Option<Overlay> {
        if let Some(method) = act.method() {
            self.call(method, json!({"botId": bot, "taskId": task.id}), After::Nothing);
            return Some(page);
        }
        if act == WorkAct::Open && task.status == WorkStatus::NeedsYou {
            // Its card in the bot's chat, picked: on screen, and the chat's keys answer it.
            self.select(bot);
            self.chat_page = true;
            self.focus = Focus::Chat;
            self.chat_scroll = 0;
            self.pick = self.card_of(bot, task).map(|e| e.id.clone());
            return None;
        }
        self.overlays.push(page);
        match act {
            WorkAct::Open => self.open_thread(bot, task),
            WorkAct::Stop => self.confirm_stop(bot.to_owned(), task),
            _ => self.overlays.push(Overlay::CutIn(CutIn { bot: bot.to_owned(), task: task.id.clone(), cursor: 0 })),
        }
        None
    }

    /// `X` in the chat: stops the one task a thread has, after asking; with several, the Details page lists them.
    pub(in crate::tui) fn ask_stop_work(&mut self) {
        let Some(b) = self.bot() else { return };
        let Some(background) = Background::of(&b.work) else {
            self.flash("No work");
            return;
        };
        match b.work.iter().find(|i| background.stops.as_deref() == Some(i.id.as_str())).cloned() {
            Some(task) => self.confirm_stop(b.id.clone(), &task),
            None => self.open_details(),
        }
    }

    /// Asks before stopping `task` of `bot`.
    pub(super) fn confirm_stop(&mut self, bot: String, task: &WorkTask) {
        self.overlays.push(Overlay::Confirm(Confirm {
            title: tr("Stop this task?").into_owned(),
            detail: "What it already changed stays.".into(),
            note: String::new(),
            button: "Stop",
            act: ConfirmAct::StopWork { bot, task: task.id.clone() },
        }));
    }

    pub(in crate::tui) fn stop_work(&mut self, bot: &str, task: &str) {
        self.call("stopWork", json!({"botId": bot, "taskId": task}), After::Nothing);
        self.flash("Stopping…");
    }

    pub(in crate::tui) fn cut_in_key(&mut self, mut c: CutIn, k: KeyEvent) -> Option<Overlay> {
        let threads = giving_way(self.box_of(&c.bot).0);
        let last = threads.len().saturating_sub(1);
        c.cursor = c.cursor.min(last);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Down | KeyCode::Char('j') => c.cursor = (c.cursor + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => c.cursor = c.cursor.saturating_sub(1),
            KeyCode::Enter => {
                let body = cut_in_body(&c.bot, &c.task, threads.get(c.cursor).map(|i| i.id.as_str()));
                self.call("workCutIn", body, After::Nothing);
                return None;
            }
            _ => {}
        }
        Some(Overlay::CutIn(c))
    }
}

#[cfg(test)]
pub(in crate::tui) mod tests {
    use super::super::super::app::{Msg, test_app};
    use super::*;
    use crossterm::event::KeyModifiers;

    pub(in crate::tui) fn key(app: &mut App, c: char) {
        let modifiers = if c.is_uppercase() { KeyModifiers::SHIFT } else { KeyModifiers::NONE };
        app.on_key(KeyEvent::new(KeyCode::Char(c), modifiers));
    }

    /// `task` as a routine's run.
    fn run_of(task: &WorkTask) -> WorkTask {
        WorkTask { id: "run".into(), source: Some(Source::Routine), ..task.clone() }
    }

    fn hint(app: &mut App) -> String {
        app.hint.take().map(|(text, _)| text).unwrap_or_default()
    }

    /// A bot `a`, selected, with these tasks (id, status), each `Fix login` of thread `thread-<id>`.
    pub(in crate::tui) fn bot_with(app: &mut App, tasks: &[(&str, &str)]) {
        let work: Vec<Value> = tasks
            .iter()
            .map(|(id, status)| {
                let thread = if *status == "waiting" { Value::Null } else { format!("thread-{id}").into() };
                json!({"id": id, "title": "Fix login", "status": status, "thread": thread})
            })
            .collect();
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        app.selected = Some("a".into());
    }

    /// The Details page of bot `a`, with a follow-up `f` and a routine `r`.
    fn details(app: &mut App) {
        app.overlays.push(Overlay::Details(DetailsPage {
            bot: "a".into(),
            routines: Some(vec![json!({"id": "r", "name": "Morning"})]),
            follow_ups: vec![json!({"id": "f", "note": "Check the build"})],
            ..Default::default()
        }));
    }

    fn page(app: &App) -> &DetailsPage {
        let Some(Overlay::Details(l)) = app.overlays.last() else { panic!("the Details page is open") };
        l
    }

    /// The id of the row the cursor is on.
    fn on_row(app: &App) -> String {
        let (work, max_threads) = app.box_of("a");
        page(app).picked(work, max_threads).map(|row| row.id().to_owned()).unwrap_or_default()
    }

    /// Moves the cursor to row `id`.
    fn pick(app: &mut App, id: &str) {
        let at = page(app).rows(app.box_of("a").0, None).iter().position(|row| row.id() == id).expect("a row");
        let Some(Overlay::Details(l)) = app.overlays.last_mut() else { panic!("the Details page is open") };
        l.cursor = at;
    }

    const BOX: [(&str, &str); 7] = [
        ("w1", "working"),
        ("n", "needsYou"),
        ("w2", "working"),
        ("q1", "waiting"),
        ("q2", "waiting"),
        ("ok", "done"),
        ("bad", "failed"),
    ];

    #[tokio::test]
    async fn the_page_lists_its_sections_in_order_and_numbers_what_waits() {
        let mut app = test_app();
        bot_with(&mut app, &BOX);
        details(&mut app);
        let (work, max_threads) = app.box_of("a");
        let listed = |max_threads| {
            let sections = page(&app).sections(work, max_threads);
            sections
                .iter()
                .map(|s| {
                    let rows: Vec<String> = s
                        .rows
                        .iter()
                        .map(|row| match row {
                            DetailRow::Waiting(place, task) => format!("{place}:{}", task.id),
                            row => row.id().to_owned(),
                        })
                        .collect();
                    format!("{} [{}] {} +{}", s.title, s.count, rows.join(" "), s.free)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            listed(max_threads),
            [
                "Threads [3 / 3] n w1 w2 +0",
                "Event box [2] 1:q1 2:q2 +0",
                "Later [] f +0",
                "Routines [] r +0",
                "Recent [] ok bad +0"
            ],
            "what needs you first; no maxThreads from the host counts 3"
        );
        assert_eq!(listed(Some(5))[0], "Threads [3 / 5] n w1 w2 +2", "a free row per thread not in use");
        assert_eq!(listed(Some(1))[0], "Threads [3 / 1] n w1 w2 +0", "right after the ceiling was lowered");

        // An idle bot has no row of free threads, and nothing of the box.
        bot_with(&mut app, &[("ok", "done")]);
        let (work, max_threads) = app.box_of("a");
        let titles: Vec<&str> = page(&app).sections(work, max_threads).iter().map(|s| s.title).collect();
        assert_eq!(titles, ["Later", "Routines", "Recent"]);
        // Only waiting: the threads show their free rows over the box.
        bot_with(&mut app, &[("q1", "waiting")]);
        let (work, max_threads) = app.box_of("a");
        let sections = page(&app).sections(work, max_threads);
        assert_eq!((sections[0].title, sections[0].count.as_str(), sections[0].free), ("Threads", "0 / 3", 3));
        assert_eq!(sections[1].title, "Event box");
    }

    #[test]
    fn a_thread_waiting_on_the_user_gives_its_seat_up() {
        let page = DetailsPage::default();
        let task = |id: &str, status: &str, seat: Option<bool>| json!({"id": id, "title": "Fix login", "status": status, "thread": format!("thread-{id}"), "seat": seat});
        let work = WorkTask::list(&json!([
            task("w", "working", Some(true)),
            task("n", "needsYou", Some(false)),
            task("q", "waiting", None)
        ]));
        let threads = |work: &[WorkTask]| {
            let sections = page.sections(work, Some(2));
            (sections[0].count.clone(), sections[0].free, can_cut_in(work, Some(2)))
        };
        assert_eq!(threads(&work), ("1 / 2".into(), 1, false), "a free seat: nothing has to give way");
        let older: Vec<WorkTask> = work.into_iter().map(|i| WorkTask { seat: None, ..i }).collect();
        assert_eq!(threads(&older), ("2 / 2".into(), 0, true), "an older host's: every live task takes one");
    }

    #[test]
    fn each_row_offers_its_own_keys() {
        use WorkAct::{CutIn, Drop, First, Open, Retry, Stop};
        let task = |status: &str, thread: bool| {
            let v = json!({"id": "i", "title": "Fix login", "status": status, "thread": thread.then_some("thread-1")});
            WorkTask::list(&json!([v])).remove(0)
        };
        let (working, asks, waiting) = (task("working", true), task("needsYou", true), task("waiting", false));
        let (done, failed, stopped) = (task("done", true), task("failed", false), task("stopped", true));
        let acts = |row: &DetailRow<'_>, full: bool| {
            let keys = [KeyCode::Enter, KeyCode::Char('X'), KeyCode::Char('F'), KeyCode::Char('C')];
            let keys = keys.into_iter().chain([KeyCode::Char('D'), KeyCode::Char('A'), KeyCode::Char('x')]);
            keys.filter_map(|key| WorkAct::of(key, row, full)).collect::<Vec<_>>()
        };
        assert_eq!(acts(&DetailRow::Thread(&working), true), [Open, Stop]);
        assert_eq!(acts(&DetailRow::Thread(&asks), false), [Open, Stop]);
        assert_eq!(acts(&DetailRow::Waiting(1, &waiting), true), [CutIn, Drop], "first in line, every thread busy");
        assert_eq!(acts(&DetailRow::Waiting(1, &waiting), false), [Drop], "a thread is free: it starts by itself");
        assert_eq!(acts(&DetailRow::Waiting(2, &waiting), true), [First, Drop], "only the first cuts in");
        assert_eq!(acts(&DetailRow::Recent(&done), true), [Open, Drop]);
        assert_eq!(acts(&DetailRow::Recent(&failed), true), [Drop, Retry], "no thread took it: nothing to open");
        assert_eq!(acts(&DetailRow::Recent(&stopped), true), [Open, Drop, Retry]);
        // Another bot's request isn't tried again (that bot has its answer); a routine's run is.
        let asked = WorkTask { source: Some(Source::Request(None)), ..failed.clone() };
        let run = WorkTask { source: Some(Source::Routine), ..failed.clone() };
        assert_eq!(acts(&DetailRow::Recent(&asked), true), [Drop], "A isn't offered on a request");
        assert_eq!(acts(&DetailRow::Recent(&run), true), [Drop, Retry]);
        assert_eq!(WorkAct::keys(&DetailRow::Recent(&asked), true), ["D Drop"]);
        // Only work the bot added gives way to a task that cuts in.
        let live = [
            WorkTask { id: "w".into(), ..working.clone() },
            run_of(&working),
            WorkTask { id: "n".into(), ..asks.clone() },
        ];
        assert_eq!(giving_way(&live).iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["n", "w"]);
        let routine = json!({"id": "r"});
        assert_eq!(acts(&DetailRow::Routine(&routine), true), [], "a routine's keys are its own");
        assert_eq!(acts(&DetailRow::FollowUp(&routine), true), []);

        assert_eq!(WorkAct::keys(&DetailRow::Waiting(2, &waiting), true), ["F First", "D Drop"]);
        assert_eq!(WorkAct::keys(&DetailRow::Thread(&working), true), ["↵ opens", "X stops"]);
        assert_eq!(WorkAct::keys(&DetailRow::Recent(&failed), true), ["D Drop", "A Try again"]);
        assert_eq!(WorkAct::keys(&DetailRow::Routine(&routine), true), [""; 0]);
        let methods = [First, Drop, Retry, Open, Stop, CutIn].map(WorkAct::method);
        assert_eq!(methods, [Some("workFirst"), Some("dropWork"), Some("retryWork"), None, None, None]);
        assert_eq!(cut_in_body("a", "q1", Some("w2")), json!({"botId": "a", "taskId": "q1", "insteadOf": "w2"}));
        assert_eq!(cut_in_body("a", "q1", None), json!({"botId": "a", "taskId": "q1"}), "the host picks");
    }

    #[tokio::test]
    async fn the_keys_open_ask_and_steer_from_the_picked_row() {
        let mut app = test_app();
        bot_with(&mut app, &BOX);
        details(&mut app);
        assert_eq!(on_row(&app), "n", "what needs you is first");

        // A working row: its transcript, and X asks before it stops.
        pick(&mut app, "w2");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(app.overlays.last(), Some(Overlay::Thread(s)) if s.thread == "thread-w2" && s.task == "w2"));
        key(&mut app, 'X');
        let asked = |app: &App| match app.overlays.last() {
            Some(Overlay::Confirm(Confirm { act: ConfirmAct::StopWork { bot, task }, .. })) => format!("{bot} {task}"),
            _ => String::new(),
        };
        assert_eq!(asked(&app), "a w2", "from the transcript: that task");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(matches!(app.overlays.last(), Some(Overlay::Thread(_))), "cancelled: the transcript stays");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        key(&mut app, 'X');
        assert_eq!(asked(&app), "a w2", "from its row");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(hint(&mut app), "Stopping…");
        assert_eq!(on_row(&app), "w2", "back on the page");

        // The first in line cuts in: it asks which thread gives way, what needs you first.
        pick(&mut app, "q2");
        key(&mut app, 'C');
        assert!(matches!(app.overlays.last(), Some(Overlay::Details(_))), "only the first in line cuts in");
        pick(&mut app, "q1");
        key(&mut app, 'C');
        let choice = |app: &App| match app.overlays.last() {
            Some(Overlay::CutIn(c)) => Some((c.task.clone(), c.cursor)),
            _ => None,
        };
        assert_eq!(choice(&app), Some(("q1".into(), 0)));
        for _ in 0..5 {
            key(&mut app, 'j');
        }
        assert_eq!(choice(&app), Some(("q1".into(), 2)), "one row per thread");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(on_row(&app), "q1", "cancelled: nothing gives way");
        key(&mut app, 'C');
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(on_row(&app), "q1", "chosen: the page again");

        // First, Drop and Try again leave the page open; a routine's row keeps its own keys.
        for (row, keys) in [("q2", "FD"), ("bad", "AD"), ("ok", "D")] {
            pick(&mut app, row);
            keys.chars().for_each(|c| key(&mut app, c));
            assert_eq!(on_row(&app), row);
        }
        pick(&mut app, "r");
        key(&mut app, 'D');
        key(&mut app, 'x');
        assert!(matches!(
            app.overlays.last(),
            Some(Overlay::Confirm(Confirm { act: ConfirmAct::DeleteRoutine(..), .. }))
        ));
        app.on_key(KeyEvent::from(KeyCode::Esc));

        // With a thread free nothing has to give way: C asks nothing.
        bot_with(&mut app, &[("w1", "working"), ("q1", "waiting")]);
        pick(&mut app, "q1");
        key(&mut app, 'C');
        assert!(matches!(app.overlays.last(), Some(Overlay::Details(_))));

        // What needs you opens the chat, where its card waits.
        bot_with(&mut app, &BOX);
        pick(&mut app, "n");
        app.chat_scroll = 7;
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.overlays.is_empty());
        assert_eq!((app.focus, app.chat_page, app.chat_scroll), (Focus::Chat, true, 0));
    }

    #[tokio::test]
    async fn work_that_needs_you_opens_at_its_own_card_and_its_keys_answer_it() {
        let mut app = test_app();
        bot_with(&mut app, &BOX);
        let allow = json!([{"optionId": "ok", "name": "Allow", "kind": "allow_once"}]);
        // Its card, and a newer one of other work below it.
        for (id, seq, task) in [("its", 1, "n"), ("newer", 2, "w1")] {
            let data = json!({"status": "pending", "task": task, "title": "Edit", "options": allow});
            let entry = json!({"id": id, "botId": "a", "seq": seq, "kind": "permission", "turn": seq, "data": data});
            app.on_msg(Msg::Event(json!({"type": "entry", "entry": entry})));
        }
        details(&mut app);
        pick(&mut app, "n");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.pick.as_deref(), Some("its"));
        key(&mut app, 'y');
        assert!(app.answering.contains_key("its") && !app.answering.contains_key("newer"));
    }

    #[tokio::test]
    async fn the_cursor_stays_on_its_row_while_the_box_changes() {
        let mut app = test_app();
        bot_with(&mut app, &[("w1", "working"), ("q1", "waiting")]);
        details(&mut app);
        pick(&mut app, "r");
        bot_with(&mut app, &BOX);
        assert_eq!(on_row(&app), "r", "rows came in above it");
        pick(&mut app, "q1");
        bot_with(&mut app, &[("q1", "working"), ("w1", "done")]);
        assert_eq!(on_row(&app), "q1", "it began: now a thread");
        bot_with(&mut app, &[]);
        assert_eq!(on_row(&app), "f", "it is gone: a row that exists");
    }

    #[tokio::test]
    async fn b_opens_the_details_and_x_stops_the_one_task_a_thread_has() {
        let mut app = test_app();
        bot_with(&mut app, &[]);
        key(&mut app, 'X');
        assert_eq!(hint(&mut app), "No work");
        assert!(app.overlays.is_empty(), "nothing to stop, nothing asked");
        key(&mut app, 'B');
        assert!(matches!(app.overlays.pop(), Some(Overlay::Details(l)) if l.bot == "a"), "B is the bot's Details");

        bot_with(&mut app, &[("w1", "working"), ("q1", "waiting")]);
        // Typing: the letters are the message's.
        app.typing = true;
        key(&mut app, 'B');
        key(&mut app, 'X');
        assert!(app.overlays.is_empty());
        assert_eq!(app.drafts["a"].text, "BX");
        app.typing = false;

        key(&mut app, 'X');
        assert!(
            matches!(app.overlays.pop(), Some(Overlay::Confirm(Confirm { act: ConfirmAct::StopWork { task, .. }, .. })) if task == "w1"),
            "one task a thread has: asked once"
        );
        bot_with(&mut app, &[("w1", "working"), ("w2", "working")]);
        key(&mut app, 'X');
        assert!(matches!(app.overlays.pop(), Some(Overlay::Details(_))), "several: the page lists them");
        bot_with(&mut app, &[("q1", "waiting")]);
        key(&mut app, 'X');
        assert!(matches!(app.overlays.pop(), Some(Overlay::Details(_))), "only waiting: the page");
    }
}
