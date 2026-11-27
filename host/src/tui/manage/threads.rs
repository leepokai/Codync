//! A task's thread, opened from its row on the Details page: its thread's own transcript (everything the thread did,
//! whatever session it was in), a line for each thing the thread put in the bot's chat, which `↵` shows there, and its
//! composer: the user writes to the thread of the bot's own task as to a chat.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use tokio::task::AbortHandle;
use uuid::Uuid;

use super::super::app::{After, App, Editor, Entry, Focus, Kind, Overlay, Queued, WorkTask, shared_line};
use super::scroll::up_from_end;
use super::{Reply, ctrl};

/// Each thread's transcript by thread id, apart from every chat: its entries by `seq`, with the `rev` they came at.
#[derive(Default)]
pub struct ThreadEntries(HashMap<String, BTreeMap<i64, (i64, Entry)>>);

impl ThreadEntries {
    fn keep(&mut self, thread: &str, e: Entry) {
        let rev = e.rev.unwrap_or(0);
        let held = self.0.entry(thread.to_owned()).or_default();
        if held.get(&e.seq).is_none_or(|(known, _)| *known <= rev) {
            held.insert(e.seq, (rev, e));
        }
    }

    fn agrees_with(&self, thread: &str, row: &Entry) -> bool {
        self.0.get(thread).and_then(|rows| rows.get(&row.seq)).is_none_or(|(rev, known)| {
            *rev < row.rev.unwrap_or(0) || (known.id == row.id && known.data["removed"] != true)
        })
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

pub(in crate::tui) struct ThreadSend {
    text: String,
    files: Vec<PathBuf>,
    /// Returned files still at the front of the draft.
    returned_files: usize,
    nonce: String,
    thread: String,
    request: Uuid,
    receipt: Option<(i64, String)>,
    task: Option<AbortHandle>,
}

fn send_row(row: &Entry) -> bool {
    row.kind == Kind::User && !row.id.is_empty() && row.seq > 0 && row.data["removed"] != true
}

impl ThreadSend {
    fn matches(&self, row: &Entry) -> bool {
        let same_row = |(seq, id): &(i64, String)| row.seq == *seq && row.id == *id;
        send_row(row)
            && if row.data["clientNonce"].is_null() {
                self.receipt.as_ref().is_some_and(same_row)
            } else {
                row.data["clientNonce"].as_str() == Some(self.nonce.as_str())
                    && self.receipt.as_ref().is_none_or(same_row)
            }
    }
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
    /// Files dragged onto the terminal while typing, sent with the next message (`⌫` on an empty draft drops the last).
    pub files: Vec<PathBuf>,
    /// The send in flight or available to retry with the same nonce.
    pub(in crate::tui) sending: Option<ThreadSend>,
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
            files: Vec::new(),
            sending: None,
            pick: None,
        }
    }

