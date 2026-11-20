//! A task's thread, opened from its row on the Details page: its thread's own transcript (everything the thread did,
//! whatever session it was in), a line for each thing the thread put in the bot's chat, which `↵` shows there, and its
//! composer: the user writes to the thread of the bot's own task as to a chat.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::super::app::{App, Editor, Entry, Focus, Overlay, Queued, WorkTask, shared_line};
use super::scroll::up_from_end;
use super::{Reply, ctrl};

/// Each thread's transcript by thread id, apart from every chat: its entries by `seq`, with the `rev` they came at.
#[derive(Default)]
pub struct ThreadEntries(HashMap<String, BTreeMap<i64, (i64, Entry)>>);

impl ThreadEntries {
    /// Keeps an entry of `thread` (its own bot id when the event names none) once: the higher `rev` wins.
    pub(in crate::tui) fn keep(&mut self, thread: Option<&str>, entry: &Value) {
        let Some((owner, e)) = Entry::parse(entry) else { return };
        let rev = entry["rev"].as_i64().unwrap_or(0);
        let held = self.0.entry(thread.map_or(owner, str::to_owned)).or_default();
        if held.get(&e.seq).is_none_or(|(known, _)| *known <= rev) {
            held.insert(e.seq, (rev, e));
        }
    }

    /// `thread`'s entries, oldest first, without a message the user took back.
    pub fn of(&self, thread: &str) -> impl Iterator<Item = &Entry> {
        let held = self.0.get(thread).into_iter().flat_map(|held| held.values().map(|(_, e)| e));
        held.filter(|e| e.data["removed"] != true)
    }

    /// The user's messages that wait for their turn in `thread`'s thread, oldest (the next to go) first. What the
    /// host wrote there (a task, as the thread is told it) is no message of theirs.
    pub fn queued(&self, thread: &str) -> Vec<&Entry> {
        self.of(thread).filter(|e| e.queued() && e.data["from"] != "host").collect()
    }

    fn forget(&mut self, thread: &str) {
        self.0.remove(thread);
    }
}

/// How the transcript `threadEntries` answers with stands.
pub enum Fetched {
    Loading,
    Failed(String),
    Loaded,
}

/// The transcript of the thread that has or had a task, scrolled from the bottom.
pub struct ThreadSheet {
    pub bot: String,
    /// The task its heading tells of.
    pub task: String,
    pub thread: String,
    pub fetched: Fetched,
    /// Tool calls show their whole diff and output (`T`).
    pub full: bool,
    /// Lines up from the bottom; the draw keeps the picked line on screen.
    pub scroll: Cell<usize>,
    /// Set by the last draw, for clamping the scroll and paging.
    pub max_scroll: Cell<usize>,
    pub page: Cell<usize>,
    /// What the user writes to the thread.
    pub draft: Editor,
    /// Keys go to the draft (`i` or `↵`) until esc.
    pub typing: bool,
    /// The message on its way to the host: the draft stays until it is delivered.
    pub sending: Option<String>,
    /// The line of what the thread put in the bot's chat that `↵` shows there (`tab` picks one): that entry's id.
    pub pick: Option<String>,
}

impl ThreadSheet {
    pub fn new(bot: String, task: String, thread: String) -> Self {
        Self {
            bot,
            task,
            thread,
            fetched: Fetched::Loading,
            full: false,
            scroll: Cell::default(),
            max_scroll: Cell::default(),
            page: Cell::default(),
            draft: Editor::default(),
            typing: false,
            sending: None,
            pick: None,
        }
    }

    /// Its task as the bot's box has it; none once it was dropped.
    pub fn task<'a>(&self, app: &'a App) -> Option<&'a WorkTask> {
        app.bots.get(&self.bot)?.work.iter().find(|i| i.id == self.task)
    }

    /// `tab` (`up`) or `⇧tab`: the pick moves to the next line of what the thread put in the chat, up or down; the
    /// first press picks the newest.
    pub(in crate::tui) fn step_pick(&mut self, app: &App, up: bool) {
        let lines: Vec<&str> = app.shared(&self.bot, &self.thread).into_iter().map(|e| e.id.as_str()).collect();
        let at = self.pick.as_deref().and_then(|p| lines.iter().position(|id| *id == p));
        let next = match at {
            Some(at) if up => at.saturating_sub(1),
            Some(at) => (at + 1).min(lines.len().saturating_sub(1)),
            None => lines.len().saturating_sub(1),
        };
        self.pick = lines.get(next).map(|id| (*id).to_owned());
    }
}

