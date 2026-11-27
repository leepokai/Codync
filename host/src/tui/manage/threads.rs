//! A task's thread, opened from its row on the Details page: its thread's own transcript (everything the thread did,
//! whatever session it was in), what the thread put in the bot's chat as the chat shows it (its cards answer here too;
//! `↵` on a picked one shows it there), and its composer: the user writes to the thread of the bot's own task as to a
//! chat.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};
use tokio::task::AbortHandle;
use uuid::Uuid;

use super::super::app::{
    After, App, Confirm, ConfirmAct, Editor, Entry, Focus, Kind, Overlay, Queued, WorkTask, card_to_answer, is_shared,
};
use super::super::l10n::tr;
use super::super::net::artifacts::ArtifactTo;
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

#[derive(Clone, Copy)]
pub(in crate::tui) enum ThreadNavigationBlock {
    Sending,
    Draft,
}

impl ThreadNavigationBlock {
    pub(in crate::tui) fn in_overlays(overlays: &[Overlay]) -> Option<Self> {
        let mut threads = overlays.iter().filter_map(|overlay| match overlay {
            Overlay::Thread(thread) => Some(thread),
            _ => None,
        });
        if threads.clone().any(ThreadSheet::send_in_flight) {
            Some(Self::Sending)
        } else {
            threads.any(ThreadSheet::has_draft).then_some(Self::Draft)
        }
    }