    /// Its task as the bot's box has it; none once it was dropped.
    pub fn task<'a>(&self, app: &'a App) -> Option<&'a WorkTask> {
        app.bots.get(&self.bot)?.work.iter().find(|i| i.id == self.task)
    }

    fn restore_send_files(&mut self) {
        if let Some(sent) = &mut self.sending
            && let Some(task) = sent.task.take()
        {
            task.abort();
            self.files.splice(0..0, sent.files.iter().cloned());
            sent.returned_files = sent.files.len();
        }
    }

    fn finish_send(&mut self) {
        let Some(sent) = self.sending.take() else { return };
        if let Some(task) = sent.task {
            task.abort();
        }
        self.files.drain(..sent.returned_files);
        if self.draft.text == sent.text {
            self.draft.clear();
        }
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
        if !self.replaying_mirror()
            && let Some(connection) = self.send_receipts.thread_read_connection()
        {
            self.sheet(
                "threadEntries",
                json!({"botId": bot, "threadId": thread}),
                Reply::ThreadEntries {
                    bot: bot.to_owned(),
                    thread: thread.to_owned(),
                    epoch: self.mirror_epoch(),
                    connection,
                },
            );
        }
    }

    pub(in crate::tui) fn rewind_threads(&mut self) {
        self.thread_entries = ThreadEntries::default();
        for overlay in &mut self.overlays {
            if let Overlay::Thread(s) = overlay {
                (s.fetched, s.pick) = (Fetched::Loading, None);
                s.scroll.set(0);
                s.restore_send_files();
                if let Some(sent) = &mut s.sending {
                    sent.receipt = None;
                }
            }
        }
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
    pub(in crate::tui) fn thread_fetched(
        &mut self,
        bot: &str,
        thread: &str,
        connection: u64,
        answer: Result<&Value, &str>,
    ) {
        if !self.thread_open(thread) && !self.bots.get(bot).is_some_and(|b| works(&b.work, thread)) {
            return;
        }
        let rows: Vec<_> = answer
            .ok()
            .and_then(|v| v["entries"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Entry::parse)
            .filter_map(|(owner, row)| (owner == thread).then_some(row))
            .collect();
        let revision = rows.iter().filter_map(|row| row.rev).max().unwrap_or_default();
        if !self.send_receipts.accept_thread_rows(connection, revision) {
            return;
        }
        let confirmed = self.confirmed_thread_connection() == Some(connection);
        for row in rows {
            self.keep_thread_row(thread, row, confirmed);
        }
        for o in &mut self.overlays {
            if let Overlay::Thread(s) = o
                && s.bot == bot
                && s.thread == thread
            {
                match answer {
                    Ok(_) => s.fetched = Fetched::Loaded,
                    Err(e) if matches!(s.fetched, Fetched::Loading) => s.fetched = Fetched::Failed(e.to_owned()),
                    Err(_) => {}
                }
            }
        }
    }

    pub(in crate::tui) fn thread_changed(&mut self, thread: Option<&str>, value: &Value) {
        let Some((owner, row)) = Entry::parse(value) else { return };
        let thread = thread.unwrap_or(&owner);
        let confirmed = owner == thread
            && self
                .confirmed_thread_connection()
                .is_some_and(|connection| self.send_receipts.accept_thread_rows(connection, 0));
        self.keep_thread_row(thread, row, confirmed);
    }

    fn keep_thread_row(&mut self, thread: &str, row: Entry, confirmed: bool) {
        if confirmed && self.thread_entries.agrees_with(thread, &row) {
            for overlay in &mut self.overlays {
                if let Overlay::Thread(s) = overlay
                    && s.sending
                        .as_ref()
                        .is_some_and(|sent| (sent.thread == thread || s.thread == thread) && sent.matches(&row))
                {
                    s.finish_send();
                }
            }
        }
        self.thread_entries.keep(thread, row);
    }

    fn thread_open(&self, thread: &str) -> bool {
        self.overlays.iter().any(|o| matches!(o, Overlay::Thread(s) if s.thread == thread))
    }

    /// A bot's box changed from `before` to `now`: the transcript of a thread that no longer works goes, unless it is
    /// being read.
    pub(in crate::tui) fn threads_ended(&mut self, before: &[WorkTask], now: &[WorkTask]) {
        for thread in before.iter().filter(|i| i.status.live()).filter_map(|i| i.thread.as_deref()) {
            if !self.thread_open(thread) && !works(now, thread) {
                self.thread_entries.forget(thread);
            }
        }
    }

    /// `bot`'s box changed: an open transcript whose task another thread took since (tried again, or replaced)
    /// follows that one, so it reads the same attempt its heading and Stop are about.
    pub(in crate::tui) fn follow_threads(&mut self, bot: &str) {
        let Some(work) = self.bots.get(bot).map(|b| &b.work) else { return };
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
            if !self.thread_open(&old) && !works(work, &old) {
                self.thread_entries.forget(&old);
            }
        }
    }

    pub(in crate::tui) fn thread_sent(
        &mut self,
        request: Uuid,
        connection: Option<u64>,
        answer: Result<&Value, String>,
    ) {
        let Some((index, s, sent)) = self.overlays.iter().enumerate().find_map(|(index, o)| {
            let Overlay::Thread(s) = o else { return None };
            let sent = s.sending.as_ref().filter(|sent| sent.request == request && sent.task.is_some())?;
            Some((index, s, sent))
        }) else {
            return;
        };
        let delivered = answer.as_ref().ok().and_then(|v| Entry::parse(&v["entry"])).filter(|(thread, row)| {
            (thread == &sent.thread || thread == &s.thread)
                && send_row(row)
                && (row.data["clientNonce"].is_null() || row.data["clientNonce"].as_str() == Some(&sent.nonce))
                && self.thread_entries.agrees_with(thread, row)
        });
        let current = self.confirmed_thread_connection();
        let confirmed = delivered.as_ref().is_some_and(|(_, row)| {
            connection.is_some_and(|connection| {
                Some(connection) == current
                    && self.send_receipts.accept_thread_rows(connection, row.rev.unwrap_or_default())
            })
        });
        let Overlay::Thread(s) = &mut self.overlays[index] else { return };
        if confirmed {
            s.finish_send();
            return;
        }
        s.restore_send_files();
        if let Some((thread, row)) = delivered
            && let Some(sent) = &mut s.sending
        {
            sent.thread = thread;
            sent.receipt = Some((row.seq, row.id));
        }
        if let Err(error) = answer {
            self.flash(&error);
        } else if current.is_some() {
            let (bot, thread) = (s.bot.clone(), s.thread.clone());
            self.fetch_thread(&bot, &thread);
        }
    }

    /// ↵ in the thread's composer: the draft goes to the task's thread. It waits there behind a running turn,
    /// and reopens a task that ended.
    fn send_to_thread(&mut self, s: &mut ThreadSheet) {
        let text = s.draft.text.trim();
        if self.replaying_mirror() || (text.is_empty() && s.files.is_empty()) {
            return;
        }
        if s.sending.as_ref().is_some_and(|sent| sent.task.is_some()) {
            self.flash("Sending…");
            return;
        }
        let nonce = s
            .sending
            .as_ref()
            .filter(|sent| sent.text.trim() == text && sent.returned_files == s.files.len() && sent.files == s.files)
            .map_or_else(|| Uuid::new_v4().to_string(), |sent| sent.nonce.clone());
        let body = json!({"botId": s.bot, "taskId": s.task, "text": text, "clientNonce": nonce});
        let files = std::mem::take(&mut s.files);
        let request = Uuid::new_v4();
        let after = After::Sheet(Reply::ThreadSent {
            request,
            epoch: self.mirror_epoch(),
            connection: self.confirmed_thread_connection(),
        });
        let task = self.send_message(body, files.clone(), after);
        s.sending = Some(ThreadSend {
            text: s.draft.text.clone(),
            files,
            returned_files: 0,
            nonce,
            thread: s.thread.clone(),
            request,
            receipt: None,
            task: Some(task),
        });
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
        if !self.thread_open(&s.thread) && !self.bots.get(&s.bot).is_some_and(|b| works(&b.work, &s.thread)) {
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
            KeyCode::Backspace if s.draft.text.is_empty() => {
                s.files.pop();
                if let Some(sent) = &mut s.sending {
                    sent.returned_files = sent.returned_files.min(s.files.len());
                }
            }
            _ => {
                s.draft.key(k);
            }
        }
    }

    pub(in crate::tui) fn thread_key(&mut self, mut s: ThreadSheet, k: KeyEvent) -> Option<Overlay> {
        let task = s.task(self).cloned();
        // A task that stopped taking messages (it was dropped, or started again elsewhere) takes no more keys.
        let talks = task.as_ref().is_some_and(|i| i.talk);
        s.typing &= talks || self.replaying_mirror();
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
    use std::sync::atomic::Ordering;

    use super::super::super::app::{Msg, SendReceipts, next_request, recording_host, test_app};
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
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Ok(fetched)));
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
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Err("the computer is unreachable".into())));
        assert_eq!(fetched(&app), "failed");
        key(&mut app, 'r');
        assert_eq!(fetched(&app), "loading", "r tries again");
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Ok(json!({"entries": []}))));
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Err("offline".into())));
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

    fn read_reply(app: &App, thread: &str) -> After {
        After::Sheet(Reply::ThreadEntries {
            bot: "a".into(),
            thread: thread.into(),
            epoch: app.mirror_epoch(),
            connection: app.send_receipts.thread_read_connection().expect("read connection"),
        })
    }

    fn send_reply(app: &App) -> After {
        let request = sheet(app).sending.as_ref().expect("send").request;
        After::Sheet(Reply::ThreadSent {
            request,
            epoch: app.mirror_epoch(),
            connection: app.confirmed_thread_connection(),
        })
    }

    fn hello(app: &mut App) {
        app.on_msg(Msg::Event(json!({"type": "hello", "rev": 0, "catchUp": 0})));
    }

    fn delivery(app: &App) -> Value {
        let sent = sheet(app).sending.as_ref().expect("send");
        json!({"entry": {"id": "sent", "botId": sent.thread, "seq": 1, "rev": 1, "kind": "user", "turn": 1,
            "data": {"text": sent.text, "clientNonce": sent.nonce}}})
    }

    fn writable_task() -> Msg {
        Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": [
            {"id": "1", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true}
        ]}}))
    }

    fn open_draft(app: &mut App, text: &str, file: PathBuf) {
        app.on_msg(writable_task());
        let mut thread = ThreadSheet::new("a".into(), "1".into(), "thread-1".into());
        thread.typing = true;
        thread.draft = Editor::with(text);
        thread.files.push(file);
        app.overlays.push(Overlay::Thread(thread));
    }

    fn append_file(app: &mut App, file: PathBuf) {
        let Some(Overlay::Thread(thread)) = app.overlays.last_mut() else { panic!("thread is open") };
        thread.files.push(file);
    }

    fn send_across_reconnect(app: &mut App, added: Option<PathBuf>) -> Value {
        let receipts = SendReceipts::default();
        let (connection, _) = receipts.begin_request();
        app.on_msg(Msg::Connected { since: 30, connection, send_receipts: receipts.clone() });
        hello(app);
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let reply = send_reply(app);
        let delivered = delivery(app);
        if let Some(file) = added {
            append_file(app, file);
        }

        receipts.invalidate_connection();
        app.on_msg(Msg::Disconnected { since: 30 });
        let (connection, _) = receipts.begin_request();
        app.on_msg(Msg::Connected { since: 30, connection, send_receipts: receipts });
        app.on_msg(Msg::Reply(reply, Ok(delivered.clone())));
        hello(app);
        delivered
    }

    #[tokio::test]
    async fn thread_history_confirmation_clears_the_sent_text_and_leaves_new_files() {
        let original = PathBuf::from("original.txt");
        let added = PathBuf::from("added.txt");
        for edited in [false, true] {
            let mut app = test_app();
            open_draft(&mut app, "sent text", original.clone());
            let delivered = send_across_reconnect(&mut app, Some(added.clone()));
            assert_eq!(sheet(&app).files, [original.clone(), added.clone()]);
            if edited {
                app.on_paste(" plus edits");
            }
            app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Ok(json!({"entries": [delivered["entry"]]}))));

            let draft = sheet(&app);
            assert!(draft.sending.is_none(), "the current history confirmed delivery");
            assert_eq!(
                (draft.draft.text.as_str(), draft.files.as_slice()),
                (if edited { "sent text plus edits" } else { "" }, std::slice::from_ref(&added)),
            );
        }
    }

    #[tokio::test]
    async fn thread_history_confirmation_keeps_a_file_reattached_at_the_same_path() {
        let mut app = test_app();
        let file = PathBuf::from("same.txt");
        open_draft(&mut app, "", file.clone());
        let delivered = send_across_reconnect(&mut app, None);
        app.on_key(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(sheet(&app).files, [] as [PathBuf; 0]);
        append_file(&mut app, file.clone());
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Ok(json!({"entries": [delivered["entry"]]}))));
        assert_eq!(sheet(&app).files, &[file]);
        assert!(sheet(&app).sending.is_none());
    }

    #[tokio::test]
    async fn an_unconfirmed_thread_send_keeps_all_files_and_can_retry_its_original_draft() {
        let original = PathBuf::from("original.txt");
        let added = PathBuf::from("added.txt");
        for reattached in [false, true] {
            let mut app = test_app();
            open_draft(&mut app, "sent text", original.clone());
            let delivered = send_across_reconnect(&mut app, Some(added.clone()));
            app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Ok(json!({"entries": []}))));
            app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Err("unreachable".into())));
            assert_eq!(sheet(&app).draft.text, "sent text");
            assert_eq!(sheet(&app).files, [original.clone(), added.clone()]);
            assert!(sheet(&app).sending.as_ref().unwrap().task.is_none());

            app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
            app.on_key(KeyEvent::from(KeyCode::Backspace));
            if reattached {
                app.on_key(KeyEvent::from(KeyCode::Backspace));
                append_file(&mut app, original.clone());
            }
            app.on_paste("sent text");
            app.on_key(KeyEvent::from(KeyCode::Enter));
            let retried = sheet(&app).sending.as_ref().unwrap();
            assert_eq!(retried.files, std::slice::from_ref(&original));
            assert!(retried.task.is_some());
            let original_nonce = delivered["entry"]["data"]["clientNonce"].as_str().unwrap();
            assert_eq!(retried.nonce == original_nonce, !reattached, "nonce follows the original attachment");
        }
    }

    #[test]
    fn escape_leaves_the_thread_draft_open_during_recovery() {
        let mut app = test_app();
        let file = PathBuf::from("draft.txt");
        open_draft(&mut app, "in progress", file.clone());
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(matches!(app.overlays.last(), Some(Overlay::Thread(_))), "Esc only leaves the composer");
        assert!(!sheet(&app).typing);
        assert_eq!(sheet(&app).draft.text, "in progress");
        assert_eq!(sheet(&app).files, &[file]);
    }

    #[tokio::test]
    async fn thread_recovery_keeps_q_in_the_composer_and_defers_enter_until_confirmed() {
        let mut app = test_app();
        let file = PathBuf::from("draft.txt");
        open_draft(&mut app, "draft", file.clone());
        let receipts = SendReceipts::default();
        app.on_msg(Msg::Rewind(receipts.clone()));
        key(&mut app, 'q');
        assert!(matches!(app.overlays.last(), Some(Overlay::Thread(_))), "q edits the draft");
        assert!(sheet(&app).typing);
        assert_eq!(sheet(&app).draft.text, "draftq");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(sheet(&app).sending.is_none());

        hello(&mut app);
        app.on_msg(writable_task());
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(sheet(&app).sending.is_none(), "a bot snapshot alone doesn't confirm the replay");
        assert_eq!(sheet(&app).files, std::slice::from_ref(&file));
        app.on_msg(Msg::ReplayComplete);
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let sent = sheet(&app).sending.as_ref().expect("the restored draft sends");
        assert_eq!(sent.text, "draftq");
        assert_eq!(sent.files, &[file]);
        app.on_msg(Msg::Reply(send_reply(&app), Ok(delivery(&app))));
        assert_eq!(sheet(&app).draft.text, "");

        receipts.invalidate_connection();
        app.on_msg(Msg::Disconnected { since: 30 });
        key(&mut app, 'q');
        assert_eq!(sheet(&app).draft.text, "q", "ordinary reconnect still allows editing");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(!sheet(&app).typing);
        key(&mut app, 'q');
        assert!(app.overlays.is_empty(), "q closes a thread outside its composer");
    }

    #[tokio::test]
    async fn a_thread_send_reply_keeps_the_draft_while_a_new_stream_awaits_hello() {
        for reconnecting in [false, true] {
            let mut app = test_app();
            let receipts = SendReceipts::default();
            let (connection, _) = receipts.begin_request();
            app.on_msg(Msg::Connected { since: 0, connection, send_receipts: receipts.clone() });
            hello(&mut app);

            let file = PathBuf::from("draft.txt");
            let mut thread = ThreadSheet::new("a".into(), "1".into(), "thread-1".into());
            thread.draft.insert("keep this draft");
            thread.sending = Some(ThreadSend {
                text: thread.draft.text.clone(),
                files: vec![file.clone()],
                returned_files: 0,
                nonce: "same-nonce".into(),
                thread: thread.thread.clone(),
                request: Uuid::new_v4(),
                receipt: None,
                task: Some(tokio::spawn(std::future::pending::<()>()).abort_handle()),
            });
            app.overlays.push(Overlay::Thread(thread));
            let reply = send_reply(&app);
            let entry = delivery(&app);

            if reconnecting {
                // The stream can start a request before the app consumes the reconnect messages.
                receipts.invalidate_connection();
                receipts.begin_request();
                assert_eq!(receipts.thread_read_connection(), None);
                assert_eq!(app.confirmed_thread_connection(), Some(connection));
            }
            app.on_msg(Msg::Reply(reply, Ok(entry)));

            let thread = sheet(&app);
            if reconnecting {
                assert_eq!(thread.draft.text, "keep this draft");
                assert_eq!(thread.files, std::slice::from_ref(&file));
                let retry = thread.sending.as_ref().expect("the original send is retryable");
                assert_eq!(retry.nonce, "same-nonce");
                assert!(retry.task.is_none());
            } else {
                assert_eq!(thread.draft.text, "");
                assert_eq!(thread.files, [] as [PathBuf; 0]);
                assert!(thread.sending.is_none());
            }
        }
    }

    #[test]
    fn accepted_thread_reads_contribute_to_the_next_stream_request_sample() {
        let mut app = test_app();
        let receipts = SendReceipts::default();
        let since = 30;
        app.client.rev.store(since, Ordering::Relaxed);
        let (connection, _) = receipts.begin_request();
        app.on_msg(Msg::Connected { since, connection, send_receipts: receipts.clone() });
        app.on_msg(Msg::Event(json!({"type": "hello", "rev": since, "catchUp": 0})));
        bot_with(&mut app, &[("1", "working")]);

        receipts.invalidate_connection();
        app.on_msg(Msg::Disconnected { since });
        let offline = receipts.thread_read_connection().expect("read while disconnected");
        let rows = json!({"entries": [thread_entry(1, 31, "edited offline"), thread_entry(2, 32, "added offline")]});
        app.thread_fetched("a", "thread-1", offline, Ok(&rows));
        assert_eq!(texts(&app), ["edited offline", "added offline"]);
        assert_eq!(app.client.rev.load(Ordering::Relaxed), 30);
        let (connection, sampled) = receipts.begin_request();
        assert_eq!(sampled, 32);

        app.on_msg(Msg::Connected { since, connection, send_receipts: receipts.clone() });
        app.client.rev.store(sampled, Ordering::Relaxed);
        app.on_msg(Msg::Event(json!({"type": "hello", "rev": sampled, "catchUp": 0})));
        assert_eq!(receipts.thread_read_connection(), Some(connection));
        let stale = json!({"entries": [thread_entry(3, 90, "delayed old read")]});
        app.thread_fetched("a", "thread-1", offline, Ok(&stale));
        assert_eq!(texts(&app), ["edited offline", "added offline"]);

        let current = json!({"entries": [thread_entry(3, 33, "current read")]});
        app.thread_fetched("a", "thread-1", connection, Ok(&current));
        assert_eq!(texts(&app), ["edited offline", "added offline", "current read"]);
        assert_eq!(app.client.rev.load(Ordering::Relaxed), 32);
        assert_eq!(receipts.begin_request().1, 33);
    }

    #[tokio::test]
    async fn restored_thread_loads_its_backup_and_ignores_an_older_read() {
        let (url, mut requests) = recording_host().await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let client = crate::tui::net::Client::new(&url, Some("test"));
        let mut app = App::new(client, tx, url);
        bot_with(&mut app, &[("1", "working"), ("2", "working")]);
        app.open_thread("a", &app.bots["a"].work[0].clone());
        assert!(next_request(&mut requests).await.contains("threadEntries"));
        let old = read_reply(&app, "thread-1");
        let rows = |rows| Ok(json!({"entries": rows}));
        app.on_msg(Msg::Reply(
            old.clone(),
            rows(vec![thread_entry(1, 29, "edited after backup"), thread_entry(3, 30, "added after backup")]),
        ));
        let mut closed = thread_entry(2, 27, "closed thread");
        closed["botId"] = "thread-2".into();
        app.on_msg(Msg::Reply(read_reply(&app, "thread-2"), rows(vec![closed])));
        assert_eq!(texts(&app), ["edited after backup", "added after backup"]);
        let old_epoch = app.mirror_epoch();
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        assert!(texts(&app).is_empty() && app.thread_entries.of("thread-2").count() == 0);
        bot_with(&mut app, &[("1", "working")]);
        app.on_msg(Msg::Reply(After::Hello, Ok(json!({"version": "2.54.0"}))));
        assert!(next_request(&mut requests).await.contains("/api/track"));
        app.on_msg(Msg::Reply(old.clone(), Err("old read failed".into())));
        assert!(matches!(sheet(&app).fetched, Fetched::Loading));
        app.on_msg(Msg::ReplayComplete);
        assert!(next_request(&mut requests).await.contains("\"threadId\":\"thread-1\""));
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), rows(vec![thread_entry(1, 8, "from backup")])));
        app.on_msg(Msg::Reply(old, rows(vec![thread_entry(1, 29, "old edit"), thread_entry(4, 31, "late read")])));
        app.on_msg(Msg::Reply(After::Queued(old_epoch), Err("old unqueue failed".into())));
        assert!(app.hint.is_none());
        assert_eq!(texts(&app), ["from backup"]);
        let live = thread_entry(1, 13, "live from restored host");
        app.on_msg(Msg::Event(json!({"type": "threadEntry", "thread": "thread-1", "rev": 13, "entry": live})));
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), rows(vec![thread_entry(1, 8, "from backup")])));
        assert_eq!(texts(&app), ["live from restored host"]);
    }

    #[tokio::test]
    async fn a_pending_thread_send_can_be_retried_after_restore_and_keeps_edits() {
        let mut app = test_app();
        let receipts = SendReceipts::default();
        app.on_msg(Msg::Connected { since: 1, connection: 0, send_receipts: receipts.clone() });
        hello(&mut app);
        let box_event = json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": [
            {"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true}
        ]}});
        app.on_msg(Msg::Event(box_event.clone()));
        app.open_thread("a", &app.bots["a"].work[0].clone());
        key(&mut app, 'i');
        app.on_paste("keep my draft");
        let file = PathBuf::from("before.png");
        let Overlay::Thread(s) = app.overlays.last_mut().unwrap() else { panic!("thread is open") };
        s.files.push(file.clone());
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let old_reply = send_reply(&app);
        let old_delivery = delivery(&app);
        let nonce = sheet(&app).sending.as_ref().unwrap().nonce.clone();
        receipts.invalidate_mirror();
        app.on_msg(Msg::Reply(old_reply.clone(), Ok(old_delivery.clone())));
        assert_eq!(sheet(&app).draft.text, "keep my draft");
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        hello(&mut app);
        app.on_msg(Msg::Event(box_event));
        app.on_msg(Msg::ReplayComplete);
        assert_eq!(sheet(&app).draft.text, "keep my draft");
        assert_eq!(sheet(&app).files, std::slice::from_ref(&file));

        app.on_key(KeyEvent::from(KeyCode::Enter));
        let new_reply = send_reply(&app);
        assert_eq!(sheet(&app).sending.as_ref().unwrap().nonce, nonce);
        app.on_msg(Msg::Reply(old_reply.clone(), Ok(old_delivery)));
        app.on_msg(Msg::Reply(old_reply, Err("late refusal".into())));
        assert!(sheet(&app).sending.as_ref().unwrap().task.is_some());
        assert_eq!(sheet(&app).files, [] as [PathBuf; 0]);
        app.on_paste(" with edits");
        let new_file = PathBuf::from("after.png");
        let Overlay::Thread(s) = app.overlays.last_mut().unwrap() else { panic!("thread is open") };
        s.files.push(new_file.clone());
        app.on_msg(Msg::Reply(new_reply, Ok(delivery(&app))));
        assert!(sheet(&app).sending.is_none());
        assert_eq!(sheet(&app).draft.text, "keep my draft with edits");
        assert_eq!(sheet(&app).files, &[new_file]);
    }

    #[tokio::test]
    async fn a_file_dropped_while_writing_to_a_thread_goes_with_the_next_message() {
        let mut app = test_app();
        hello(&mut app);
        let work = json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true,
            "talkFiles": true}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        let own = app.bots["a"].work[0].clone();
        app.open_thread("a", &own);
        key(&mut app, 'i');
        let file = std::env::temp_dir().join(format!("codync-thread-{}.png", Uuid::new_v4()));
        std::fs::write(&file, b"png").expect("written");
        let path = file.display().to_string();
        app.on_paste(&path);
        assert_eq!(sheet(&app).files, std::slice::from_ref(&file), "attached, not typed");
        assert_eq!(sheet(&app).draft.text, "");
        app.on_key(KeyEvent::from(KeyCode::Backspace));
        assert!(sheet(&app).files.is_empty(), "⌫ on an empty draft drops it");
        app.on_paste(&path);
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let sending = |app: &App| sheet(app).sending.as_ref().map(|sent| (sent.text.clone(), sent.files.len()));
        assert_eq!(sending(&app), Some((String::new(), 1)), "a file alone is a message, out of the box");
        app.on_msg(Msg::Reply(send_reply(&app), Err("refused".into())));
        assert_eq!(sheet(&app).files, std::slice::from_ref(&file), "refused: it comes back to the box");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        app.on_paste(&path);
        app.on_msg(Msg::Reply(send_reply(&app), Ok(delivery(&app))));
        assert_eq!(sheet(&app).files.len(), 1, "delivered: only the file added meanwhile stays");

        // A host that doesn't say its threads take files: the path is typed.
        let work = json!([{"id": "i", "title": "Fix login", "status": "working", "thread": "thread-1", "talk": true}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        app.on_paste(&path);
        assert_eq!(sheet(&app).draft.text, path);
        let _ = std::fs::remove_file(file);
    }

    #[tokio::test]
    async fn the_thread_takes_what_the_user_types_and_the_chats_keys_for_what_waits() {
        let mut app = test_app();
        hello(&mut app);
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
        assert_eq!(sheet(&app).sending.as_ref().map(|sent| sent.text.as_str()), Some("also q"));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(hint(&mut app), "Sending…", "one at a time");
        app.on_msg(Msg::Reply(send_reply(&app), Err("\"Fix login\" hasn't started yet".into())));
        assert_eq!(hint(&mut app), "\"Fix login\" hasn't started yet");
        assert_eq!(sheet(&app).draft.text, "also q", "refused: the words stay");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        app.on_msg(Msg::Reply(send_reply(&app), Ok(delivery(&app))));
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
        app.on_msg(Msg::Reply(read_reply(&app, "thread-1"), Ok(json!({"entries": []}))));
        // Tried again elsewhere, or replaced: a new thread has the task now.
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work("thread-2")}})));
        assert_eq!(sheet(&app).thread, "thread-2");
        assert!(matches!(sheet(&app).fetched, Fetched::Loading), "its transcript loads");
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work("thread-1")}})));
        assert_eq!(sheet(&app).thread, "thread-1", "the first restored snapshot can reassign the task");
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