/// Whether one of `work`'s live tasks is `thread`'s: its entries still come.
fn works(work: &[WorkTask], thread: &str) -> bool {
    work.iter().any(|i| i.status.live() && i.thread.as_deref() == Some(thread))
}

impl App {
    /// What `thread` put in `bot`'s chat that its thread has a line for (`shared_line`), oldest first.
    pub fn shared(&self, bot: &str, thread: &str) -> Vec<&Entry> {
        self.lane(bot).into_iter().filter(|e| e.data["thread"]["id"] == thread && shared_line(e).is_some()).collect()
    }

    /// Opens the transcript of the thread that has or had `task`; a task no thread took has none.
    pub(in crate::tui) fn open_thread(&mut self, bot: &str, task: &WorkTask) {
        let Some(thread) = task.thread.clone() else { return };
        self.fetch_thread(bot, &thread);
        self.overlays.push(Overlay::Thread(ThreadSheet::new(bot.to_owned(), task.id.clone(), thread)));
    }

    fn fetch_thread(&self, bot: &str, thread: &str) {
        self.sheet("threadEntries", json!({"botId": bot, "threadId": thread}), Reply::ThreadEntries(thread.to_owned()));
    }

    /// The open transcripts load again: a new connection brings none of a thread's entries.
    pub(in crate::tui) fn reload_thread_details(&self) {
        for o in &self.overlays {
            if let Overlay::Thread(s) = o {
                self.fetch_thread(&s.bot, &s.thread);
            }
        }
    }

    /// `threadEntries` answered for `thread`: its entries join what the stream brought. A reload that failed leaves what
    /// the sheet shows.
    pub(in crate::tui) fn thread_fetched(&mut self, thread: &str, answer: Result<&Value, String>) {
        let loaded = answer.map(|v| {
            for entry in v["entries"].as_array().into_iter().flatten() {
                self.thread_entries.keep(Some(thread), entry);
            }
        });
        for o in &mut self.overlays {
            if let Overlay::Thread(s) = o
                && s.thread == thread
            {
                match &loaded {
                    Ok(()) => s.fetched = Fetched::Loaded,
                    Err(e) if matches!(s.fetched, Fetched::Loading) => s.fetched = Fetched::Failed(e.clone()),
                    Err(_) => {}
                }
            }
        }
    }

    /// A bot's box changed from `before` to `now`: the transcript of a thread that no longer works goes, unless it is
    /// being read.
    pub(in crate::tui) fn threads_ended(&mut self, before: &[WorkTask], now: &[WorkTask]) {
        for thread in before.iter().filter(|i| i.status.live()).filter_map(|i| i.thread.as_deref()) {
            let open = self.overlays.iter().any(|o| matches!(o, Overlay::Thread(s) if s.thread == thread));
            if !open && !works(now, thread) {
                self.thread_entries.forget(thread);
            }
        }
    }

    /// `bot`'s box changed: an open transcript whose task another thread took since (tried again, or replaced)
    /// follows that one, so it reads the same attempt its heading and Stop are about.
    pub(in crate::tui) fn follow_threads(&mut self, bot: &str) {
        let Some(work) = self.bots.get(bot).map(|b| b.work.clone()) else { return };
        let mut moved = Vec::new();
        for o in &mut self.overlays {
            if let Overlay::Thread(s) = o
                && s.bot == bot
                && let Some(thread) = work.iter().find(|i| i.id == s.task).and_then(|i| i.thread.clone())
                && thread != s.thread
            {
                moved.push((std::mem::replace(&mut s.thread, thread), s.thread.clone()));
                (s.fetched, s.pick) = (Fetched::Loading, None);
                s.scroll.set(0);
            }
        }
        for (old, new) in moved {
            self.fetch_thread(bot, &new);
            if !works(&work, &old) {
                self.thread_entries.forget(&old);
            }
        }
    }