    pub(in crate::tui) fn label(self) -> Cow<'static, str> {
        tr(match self {
            Self::Sending => "Sending…",
            Self::Draft => "Draft kept",
        })
    }
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

    fn has_draft(&self) -> bool {
        !self.draft.text.is_empty() || !self.files.is_empty()
    }

    pub(in crate::tui) fn send_in_flight(&self) -> bool {
        self.sending.as_ref().is_some_and(|sent| sent.task.is_some())
    }

    pub(in crate::tui) fn can_edit(&self, task: Option<&WorkTask>) -> bool {
        self.typing || self.has_draft() || task.is_some_and(|task| task.talk)
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
    /// What `thread` put in `bot`'s chat that its page shows (`is_shared`), oldest first.
    pub fn shared(&self, bot: &str, thread: &str) -> Vec<&Entry> {
        self.lane(bot).into_iter().filter(|e| e.data["thread"]["id"] == thread && is_shared(e)).collect()
    }

    /// What `s`'s thread put in its bot's chat that `wanted` takes and the chat's keys act on, as in the chat: the
    /// picked one, else the newest.
    fn thread_card(&self, s: &ThreadSheet, wanted: impl Fn(&Entry) -> bool) -> Option<Entry> {
        card_to_answer(self.shared(&s.bot, &s.thread).into_iter().filter(|e| wanted(e)), s.pick.as_ref()).cloned()
    }

    /// The chat's keys on what the thread put in its bot's chat: y / a / n / N answer an approval (N then writes to
    /// the thread why), F1… a choice, 1–9 either; `o` opens a picked artifact in the browser, `f` saves a picked
    /// message's files. Whether it took `k`: with nothing of the kind there, the key is the page's.
    fn thread_card_key(&mut self, s: &mut ThreadSheet, k: KeyEvent, talks: bool) -> bool {
        let card = match k.code {
            KeyCode::Char('y' | 'a' | 'n' | 'N') => self.thread_card(s, |e| e.pending() && e.kind == Kind::Permission),
            KeyCode::F(1..=12) => self.thread_card(s, |e| e.pending() && e.kind == Kind::Decision),
            KeyCode::Char('1'..='9') => self.thread_card(s, Entry::pending),
            KeyCode::Char('o' | 'f') => self.thread_card(s, |e| s.pick.as_ref() == Some(&e.id)),
            _ => None,
        };
        let Some(e) = card else { return false };
        match k.code {
            KeyCode::Char(c @ ('y' | 'a' | 'n' | 'N')) => {
                if self.answer_card_kind(&e, c) && c == 'N' {
                    s.typing = talks;
                }
            }
            KeyCode::F(n) => {
                if let Some(option) = e.choices().get(usize::from(n - 1)) {
                    self.answer(&e.id, option);
                }
            }
            KeyCode::Char(c @ '1'..='9') => {
                if let Some(option) = e.answer_options().get((c as usize) - ('1' as usize)) {
                    self.answer(&e.id, option);
                }
            }
            KeyCode::Char('o') => {
                if let Some(artifact) = e.artifact() {
                    self.fetch_artifact(&s.bot, artifact, ArtifactTo::Browser);
                }
            }
            _ => self.save_files(&s.bot, &e.id, &e.data),
        }
        true
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
            match overlay {
                Overlay::Confirm(Confirm { act: ConfirmAct::DiscardThreadDraft { entry: Some(_) }, .. }) => {
                    *overlay = Overlay::ThreadDraftKept;
                }
                Overlay::Thread(s) => {
                    (s.fetched, s.pick) = (Fetched::Loading, None);
                    s.scroll.set(0);
                    s.restore_send_files();
                    if let Some(sent) = &mut s.sending {
                        sent.receipt = None;
                    }
                }
                _ => {}
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
        if self.replaying_mirror()
            || !s.task(self).is_some_and(|task| task.talk)
            || (text.is_empty() && s.files.is_empty())
        {
            return;
        }
        if s.send_in_flight() {
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

    pub(in crate::tui) fn keep_thread_overlays(&self) -> bool {
        ThreadNavigationBlock::in_overlays(&self.overlays).is_some()
    }

    pub(in crate::tui) fn close_thread_overlays(&mut self) {
        let mut closed = Vec::new();
        let mut previous_was_thread = false;
        self.overlays.retain(|overlay| {
            let is_thread = if let Overlay::Thread(s) = overlay {
                closed.push((s.bot.clone(), s.thread.clone()));
                true
            } else {
                false
            };
            let is_thread_prompt = previous_was_thread
                && matches!(
                    overlay,
                    Overlay::ThreadDraftKept
                        | Overlay::Confirm(Confirm {
                            act: ConfirmAct::DiscardThreadDraft { .. } | ConfirmAct::StopWork { .. },
                            ..
                        })
                );
            previous_was_thread = is_thread;
            !is_thread && !is_thread_prompt
        });
        for (bot, thread) in closed {
            self.forget_closed_thread(&bot, &thread);
        }
    }

    fn forget_closed_thread(&mut self, bot: &str, thread: &str) {
        if !self.thread_open(thread) && !self.bots.get(bot).is_some_and(|b| works(&b.work, thread)) {
            self.thread_entries.forget(thread);
        }
    }

    fn close_thread(&mut self, s: ThreadSheet, entry: Option<String>) -> Option<Overlay> {
        if s.send_in_flight() {
            if s.pick.is_none() {
                self.flash("Sending…");
            }
            return Some(Overlay::Thread(s));
        }
        if !s.has_draft() {
            return self.leave_thread(s, entry.as_deref());
        }
        if entry.is_some() && self.keep_thread_overlays() {
            return Some(Overlay::Thread(s));
        }
        self.overlays.push(Overlay::Confirm(Confirm {
            title: tr("Discard the draft and its attachments?").into_owned(),
            detail: String::new(),
            note: String::new(),
            button: "Close",
            act: ConfirmAct::DiscardThreadDraft { entry },
        }));
        Some(Overlay::Thread(s))
    }

    pub(in crate::tui) fn discard_thread_draft(&mut self, entry: Option<&str>) {
        if matches!(self.overlays.last(), Some(Overlay::Thread(_)))
            && let Some(Overlay::Thread(s)) = self.overlays.pop()
            && let Some(overlay) = self.leave_thread(s, entry)
        {
            self.overlays.push(overlay);
        }
    }

    fn leave_thread(&mut self, mut s: ThreadSheet, entry: Option<&str>) -> Option<Overlay> {
        let pending = if let Some(id) = entry {
            let Some(row) = self.shared(&s.bot, &s.thread).into_iter().find(|row| row.id == id) else {
                s.pick = None;
                return Some(Overlay::Thread(s));
            };
            row.pending()
        } else {
            false
        };
        if entry.is_some() && self.keep_thread_overlays() {
            return Some(Overlay::Thread(s));
        }
        self.forget_closed_thread(&s.bot, &s.thread);
        if let Some(entry) = entry {
            self.overlays.clear();
            self.select(&s.bot);
            self.chat_page = true;
            self.focus = Focus::Chat;
            self.chat_scroll = 0;
            self.pick = (!pending).then(|| entry.to_owned());
        }
        None
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
        if s.typing {
            self.thread_typing(&mut s, k);
            return Some(Overlay::Thread(s));
        }
        let task = s.task(self);
        let talks = task.is_some_and(|task| task.talk);
        let can_edit = s.can_edit(task);
        let connected = self
            .confirmed_thread_connection()
            .is_some_and(|connection| self.send_receipts.thread_read_connection() == Some(connection));
        if (connected || matches!(k.code, KeyCode::Char('o' | 'f'))) && self.thread_card_key(&mut s, k, talks) {
            return Some(Overlay::Thread(s));
        }
        match k.code {
            KeyCode::Tab | KeyCode::BackTab => s.step_pick(self, k.code == KeyCode::Tab),
            KeyCode::Enter if s.pick.is_some() => {
                let entry = s.pick.clone();
                return self.close_thread(s, entry);
            }
            KeyCode::Esc if s.pick.is_some() => s.pick = None,
            // `C`: its agent's sign-in, the picked one, else the newest.
            KeyCode::Char('C') if connected => {
                let request = self.thread_card(&s, |e| e.data["connectionRequest"]["status"] == "pending");
                self.overlays.push(Overlay::Thread(s));
                self.open_connection_of(request.as_ref());
                return None;
            }
            KeyCode::Char('i') | KeyCode::Enter if can_edit => s.typing = true,
            KeyCode::Char('S') => self.on_thread_queued(&s, Queued::SendNow),
            KeyCode::Char('D') => self.on_thread_queued(&s, Queued::Remove),
            KeyCode::Esc | KeyCode::Char('q') => return self.close_thread(s, None),
            KeyCode::Char('T') => s.full = !s.full,
            KeyCode::Char('X') => {
                if let Some(task) = s.task(self).filter(|i| i.status.live()).cloned() {
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
    use std::time::Duration;

    use axum::{Json, Router, http::StatusCode, routing::post};
    use crossterm::event::{KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};
    use tokio::sync::{mpsc::UnboundedReceiver, oneshot};

    use super::super::super::app::{Click, Field, Msg, SendReceipts, next_request, recording_host, test_app};
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

    fn open_pending_file_send(app: &mut App, file: PathBuf) {
        open_draft(app, "", file);
        let Some(Overlay::Thread(thread)) = app.overlays.last_mut() else { panic!("no thread") };
        thread.typing = false;
        thread.sending = Some(ThreadSend {
            text: String::new(),
            files: std::mem::take(&mut thread.files),
            returned_files: 0,
            nonce: "fixture-nonce".into(),
            thread: thread.thread.clone(),
            request: Uuid::nil(),
            receipt: None,
            task: Some(tokio::spawn(std::future::pending::<()>()).abort_handle()),
        });
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
        assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
        key(&mut app, 'd');
        assert!(app.overlays.is_empty(), "q can close a thread after confirming");
    }

    #[tokio::test]
    async fn a_thread_without_its_restored_task_keeps_the_draft_until_discarded() {
        let file = PathBuf::from("draft.txt");
        for (text, typing) in [("draft", true), ("", false)] {
            let mut app = test_app();
            open_draft(&mut app, text, file.clone());
            if !typing {
                app.on_key(KeyEvent::from(KeyCode::Esc));
            }
            app.on_msg(Msg::Rewind(SendReceipts::default()));
            hello(&mut app);
            bot_with(&mut app, &[]);
            app.on_msg(Msg::ReplayComplete);
            assert!(!app.replaying_mirror() && sheet(&app).task(&app).is_none());

            if typing {
                key(&mut app, 'q');
                assert_eq!(sheet(&app).draft.text, "draftq");
                app.on_key(KeyEvent::from(KeyCode::Esc));
            }
            let draft = sheet(&app).draft.text.clone();
            app.on_key(KeyEvent::from(KeyCode::Esc));
            assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
            app.on_key(KeyEvent::from(KeyCode::Esc));
            assert_eq!(sheet(&app).draft.text, draft);
            assert_eq!(sheet(&app).files, std::slice::from_ref(&file));

            key(&mut app, 'i');
            assert!(sheet(&app).typing);
            app.on_key(KeyEvent::from(KeyCode::Enter));
            assert!(sheet(&app).sending.is_none());
            assert_eq!(sheet(&app).files, std::slice::from_ref(&file));
            app.on_key(KeyEvent::from(KeyCode::Esc));
            key(&mut app, 'q');
            key(&mut app, 'd');
            assert!(app.overlays.is_empty());
        }
    }

    #[tokio::test]
    async fn clearing_the_draft_keeps_the_editor_open_without_a_task() {
        for (text, files) in [("x", vec![]), ("", vec![PathBuf::from("draft.txt")])] {
            let mut app = test_app();
            app.on_msg(writable_task());
            app.overlays.push(Overlay::Thread(ThreadSheet {
                draft: Editor::with(text),
                files,
                typing: true,
                ..ThreadSheet::new("a".into(), "1".into(), "thread-1".into())
            }));
            app.on_msg(Msg::Rewind(SendReceipts::default()));
            hello(&mut app);
            bot_with(&mut app, &[]);
            app.on_msg(Msg::ReplayComplete);

            app.on_key(KeyEvent::from(KeyCode::Backspace));
            assert!(sheet(&app).typing && !sheet(&app).has_draft());
            key(&mut app, 'q');
            assert_eq!(sheet(&app).draft.text, "q");
            app.on_key(KeyEvent::from(KeyCode::Enter));
            assert!(sheet(&app).sending.is_none());
            app.on_key(KeyEvent::from(KeyCode::Esc));
            assert!(!sheet(&app).typing);
        }
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
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.overlays.is_empty(), "a read-only thread with no draft closes directly");
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

        app.on_paste("draft for the thread");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        for kind in [KeyEventKind::Repeat, KeyEventKind::Press, KeyEventKind::Press] {
            app.on_key(KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, kind));
            assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
        }
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(sheet(&app).draft.text, "draft for the thread");
        assert_eq!(sheet(&app).pick.as_deref(), Some("card"));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        key(&mut app, 'd');
        assert!(app.overlays.is_empty() && app.focus == Focus::Chat && app.pick.is_none());
    }

    fn shared_file(rev: i64, thread: &str) -> Msg {
        Msg::Event(json!({"type": "entry", "rev": rev, "entry": {
            "id": "file", "botId": "a", "seq": 1, "rev": rev, "kind": "agent", "turn": 1,
            "data": {"final": true, "thread": {"id": thread}, "files": [{"id": "f", "name": "log.txt", "size": 3}]}
        }}))
    }

    fn draw_terminal(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| crate::tui::view::draw(frame, app)).expect("draw the terminal");
        terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect()
    }

    fn go_to_preserves_a_thread_until_its_send_succeeds(choose_bot: impl Fn(&mut App), focus: Focus) {
        let file = PathBuf::from("draft.txt");
        let go_to = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        let escape = KeyEvent::from(KeyCode::Esc);
        let search = |app: &mut App| {
            app.on_key(go_to);
            app.on_paste("Second");
            app.on_key(KeyEvent::from(KeyCode::Down));
        };
        let assert_picker_kept = |app: &App| {
            assert!(matches!(app.overlays.as_slice(), [Overlay::Thread(_), Overlay::Goto(g)]
                if g.query.text == "Second" && g.cursor == 1));
            assert_eq!(app.selected.as_deref(), Some("a"));
        };
        for delivered in [false, true] {
            let mut app = test_app();
            hello(&mut app);
            open_pending_file_send(&mut app, file.clone());
            for (id, name) in [("b", "Second beta"), ("c", "Second alpha")] {
                app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": id, "name": name}})));
            }
            app.select("a");
            let reply = send_reply(&app);
            let result = if delivered { Ok(delivery(&app)) } else { Err("refused".into()) };

            search(&mut app);
            choose_bot(&mut app);
            assert_picker_kept(&app);
            assert!(draw_terminal(&mut app).contains(tr("Sending…").as_ref()));

            app.on_key(escape);
            assert!(sheet(&app).send_in_flight(), "Esc closes Go to while the thread still waits");
            search(&mut app);
            app.on_msg(Msg::Reply(reply, result));
            choose_bot(&mut app);
            if delivered {
                assert!(app.overlays.is_empty());
                assert_eq!(app.selected.as_deref(), Some("b"));
                assert_eq!(app.focus, focus);
                assert_eq!(app.typing, focus == Focus::Chat);
                if app.typing {
                    key(&mut app, 'm');
                    assert_eq!(app.drafts["b"].text, "m");
                }
            } else {
                assert_picker_kept(&app);
                let drawn = draw_terminal(&mut app);
                assert!(drawn.contains(&format!("{} · esc back", tr("Draft kept"))));
                assert!(drawn.contains("refused"));
                app.on_key(escape);
                assert_eq!(sheet(&app).files, std::slice::from_ref(&file));
                key(&mut app, 'q');
                assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
                key(&mut app, 'd');
                assert!(app.overlays.is_empty());
            }
        }
    }

    #[tokio::test]
    async fn go_to_enter_keeps_a_thread_draft_and_opens_a_safe_bot_in_the_composer() {
        go_to_preserves_a_thread_until_its_send_succeeds(|app| app.on_key(KeyEvent::from(KeyCode::Enter)), Focus::Chat);
    }

    fn go_to_from_loaded_thread(status: &str) -> App {
        let mut app = test_app();
        bot_with(&mut app, &[("1", status)]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "b", "name": "B"}})));
        key(&mut app, 'R');
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(sheet(&app).thread, "thread-1");
        app.on_msg(Msg::Reply(
            read_reply(&app, "thread-1"),
            Ok(json!({"entries": [thread_entry(1, 1, "the earlier transcript")]})),
        ));
        assert!(matches!(sheet(&app).fetched, Fetched::Loaded));
        assert_eq!(texts(&app), ["the earlier transcript"]);

        app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        key(&mut app, 'b');
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.selected.as_deref(), Some("b"));
        assert!(matches!(app.overlays.as_slice(), [Overlay::Details(_)]));
        app
    }

    #[tokio::test]
    async fn go_to_enter_releases_the_closed_recent_threads_transcript() {
        let app = go_to_from_loaded_thread("done");
        assert!(texts(&app).is_empty(), "the closed, finished thread releases its transcript");
    }

    #[tokio::test]
    async fn go_to_enter_keeps_the_closed_working_threads_transcript() {
        let app = go_to_from_loaded_thread("working");
        assert_eq!(texts(&app), ["the earlier transcript"]);
    }

    #[tokio::test]
    async fn go_to_enter_keeps_an_open_bot_settings_form() {
        let mut app = test_app();
        bot_with(&mut app, &[]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "b", "name": "B"}})));
        let instructions = "Keep the working draft";
        key(&mut app, 'e');
        app.on_key(KeyEvent::from(KeyCode::Tab));
        for c in instructions.chars() {
            key(&mut app, c);
        }
        assert!(draw_terminal(&mut app).contains(instructions));

        app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        key(&mut app, 'b');
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.selected.as_deref(), Some("b"));
        let [Overlay::Form(form)] = app.overlays.as_slice() else { panic!("A's settings form stays open") };
        assert_eq!(form.bot_id, "a");
        assert_eq!(form.current(), Field::Instructions);
        assert_eq!(form.instructions.text, instructions);
        assert!(draw_terminal(&mut app).contains(instructions));

        let (url, mut requests) = recording_host().await;
        app.client = crate::tui::net::Client::new(&url, Some("test"));
        app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let request = next_request(&mut requests).await;
        let (headers, body) = request.split_once("\r\n\r\n").expect("an HTTP request");
        assert!(headers.starts_with("POST /api/updateBot "));
        assert_eq!(
            serde_json::from_str::<Value>(body).expect("a JSON body"),
            json!({"id": "a", "description": instructions})
        );
        app.on_msg(Msg::Reply(
            After::Saved(app.mirror_epoch()),
            Ok(json!({
                "bot": {"id": "a", "name": "A", "description": instructions}
            })),
        ));
        assert!(app.overlays.is_empty());
        assert_eq!(app.bots["a"].description, instructions);
        key(&mut app, 'm');
        assert_eq!(app.drafts["b"].text, "m");
    }

    #[tokio::test]
    async fn go_to_enter_closes_a_thread_with_its_prompt_and_keeps_other_edits() {
        let mut app = test_app();
        bot_with(&mut app, &[("1", "working")]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "b", "name": "B"}})));
        app.custom_icons = true;
        key(&mut app, 'e');
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_paste("Keep instructions");
        app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_paste("draft-icon.png");

        let go_to = |app: &mut App, query: &str| {
            app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
            app.on_paste(query);
            app.on_key(KeyEvent::from(KeyCode::Enter));
        };
        go_to(&mut app, "Details of this bot");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(sheet(&app).task, "1");
        key(&mut app, 'X');
        assert!(draw_terminal(&mut app).contains(tr("Stop this task?").as_ref()));

        go_to(&mut app, "B");
        assert_eq!(app.selected.as_deref(), Some("b"));
        let [Overlay::Form(form), Overlay::Fields(fields), Overlay::Details(_)] = app.overlays.as_slice() else {
            panic!("the form, icon editor and Details stay open")
        };
        assert_eq!(form.instructions.text, "Keep instructions");
        assert_eq!(fields.inputs[0].ed.text, "draft-icon.png");

        key(&mut app, 'X');
        go_to(&mut app, "B");
        assert!(draw_terminal(&mut app).contains(tr("Stop this task?").as_ref()), "Details keeps its confirmation");
        key(&mut app, 'q');
        key(&mut app, 'q');
        assert!(draw_terminal(&mut app).contains("draft-icon.png"));
    }

    #[tokio::test]
    async fn go_to_click_keeps_a_thread_draft_and_opens_a_safe_bot_in_the_roster() {
        go_to_preserves_a_thread_until_its_send_succeeds(
            |app| {
                draw_terminal(app);
                let row = app.hits.clicks.iter().find(|(_, hit)| matches!(hit, Click::Bot(id) if id == "b"));
                let (rect, _) = row.expect("the bot row is on screen");
                app.on_mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE,
                });
            },
            Focus::Roster,
        );
    }

    #[tokio::test]
    async fn go_to_can_open_an_action_over_a_thread_draft() {
        let mut app = test_app();
        let file = PathBuf::from("draft.txt");
        open_draft(&mut app, "draft text", file.clone());
        app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
        app.on_paste("Keys");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(app.overlays.as_slice(), [Overlay::Thread(_), Overlay::Help(_)]));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(sheet(&app).draft.text, "draft text");
        assert_eq!(sheet(&app).files, &[file]);
    }

    #[tokio::test]
    async fn a_chat_reference_keeps_a_draft_in_the_thread_underneath() {
        let file = PathBuf::from("draft.txt");
        for delivered in [false, true] {
            let mut app = test_app();
            hello(&mut app);
            open_pending_file_send(&mut app, file.clone());
            let reply = send_reply(&app);
            let result = if delivered { Ok(delivery(&app)) } else { Err("refused".into()) };
            app.overlays.push(Overlay::Thread(ThreadSheet::new("a".into(), "2".into(), "thread-2".into())));
            app.on_msg(shared_file(1, "thread-2"));
            app.on_key(KeyEvent::from(KeyCode::Tab));
            app.on_key(KeyEvent::from(KeyCode::Enter));
            assert_eq!(sheet(&app).pick.as_deref(), Some("file"));
            let waiting = draw_terminal(&mut app);

            app.on_msg(Msg::Reply(reply, result));
            if delivered {
                assert!(draw_terminal(&mut app).contains("↵ shows it in the chat"));
                app.on_key(KeyEvent::from(KeyCode::Enter));
                assert!(app.overlays.is_empty() && app.chat_page);
                assert_eq!(app.pick.as_deref(), Some("file"));
            } else {
                for _ in 0..2 {
                    app.on_key(KeyEvent::from(KeyCode::Enter));
                }
                assert_eq!(app.overlays.len(), 2, "the chat jump keeps the underlying thread open");
                assert_eq!(sheet(&app).pick.as_deref(), Some("file"));
                let drawn = draw_terminal(&mut app);
                assert!(drawn.contains(&format!("{} · q back · esc clears pick", tr("Draft kept"))));
                assert_eq!(drawn.matches(tr("Draft kept").as_ref()).count(), 1);
                assert!(drawn.contains("refused"));
                assert!(!drawn.contains("↵ shows it in the chat"));
                app.on_key(KeyEvent::from(KeyCode::Esc));
                assert_eq!(app.overlays.len(), 2);
                assert!(sheet(&app).pick.is_none());
                app.on_key(KeyEvent::from(KeyCode::Esc));
                assert_eq!(sheet(&app).files, std::slice::from_ref(&file));
            }
            assert!(waiting.contains(&format!("{} · q back", tr("Sending…"))));
            assert!(!waiting.contains("↵ shows it in the chat"));
        }
    }

    #[tokio::test]
    async fn a_pending_file_send_keeps_the_thread_open_until_the_receipt() {
        let mut app = test_app();
        let file = PathBuf::from("draft.txt");
        hello(&mut app);
        open_draft(&mut app, "", file.clone());
        app.on_msg(shared_file(1, "thread-1"));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let reply = send_reply(&app);
        let sent = delivery(&app);
        app.on_key(KeyEvent::from(KeyCode::Esc));
        for code in [KeyCode::Esc, KeyCode::Char('q')] {
            app.on_key(KeyEvent::from(code));
            assert!(sheet(&app).send_in_flight());
        }
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(sheet(&app).send_in_flight());
        assert_eq!(sheet(&app).pick.as_deref(), Some("file"));

        app.on_msg(Msg::Reply(reply, Err("refused".into())));
        assert_eq!(sheet(&app).files, std::slice::from_ref(&file));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        key(&mut app, 'i');
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(delivery(&app), sent, "retry sends with the original nonce");
        app.on_msg(Msg::Reply(send_reply(&app), Ok(sent)));
        assert!(sheet(&app).sending.is_none() && sheet(&app).files.is_empty());
        app.on_key(KeyEvent::from(KeyCode::Esc));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.overlays.is_empty() && app.chat_page);
        assert_eq!(app.pick.as_deref(), Some("file"));
    }

    #[tokio::test]
    async fn a_pending_send_leaves_other_threads_alone_and_replayed_files_can_be_discarded() {
        let mut app = test_app();
        let file = PathBuf::from("draft.txt");
        hello(&mut app);
        open_draft(&mut app, "", file.clone());
        app.on_key(KeyEvent::from(KeyCode::Enter));
        app.on_paste("new draft");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        key(&mut app, 'q');
        assert!(sheet(&app).send_in_flight());

        let original = sheet(&app).task(&app).expect("writable task");
        let other = WorkTask { id: "2".into(), thread: Some("thread-2".into()), ..original.clone() };
        app.open_thread("a", &other);
        app.on_msg(shared_file(1, "thread-2"));
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(sheet(&app).thread, "thread-2");
        assert_eq!(sheet(&app).pick.as_deref(), Some("file"));
        key(&mut app, 'q');
        assert_eq!(sheet(&app).thread, "thread-1");
        assert!(sheet(&app).send_in_flight());

        app.on_msg(Msg::Rewind(SendReceipts::default()));
        assert_eq!(sheet(&app).files, &[file]);
        assert_eq!(sheet(&app).draft.text, "new draft");
        key(&mut app, 'q');
        assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
        key(&mut app, 'd');
        assert!(app.overlays.is_empty());
    }

    #[tokio::test]
    async fn a_reference_that_changed_threads_cannot_discard_the_draft() {
        let mut app = test_app();
        let file = PathBuf::from("draft.txt");
        open_draft(&mut app, "keep", file.clone());
        app.on_msg(shared_file(1, "thread-1"));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));

        app.on_msg(shared_file(2, "thread-2"));
        assert!(app.shared("a", "thread-1").is_empty());
        key(&mut app, 'd');
        assert_eq!(sheet(&app).draft.text, "keep");
        assert_eq!(sheet(&app).files, &[file]);
        assert!(sheet(&app).pick.is_none());
    }

    #[tokio::test]
    async fn a_rewind_holds_reference_input_until_escape_but_keeps_plain_close_confirmation() {
        let mut app = test_app();
        let file = PathBuf::from("draft.txt");
        open_draft(&mut app, "keep", file.clone());
        app.on_msg(shared_file(1, "thread-1"));
        app.on_key(KeyEvent::from(KeyCode::Esc));
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        let check_notice = |app: &mut App| {
            for kind in [KeyEventKind::Press, KeyEventKind::Repeat, KeyEventKind::Press] {
                app.on_key(KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, kind));
            }
            key(app, 'd');
            let [Overlay::Thread(thread), Overlay::ThreadDraftKept] = app.overlays.as_slice() else {
                panic!("the notice still covers the thread");
            };
            assert!(!thread.typing && thread.sending.is_none() && thread.pick.is_none());
            assert_eq!(thread.draft.text, "keep");
            assert_eq!(thread.files, std::slice::from_ref(&file));
        };
        check_notice(&mut app);
        hello(&mut app);
        app.on_msg(writable_task());
        check_notice(&mut app);
        app.on_msg(Msg::ReplayComplete);
        check_notice(&mut app);
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(!sheet(&app).typing && sheet(&app).task(&app).is_some_and(|task| task.talk));

        key(&mut app, 'q');
        app.on_msg(Msg::Rewind(SendReceipts::default()));
        assert!(matches!(app.overlays.last(), Some(Overlay::Confirm(_))));
        key(&mut app, 'd');
        assert!(app.overlays.is_empty());
    }

    #[tokio::test]
    async fn the_threads_cards_answer_in_the_thread_as_in_the_chat() {
        let mut app = test_app();
        hello(&mut app);
        let work = json!([{"id": "i", "title": "Fix login", "status": "needsYou", "thread": "thread-1", "talk": true}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        let thread = json!({"id": "thread-1", "title": "Fix login"});
        let chat = |id: &str, seq: i64, kind: &str, mut data: Value| {
            data["thread"] = thread.clone();
            data["task"] = json!("i");
            let entry = json!({"id": id, "botId": "a", "seq": seq, "kind": kind, "turn": 1, "data": data});
            Msg::Event(json!({"type": "entry", "entry": entry}))
        };
        let options = json!([{"optionId": "ok", "name": "Allow", "kind": "allow_once"},
            {"optionId": "no", "name": "Deny", "kind": "reject_once"}]);
        let approval = |title: &str| json!({"status": "pending", "title": title, "options": options});
        app.on_msg(chat("old", 1, "permission", approval("Read the logs")));
        app.on_msg(chat("card", 2, "permission", approval("Run tests")));
        let choice =
            json!({"decision": true, "status": "pending", "question": "Squash?", "choices": ["Squash", "Merge"]});
        app.on_msg(chat("choice", 3, "notice", choice));
        let meta = json!([{"id": "f1", "name": "log.txt", "size": 3, "sha256": "00"}]);
        let file = json!({"text": "log.txt", "final": true, "files": meta});
        app.on_msg(chat("file", 4, "agent", file));
        let artifact = json!({"text": "Report", "final": true, "artifact": {"id": "art-1", "title": "Report"}});
        app.on_msg(chat("art", 5, "agent", artifact));
        let own = app.bots["a"].work[0].clone();
        app.open_thread("a", &own);
        let pick = |app: &mut App, id: &str| match app.overlays.last_mut() {
            Some(Overlay::Thread(s)) => s.pick = Some(id.to_owned()),
            _ => panic!("no transcript is open"),
        };
        let hint = |app: &App| app.hint.as_ref().map(|(text, _)| text.clone()).unwrap_or_default();

        // Nothing picked: `o` is the page's, and opens nothing.
        app.on_key(KeyEvent::from(KeyCode::Char('o')));
        assert_eq!(hint(&app), "");
        // The picked card first: 1 answers the older approval.
        pick(&mut app, "old");
        app.on_key(KeyEvent::from(KeyCode::Char('1')));
        assert_eq!(app.answering.get("old").map(String::as_str), Some("ok"));
        assert!(!app.answering.contains_key("card"), "only the picked one");
        // Else the newest: N denies the newer approval and gives the keys to the thread's composer, to say why.
        app.on_key(KeyEvent::from(KeyCode::Esc));
        app.on_key(KeyEvent::from(KeyCode::Char('N')));
        assert_eq!(app.answering.get("card").map(String::as_str), Some("no"));
        assert!(sheet(&app).typing, "N writes to the thread");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        // A choice: F2 picks its second, as 2 does.
        app.on_key(KeyEvent::from(KeyCode::F(2)));
        assert_eq!(app.answering.remove("choice").as_deref(), Some("Merge"));
        app.on_key(KeyEvent::from(KeyCode::Char('2')));
        assert_eq!(app.answering.get("choice").map(String::as_str), Some("Merge"), "the newest card of any kind");
        // A picked file saves; a picked artifact opens in the browser.
        pick(&mut app, "file");
        app.on_key(KeyEvent::from(KeyCode::Char('f')));
        assert!(hint(&app).starts_with("Downloading"), "{}", hint(&app));
        pick(&mut app, "art");
        app.on_key(KeyEvent::from(KeyCode::Char('o')));
        assert!(hint(&app).starts_with("Opening the artifact"), "{}", hint(&app));
        assert!(matches!(app.overlays.last(), Some(Overlay::Thread(_))), "it stays open");

        app.on_key(KeyEvent::from(KeyCode::Esc));
        app.answering.clear();
        app.on_msg(Msg::Disconnected { since: 0 });
        app.on_key(KeyEvent::from(KeyCode::Char('1')));
        assert!(app.answering.is_empty(), "a disconnected card cannot answer");
    }

    type ArtifactRead = (Value, oneshot::Sender<()>);

    async fn thread_with_shared_artifact() -> (App, UnboundedReceiver<ArtifactRead>, UnboundedReceiver<Msg>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let url = format!("http://{}", listener.local_addr().expect("the fixture address"));
        let (requests, reads) = tokio::sync::mpsc::unbounded_channel();
        let router = Router::new().route(
            "/api/readArtifact",
            post(move |Json(body): Json<Value>| {
                let (release, held) = oneshot::channel();
                let _ = requests.send((body, release));
                async move {
                    let _ = held.await;
                    (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "fixture artifact unavailable"})))
                }
            }),
        );
        tokio::spawn(async move { axum::serve(listener, router).await.expect("the fixture serves") });
        let (tx, messages) = tokio::sync::mpsc::unbounded_channel();
        let client = crate::tui::net::Client::new(&url, Some("test"));
        let mut app = App::new(client, tx, url);
        hello(&mut app);
        app.on_msg(writable_task());
        app.on_msg(shared_file(1, "thread-1"));
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": {
            "id": "art", "botId": "a", "seq": 2, "turn": 1, "kind": "agent",
            "data": {"final": true, "thread": {"id": "thread-1"}, "artifact": {"id": "art-1", "title": "Report"}}
        }})));
        app.open_thread("a", &app.bots["a"].work[0].clone());
        (app, reads, messages)
    }

    async fn artifact_read(reads: &mut UnboundedReceiver<ArtifactRead>) -> oneshot::Sender<()> {
        let (body, release) = tokio::time::timeout(Duration::from_secs(5), reads.recv())
            .await
            .expect("the host receives readArtifact")
            .expect("the fixture is listening");
        assert_eq!(body, json!({"botId": "a", "artifactId": "art-1", "offset": 0}));
        release
    }

    #[tokio::test]
    async fn f_cancels_the_threads_running_download_after_disconnect() {
        let (mut app, mut reads, _messages) = thread_with_shared_artifact().await;
        app.on_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(sheet(&app).pick.as_deref(), Some("art"));
        key(&mut app, 'f');
        let _held = artifact_read(&mut reads).await;
        assert!(app.download.is_some(), "the host is holding the running download");

        app.send_receipts.invalidate_connection();
        app.on_msg(Msg::Disconnected { since: 1 });
        key(&mut app, 'f');
        assert!(app.download.is_none(), "f cancels the running download after disconnect");
        assert_eq!(app.hint.as_ref().map(|(text, _)| text.as_str()), Some("Download cancelled · f retries"));
    }

    #[tokio::test]
    async fn a_picked_thread_artifact_and_file_are_actionable_during_mirror_replay() {
        let (mut app, mut reads, mut messages) = thread_with_shared_artifact().await;
        app.on_msg(Msg::ReplayStarted);
        key(&mut app, 'o');
        assert!(app.download.is_none(), "o needs an explicit pick");
        app.on_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(sheet(&app).pick.as_deref(), Some("art"));
        key(&mut app, 'o');
        assert!(app.download.is_some(), "o starts loading the picked artifact during replay");
        artifact_read(&mut reads).await.send(()).expect("the fixture has the read");
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let msg = messages.recv().await.expect("a download outcome");
                if matches!(msg, Msg::FileDownload { done: true, .. }) {
                    break msg;
                }
            }
        })
        .await
        .expect("the artifact read reports the HTTP error");
        app.on_msg(result);
        assert_eq!(
            app.hint.as_ref().map(|(text, _)| text.as_str()),
            Some("Download failed: fixture artifact unavailable")
        );

        app.on_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(sheet(&app).pick.as_deref(), Some("file"));
        key(&mut app, 'f');
        assert!(
            app.hint.as_ref().is_some_and(|(text, _)| text.starts_with("Invalid file:")),
            "f reaches the picked file"
        );
    }

    #[tokio::test]
    async fn c_opens_the_threads_agent_sign_in_over_the_thread() {
        let mut app = test_app();
        hello(&mut app);
        let work = json!([{"id": "i", "title": "Fix login", "status": "needsYou", "thread": "thread-1", "talk": true}]);
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A", "work": work}})));
        let request = json!({"kind": "agent", "status": "pending", "title": "Claude", "backend": "claude"});
        let data = json!({"connectionRequest": request, "thread": {"id": "thread-1"}, "task": "i"});
        let entry = json!({"id": "sign", "botId": "a", "seq": 1, "kind": "notice", "turn": 1, "data": data});
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": entry})));
        let own = app.bots["a"].work[0].clone();
        app.open_thread("a", &own);

        app.on_msg(Msg::ReplayStarted);
        app.on_key(KeyEvent::from(KeyCode::Char('C')));
        assert!(matches!(app.overlays.last(), Some(Overlay::Thread(_))), "a replayed request cannot open sign-in");
        app.on_msg(Msg::ReplayComplete);
        app.on_key(KeyEvent::from(KeyCode::Char('C')));
        let n = app.overlays.len();
        assert!(matches!(app.overlays.last(), Some(Overlay::Agent(a)) if a.backend == "claude"), "the agent's sign-in");
        assert!(matches!(app.overlays.get(n - 2), Some(Overlay::Thread(_))), "over the thread");
    }
}