    /// `send` for `task`'s thread answered: a delivered message leaves the draft, a refused one says why.
    pub(in crate::tui) fn thread_sent(&mut self, task: &str, answer: Result<(), String>) {
        for o in &mut self.overlays {
            if let Overlay::Thread(s) = o
                && s.task == task
                && let Some(sent) = s.sending.take()
                && answer.is_ok()
                && s.draft.text.trim() == sent
            {
                s.draft.clear();
            }
        }
        if let Err(error) = answer {
            self.flash(&error);
        }
    }

    /// ↵ in the thread's composer: the draft goes to the task's thread. It waits there behind a running turn,
    /// and reopens a task that ended.
    fn send_to_thread(&mut self, s: &mut ThreadSheet) {
        let text = s.draft.text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        if s.sending.is_some() {
            self.flash("Sending…");
            return;
        }
        // ponytail: a new nonce each time, so a send whose answer was lost and is typed again goes twice; keep
        // the nonce with the draft as the chat does if that shows.
        let nonce = uuid::Uuid::new_v4().to_string();
        let body = json!({"botId": s.bot, "taskId": s.task, "text": text, "clientNonce": nonce});
        self.sheet("send", body, Reply::ThreadSent(s.task.clone()));
        s.sending = Some(text);
        s.scroll.set(0);
    }

    /// `S` and `D` on the thread's waiting messages, as in a chat: the next one goes now, the last is taken back.
    fn on_thread_queued(&mut self, s: &ThreadSheet, act: Queued) {
        let queued = self.thread_entries.queued(&s.thread);
        let picked = match act {
            Queued::SendNow => queued.first(),
            Queued::Remove => queued.last(),
        };
        let id = picked.map(|e| e.id.clone());
        self.on_queued(act, json!({"botId": s.bot, "taskId": s.task}), id);
    }

    /// The thread closes: nothing brings an ended thread's entries any more.
    fn leave_thread(&mut self, s: &ThreadSheet) {
        if !self.bots.get(&s.bot).is_some_and(|b| works(&b.work, &s.thread)) {
            self.thread_entries.forget(&s.thread);
        }
    }

    /// `↵` on a picked line: the thread and what is under it close for the bot's chat at that entry. A card still
    /// waiting is at the chat's end, where the chat's keys answer it; anything else is picked, which brings it on
    /// screen.
    fn show_in_chat(&mut self, s: &ThreadSheet, entry: &str) {
        let pending = self.lane(&s.bot).into_iter().find(|e| e.id == entry).is_none_or(Entry::pending);
        self.leave_thread(s);
        self.overlays.clear();
        self.select(&s.bot);
        self.chat_page = true;
        self.focus = Focus::Chat;
        self.chat_scroll = 0;
        self.pick = (!pending).then(|| entry.to_owned());
    }

    /// A key while the thread's composer has the keys.
    fn thread_typing(&mut self, s: &mut ThreadSheet, k: KeyEvent) {
        let newline = k.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT);
        match k.code {
            KeyCode::Esc => s.typing = false,
            KeyCode::Enter if newline => s.draft.insert("\n"),
            KeyCode::Char('j') if ctrl(k) => s.draft.insert("\n"),
            KeyCode::Enter => self.send_to_thread(s),
            _ => {
                s.draft.key(k);
            }
        }
    }

    pub(in crate::tui) fn thread_key(&mut self, mut s: ThreadSheet, k: KeyEvent) -> Option<Overlay> {
        let task = s.task(self).cloned();
        // A task that stopped taking messages (it was dropped, or started again elsewhere) takes no more keys.
        let talks = task.as_ref().is_some_and(|i| i.talk);
        s.typing &= talks;
        if s.typing {
            self.thread_typing(&mut s, k);
            return Some(Overlay::Thread(s));
        }
        let live = task.filter(|i| i.status.live());
        match k.code {
            KeyCode::Tab | KeyCode::BackTab => s.step_pick(self, k.code == KeyCode::Tab),
            KeyCode::Enter if s.pick.is_some() => {
                if let Some(entry) = s.pick.clone() {
                    self.show_in_chat(&s, &entry);
                }
                return None;
            }
            KeyCode::Esc if s.pick.is_some() => s.pick = None,
            KeyCode::Char('i') | KeyCode::Enter if talks => s.typing = true,
            KeyCode::Char('S') => self.on_thread_queued(&s, Queued::SendNow),
            KeyCode::Char('D') => self.on_thread_queued(&s, Queued::Remove),
            KeyCode::Esc | KeyCode::Char('q') => {
                self.leave_thread(&s);
                return None;
            }
            KeyCode::Char('T') => s.full = !s.full,
            KeyCode::Char('X') => {
                if let Some(task) = live {
                    let bot = s.bot.clone();
                    self.overlays.push(Overlay::Thread(s));
                    self.confirm_stop(bot, &task);
                    return None;
                }
            }
            KeyCode::Char('r') if matches!(s.fetched, Fetched::Failed(_)) => {
                s.fetched = Fetched::Loading;
                self.fetch_thread(&s.bot, &s.thread);
            }
            _ => {
                let mut lines = s.scroll.get();
                up_from_end(&mut lines, s.max_scroll.get(), s.page.get(), k);
                s.scroll.set(lines);
            }
        }
        Some(Overlay::Thread(s))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::{Msg, test_app};
    use super::super::work::tests::{bot_with, key};
    use super::*;

    fn thread_entry(seq: i64, rev: i64, text: &str) -> Value {
        json!({"id": format!("t{seq}"), "botId": "thread-1", "seq": seq, "rev": rev, "kind": "agent", "turn": 1,
            "data": {"text": text}})
    }

    fn texts(app: &App) -> Vec<&str> {
        app.thread_entries.of("thread-1").map(Entry::text).collect()
    }

    #[tokio::test]
    async fn a_threads_entries_stay_out_of_the_chat_and_the_higher_rev_wins() {
        let mut app = test_app();
        bot_with(&mut app, &[("1", "working")]);
        for (seq, rev, text) in [(1, 5, "reading"), (2, 6, "half"), (2, 8, "whole")] {
            app.on_msg(Msg::Event(json!({"type": "threadEntry", "rev": rev, "thread": "thread-1",
                "entry": thread_entry(seq, rev, text)})));
        }
        assert!(app.entries.is_empty(), "no chat holds them");
        assert!(app.lane("a").is_empty() && app.lane("thread-1").is_empty());
        assert_eq!(texts(&app), ["reading", "whole"]);

        // What `threadEntries` answers joins them: an older copy of a known entry loses, the rest are new. Only its
        // `entries` are read.
        let fetched = json!({"thread": {"id": "thread-1", "title": "Another", "status": "later"},
            "entries": [thread_entry(0, 2, "told"), thread_entry(2, 7, "stale")]});
        app.thread_fetched("thread-1", Ok(&fetched));
        assert_eq!(texts(&app), ["told", "reading", "whole"]);
    }

    #[tokio::test]
    async fn the_transcript_stays_while_it_is_read_and_says_when_it_cannot_load() {
        let mut app = test_app();
        bot_with(&mut app, &[("1", "working"), ("2", "working")]);
        for thread in ["thread-1", "thread-2"] {
            let mut entry = thread_entry(1, 1, "step");
            entry["botId"] = thread.into();
            app.on_msg(Msg::Event(json!({"type": "threadEntry", "rev": 1, "thread": thread, "entry": entry})));
        }
        let first = app.bots["a"].work[0].clone();
        app.open_thread("a", &first);
        let fetched = |app: &App| match app.overlays.last() {
            Some(Overlay::Thread(ThreadSheet { fetched: Fetched::Loading, .. })) => "loading",
            Some(Overlay::Thread(ThreadSheet { fetched: Fetched::Failed(_), .. })) => "failed",
            Some(Overlay::Thread(ThreadSheet { fetched: Fetched::Loaded, .. })) => "loaded",
            _ => "closed",
        };
        app.thread_fetched("thread-1", Err("the computer is unreachable".into()));
        assert_eq!(fetched(&app), "failed");
        key(&mut app, 'r');
        assert_eq!(fetched(&app), "loading", "r tries again");
        app.thread_fetched("thread-1", Ok(&json!({"entries": []})));
        app.thread_fetched("thread-1", Err("offline".into()));
        assert_eq!(fetched(&app), "loaded", "a failed reload keeps what it shows");

        // Both end: the one being read keeps its transcript, and its heading reads the box.
        bot_with(&mut app, &[("1", "done"), ("2", "stopped")]);
        assert_eq!(app.thread_entries.of("thread-2").count(), 0);
        assert_eq!(app.thread_entries.of("thread-1").count(), 1);
        key(&mut app, 'X');
        assert!(matches!(app.overlays.last(), Some(Overlay::Thread(_))), "nothing left to stop");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.thread_entries.of("thread-1").count(), 0, "closed: its transcript goes");

        // A waiting task no thread took has no transcript to open.
        bot_with(&mut app, &[("3", "waiting")]);
        let waiting = WorkTask { thread: None, ..app.bots["a"].work[0].clone() };
        app.open_thread("a", &waiting);
        assert!(app.overlays.is_empty());
    }

    /// The open transcript's sheet.
    fn sheet(app: &App) -> &ThreadSheet {
        match app.overlays.last() {
            Some(Overlay::Thread(s)) => s,
            _ => panic!("no transcript is open"),
        }
    }

    #[tokio::test]
    async fn the_thread_takes_what_the_user_types_and_the_chats_keys_for_what_waits() {
        let mut app = test_app();
        let work = json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true},
            {"id": "r", "title": "Digest", "status": "working", "thread": "thread-2", "source": {"kind": "routine"}}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        app.host_version = "2.49.1".into();
        let hint = |app: &mut App| app.hint.take().map(|(text, _)| text).unwrap_or_default();
        let own = app.bots["a"].work[0].clone();
        app.open_thread("a", &own);

        // `i` gives the keys to the composer; ↵ sends, and the draft stays until the host has the message.
        key(&mut app, 'i');
        for c in "also q".chars() {
            key(&mut app, c);
        }
        assert!(sheet(&app).typing && sheet(&app).draft.text == "also q", "q types, it doesn't close");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(sheet(&app).sending.as_deref(), Some("also q"));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(hint(&mut app), "Sending…", "one at a time");
        app.thread_sent("i", Err("\"Fix login\" hasn't started yet".into()));
        assert_eq!(hint(&mut app), "\"Fix login\" hasn't started yet");
        assert_eq!(sheet(&app).draft.text, "also q", "refused: the words stay");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        app.thread_sent("i", Ok(()));
        assert!(sheet(&app).draft.text.is_empty() && sheet(&app).sending.is_none());

        // What waits in the thread: `S` sends the next one now, `D` takes back the last, as in a chat.
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(
            !sheet(&app).typing && matches!(app.overlays.last(), Some(Overlay::Thread(_))),
            "esc leaves the composer"
        );
        key(&mut app, 'S');
        assert_eq!(hint(&mut app), "No message waiting");
        let told = json!({"id": "t1", "botId": "thread-1", "seq": 1, "rev": 1, "kind": "user", "turn": 1,
            "data": {"text": "[Task] Fix login", "from": "host", "status": "queued"}});
        let waits = json!({"id": "t2", "botId": "thread-1", "seq": 2, "rev": 2, "kind": "user", "turn": 2,
            "data": {"text": "also q", "status": "queued"}});
        for entry in [&told, &waits] {
            app.on_msg(Msg::Event(
                json!({"type": "threadEntry", "rev": entry["rev"], "thread": "thread-1", "entry": entry}),
            ));
        }
        let queued: Vec<&str> = app.thread_entries.queued("thread-1").iter().map(|e| e.id.as_str()).collect();
        assert_eq!(queued, ["t2"], "what the host told the thread is no message of the user's");
        key(&mut app, 'S');
        assert_eq!(hint(&mut app), "Sending now…");
        key(&mut app, 'D');
        assert_eq!(hint(&mut app), "Removed");
        let mut gone = waits.clone();
        gone["rev"] = 3.into();
        gone["data"]["removed"] = true.into();
        app.on_msg(Msg::Event(json!({"type": "threadEntry", "rev": 3, "thread": "thread-1", "entry": gone})));
        assert_eq!(app.thread_entries.of("thread-1").count(), 1, "a message taken back leaves the transcript");

        // A routine's run is read, not written to.
        app.overlays.clear();
        let run = app.bots["a"].work[1].clone();
        app.open_thread("a", &run);
        key(&mut app, 'i');
        assert!(!sheet(&app).typing);
    }

    #[tokio::test]
    async fn a_paste_goes_to_the_thread_being_written() {
        let mut app = test_app();
        let work = json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        app.selected = Some("a".into());
        let own = app.bots["a"].work[0].clone();
        app.open_thread("a", &own);
        key(&mut app, 'i');
        app.on_paste("run the tests");
        assert_eq!(sheet(&app).draft.text, "run the tests");
        assert!(app.draft().text.is_empty(), "not the chat's");
    }

    #[tokio::test]
    async fn the_transcript_follows_the_thread_that_took_its_task_since() {
        let mut app = test_app();
        let work = |thread: &str| json!([{"id": "i", "title": "Fix login", "status": "working", "thread": thread}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work("thread-1")}})));
        let own = app.bots["a"].work[0].clone();
        app.open_thread("a", &own);
        app.thread_fetched("thread-1", Ok(&json!({"entries": []})));
        // Tried again elsewhere, or replaced: a new thread has the task now.
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work("thread-2")}})));
        assert_eq!(sheet(&app).thread, "thread-2");
        assert!(matches!(sheet(&app).fetched, Fetched::Loading), "its transcript loads");
    }

    #[tokio::test]
    async fn enter_on_a_picked_line_shows_what_the_thread_put_in_the_chat_there() {
        let mut app = test_app();
        let work = json!([{"id": "i", "title": "Fix login", "status": "needsYou", "thread": "thread-1", "talk": true}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        let thread = json!({"id": "thread-1", "title": "Fix login"});
        let chat = |id: &str, seq: i64, kind: &str, data: Value| {
            let entry = json!({"id": id, "botId": "a", "seq": seq, "kind": kind, "turn": 1, "data": data});
            json!({"type": "entry", "entry": entry})
        };
        let file = json!({"final": true, "thread": thread, "files": [{"id": "f1", "name": "log.txt", "size": 3}]});
        let card = json!({"status": "pending", "title": "Run tests", "thread": thread, "task": "i",
            "options": [{"optionId": "ok", "name": "Allow", "kind": "allow_once"}]});
        app.on_msg(Msg::Event(chat("file", 1, "agent", file)));
        app.on_msg(Msg::Event(chat("card", 2, "permission", card)));
        let own = app.bots["a"].work[0].clone();
        let opened = |app: &mut App| {
            app.overlays.clear();
            app.open_thread("a", &own);
        };

        // The card it waits on: the chat's end, where the chat's keys answer it.
        opened(&mut app);
        app.on_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(sheet(&app).pick.as_deref(), Some("card"), "the newest first");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.overlays.is_empty() && app.focus == Focus::Chat && app.chat_page);
        assert_eq!((app.selected.as_deref(), app.pick.as_deref()), (Some("a"), None));

        // The file: picked in the chat, which brings it on screen.
        opened(&mut app);
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.pick.as_deref(), Some("file"));

        // Esc lets go of the pick first; ↵ then types, as before.
        opened(&mut app);
        app.on_key(KeyEvent::from(KeyCode::BackTab));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(sheet(&app).pick.is_none());
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(sheet(&app).typing);
    }
}
