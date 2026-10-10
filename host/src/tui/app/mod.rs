//! TUI state and everything that changes it: host events, keys, mouse, replies.

mod analytics;
mod editor;
mod events;
mod exchange;
mod forms;
mod input;
mod model;
mod navigate;
mod overlay_keys;
mod ui;

pub use editor::Editor;
pub use exchange::{ConversationRow, Exchange, Item, Outcome, conversation, group, rows};
pub use model::{After, Bot, Entry, Kind, Mark, Msg, Status};
pub use ui::{
    ACTIONS, Action, AgentPicker, COLORS, Click, Confirm, ConfirmAct, Dir, FIELDS, FILTERS, Field, Focus, FolderPicker,
    Form, Goto, GotoItem, GroupField, GroupForm, Hits, NEW_GROUP, Overlay, SHAPES, Toast, Toggle, TraceMode, Width,
};

use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write as _;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

use super::manage::Shell;
use super::net::Client;

#[derive(Clone)]
struct PendingSend {
    text: String,
    files: Vec<std::path::PathBuf>,
    nonce: String,
    busy: bool,
}

#[allow(clippy::struct_excessive_bools, reason = "independent UI flags, not a state machine")]
pub struct App {
    pub client: Client,
    pub(super) tx: UnboundedSender<Msg>,
    pub url: String,
    pub host: String,
    pub home: String,
    pub online: bool,
    pub error: Option<String>,
    /// This client and the host can't work together until one of them updates.
    pub mismatch: Option<crate::compat::Mismatch>,
    pub bots: HashMap<String, Bot>,
    pub entries: HashMap<String, BTreeMap<i64, Entry>>,
    pub usage: Value,
    pub screen: Value,
    pub backends: Vec<Value>,
    pub selected: Option<String>,
    pub focus: Focus,
    pub typing: bool,
    /// Narrow layout: showing the chat page (else the roster page).
    pub chat_page: bool,
    pub drafts: HashMap<String, Editor>,
    /// Files dropped on the composer (pasted paths), per draft like the text.
    pub files: HashMap<String, Vec<std::path::PathBuf>>,
    pub download: Option<super::net::files::Download>,
    sends: HashMap<String, PendingSend>,
    /// Lines scrolled up from the bottom of the chat (0 = follow new messages).
    pub chat_scroll: usize,
    pub trace_scroll: usize,
    pub trace: TraceMode,
    /// Turn shown in the trace; `None` follows the latest.
    pub trace_turn: Option<i64>,
    pub compact: bool,
    pub overlays: Vec<Overlay>,
    pub toasts: Vec<Toast>,
    pub frame: u64,
    pub term_focused: bool,
    pub quit: bool,
    pub hits: Hits,
    pub width: Width,
    /// Chat/trace viewport heights from the last draw, for page scrolling.
    pub chat_height: usize,
    pub chat_top: bool,
    /// The open thread's root entry in the selected chat; `None` shows the main chat.
    pub thread: Option<String>,
    /// A main-chat message picked to reply to in its thread (`r`, then j/k and ↵).
    pub pick: Option<String>,
    pub market: (Vec<Toggle>, Vec<Toggle>),
    /// Installed connectors and skills as the host lists them.
    pub plugins: (Vec<Value>, Vec<Value>),
    /// Models each agent offers (`None` while loading).
    pub models: HashMap<String, Option<Vec<(String, String)>>>,
    /// A setup terminal holding the screen.
    pub shell: Option<Shell>,
    reading: HashSet<String>,
    history_busy: HashSet<String>,
    /// Permission cards whose answer is on its way, with the chosen option.
    pub answering: HashMap<String, String>,
    history_done: HashSet<String>,
    /// The `since` the current events connection requested.
    stream_since: i64,
    /// Per bot, the lowest seq of its loaded main chat when this connection's catch-up began
    /// (empty when it asked for everything): unknown entries below it are rewrites of old ones.
    floors: HashMap<String, i64>,
    pub hint: Option<(String, Instant)>,
    /// Whether this computer shares usage analytics (`None`: nobody decided yet).
    pub analytics: Option<bool>,
    /// The first `hello` of this run was handled (`app_opened`, the analytics question).
    opened: bool,
}

impl App {
    pub fn new(client: Client, tx: UnboundedSender<Msg>, url: String) -> Self {
        Self {
            download: None,
            client,
            tx,
            url,
            host: String::new(),
            home: String::new(),
            online: false,
            error: None,
            mismatch: None,
            bots: HashMap::new(),
            entries: HashMap::new(),
            usage: Value::Null,
            screen: Value::Null,
            backends: vec![],
            selected: None,
            focus: Focus::Roster,
            typing: false,
            chat_page: false,
            drafts: HashMap::new(),
            files: HashMap::new(),
            sends: HashMap::new(),
            chat_scroll: 0,
            trace_scroll: 0,
            trace: TraceMode::Off,
            trace_turn: None,
            compact: false,
            overlays: vec![],
            toasts: vec![],
            frame: 0,
            term_focused: true,
            quit: false,
            hits: Hits::default(),
            width: Width::Wide,
            chat_height: 20,
            chat_top: false,
            thread: None,
            pick: None,
            market: (vec![], vec![]),
            plugins: (vec![], vec![]),
            models: HashMap::new(),
            shell: None,
            reading: HashSet::new(),
            history_busy: HashSet::new(),
            answering: HashMap::new(),
            history_done: HashSet::new(),
            stream_since: 0,
            floors: HashMap::new(),
            hint: None,
            analytics: None,
            opened: false,
        }
    }

    pub fn call(&self, method: &'static str, body: Value, after: After) {
        self.client.spawn_call(method, body, after, self.tx.clone());
    }

    // ---------- derived ----------

    /// Roster order: pinned first, then most recent (same as the phone).
    pub fn roster(&self) -> Vec<&Bot> {
        let mut v: Vec<&Bot> = self.bots.values().filter(|b| !b.hidden).collect();
        v.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.last_at.cmp(&a.last_at)).then(a.name.cmp(&b.name)));
        v
    }

    pub fn bot(&self) -> Option<&Bot> {
        self.selected.as_ref().and_then(|id| self.bots.get(id))
    }

    pub fn counts(&self) -> [usize; 4] {
        let mut c = [0; 4];
        for b in self.bots.values().filter(|b| !b.hidden) {
            match b.mark() {
                Mark::Need => c[0] += 1,
                Mark::Work => c[1] += 1,
                Mark::Unread => c[2] += 1,
                Mark::Error => c[3] += 1,
                Mark::Idle => {}
            }
        }
        c
    }

    /// A chat's entries in the lane on screen (main chat, or the open thread's replies), in display order.
    pub fn lane(&self, bot: &str) -> Vec<&Entry> {
        let thread = self.thread.as_deref();
        let mut v: Vec<&Entry> = self
            .entries
            .get(bot)
            .map(|m| m.values().filter(|e| e.thread_id.as_deref() == thread).collect())
            .unwrap_or_default();
        // A message sent mid-turn belongs after that turn's reply.
        v.sort_by_key(|e| (e.turn, e.seq));
        v
    }

    pub fn pending(&self) -> Option<&Entry> {
        let id = self.selected.as_ref()?;
        self.lane(id).into_iter().rev().find(|e| e.pending())
    }

    /// Where the composer's draft lives: per chat, and per thread in it.
    fn draft_key(&self) -> Option<String> {
        let id = self.selected.as_ref()?;
        Some(self.thread.as_ref().map_or_else(|| id.clone(), |root| format!("{id}#{root}")))
    }

    pub fn draft(&self) -> Editor {
        self.draft_key().and_then(|k| self.drafts.get(&k).cloned()).unwrap_or_default()
    }

    pub fn draft_files(&self) -> &[std::path::PathBuf] {
        self.draft_key().and_then(|k| self.files.get(&k)).map_or(&[], Vec::as_slice)
    }

    /// Name of an entry's author (a group reply's bot), as the apps show it.
    pub fn author_name(&self, id: Option<&str>) -> String {
        id.and_then(|id| self.bots.get(id)).map_or_else(|| "A deleted bot".into(), |b| b.name.clone())
    }

    /// The turns of the lane on screen, oldest first: the trace steps through these.
    /// Turn numbers count per bot, so a lane's own are not consecutive.
    pub fn lane_turns(&self, bot: &str) -> Vec<i64> {
        let mut v: Vec<i64> = self.lane(bot).iter().map(|e| e.turn).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Whether every older entry of this bot is loaded.
    pub fn history_complete(&self, id: &str) -> bool {
        self.history_done.contains(id)
            || self.entries.get(id).is_none_or(BTreeMap::is_empty)
            || self.oldest_main(id).is_some_and(|s| s <= 1)
    }

    /// Paging (`history`) covers the main chat only; thread replies don't count.
    fn oldest_main(&self, id: &str) -> Option<i64> {
        self.entries.get(id)?.values().find(|e| e.thread_id.is_none()).map(|e| e.seq)
    }

    pub fn chat_visible(&self) -> bool {
        self.selected.is_some() && (self.width != Width::Narrow || self.chat_page) && self.trace != TraceMode::Full
    }

    pub fn animating(&self) -> bool {
        !self.toasts.is_empty()
            || self.hint.is_some()
            || !self.online
            || !self.answering.is_empty()
            || self.bots.values().any(|b| b.status == Status::Working)
    }

    fn select(&mut self, id: &str) {
        if self.selected.as_deref() != Some(id) {
            self.selected = Some(id.to_owned());
            self.thread = None;
            self.pick = None;
            self.chat_scroll = 0;
            self.trace_scroll = 0;
            self.trace_turn = None;
        }
    }

    fn move_bot(&mut self, delta: isize) {
        let ids: Vec<String> = self.roster().iter().map(|b| b.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let i = self.selected.as_ref().and_then(|s| ids.iter().position(|x| x == s));
        let n = isize::try_from(ids.len()).unwrap_or(isize::MAX);
        let next = match i {
            Some(i) => (isize::try_from(i).unwrap_or(0) + delta).clamp(0, n - 1),
            None => 0,
        };
        let id = ids[usize::try_from(next).unwrap_or(0)].clone();
        self.select(&id);
    }

    /// Next bot (after the selected one, wrapping) whose mark matches.
    fn jump(&mut self, want: Mark) {
        // A bot busy in a group needs you there, and the group is marked too: skip the bot.
        let ids: Vec<(String, bool)> = self
            .roster()
            .iter()
            .map(|b| (b.id.clone(), b.mark() == want && (want != Mark::Need || b.away().is_none())))
            .collect();
        let start = self.selected.as_ref().and_then(|s| ids.iter().position(|(x, _)| x == s)).map_or(0, |i| i + 1);
        let found = (0..ids.len()).map(|k| &ids[(start + k) % ids.len()]).find(|(_, hit)| *hit).cloned();
        match found {
            Some((id, _)) => {
                self.select(&id);
                self.chat_page = true;
                self.focus = Focus::Chat;
                // A card waiting in a thread shows there.
                let thread = self.bots.get(&id).and_then(|b| b.working_thread.clone()).filter(|_| want == Mark::Need);
                if let Some(root) = thread {
                    self.open_thread(root);
                }
            }
            None => self.flash(match want {
                Mark::Need => "No bot needs you",
                Mark::Unread => "Nothing unread",
                _ => "No match",
            }),
        }
    }

    pub fn flash(&mut self, s: &str) {
        self.hint = Some((s.to_owned(), Instant::now() + Duration::from_secs(3)));
    }

    fn toast(&mut self, text: String, need: bool) {
        if !self.term_focused {
            // OSC 9 desktop notification (iTerm2, Ghostty, kitty, WezTerm…) plus a bell.
            let mut out = std::io::stdout();
            let _ = write!(out, "\x1b]9;{}\x07\x07", text.replace(['\x07', '\x1b'], ""));
            let _ = out.flush();
        }
        self.toasts.retain(|t| t.text != text);
        self.toasts.push(Toast { text, need, until: Instant::now() + Duration::from_secs(4) });
        if self.toasts.len() > 2 {
            self.toasts.remove(0);
        }
    }

    /// Drops expired toasts and keeps the viewed bot marked read.
    pub fn tick(&mut self) {
        if self.selected.is_none()
            && let Some(id) = self.roster().first().map(|b| b.id.clone())
        {
            self.selected = Some(id);
        }
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.hint.as_ref().is_some_and(|(_, until)| *until <= now) {
            self.hint = None;
        }
        if self.chat_visible()
            && self.term_focused
            && let Some(b) = self.bot()
            && b.unread > 0
            && !self.reading.contains(&b.id)
        {
            let id = b.id.clone();
            self.reading.insert(id.clone());
            self.call("markRead", json!({"botId": id, "threadId": self.thread}), After::Nothing);
        }
        if self.chat_top && self.chat_visible() && self.thread.is_none() {
            self.load_history();
        }
    }

    fn load_history(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        if self.history_busy.contains(&id) || self.history_done.contains(&id) {
            return;
        }
        let oldest = self.oldest_main(&id).unwrap_or(i64::MAX);
        if oldest <= 1 {
            self.history_done.insert(id);
            return;
        }
        self.history_busy.insert(id.clone());
        self.call("history", json!({"botId": id, "beforeSeq": oldest, "limit": 200}), After::History(id));
    }
}

/// What to update, in the words every client uses (docs/reference/compatibility.md).
pub fn mismatch_text(m: &crate::compat::Mismatch, host: &str) -> String {
    let host = if host.is_empty() { "the computer" } else { host };
    match m {
        crate::compat::Mismatch::UpdateApp { minimum } => {
            format!("Update this app: {host} needs Codync {minimum} or newer here.")
        }
        crate::compat::Mismatch::UpdateHost { version, minimum } => format!(
            "Update Codync on {host}: it runs {version}, this app needs {minimum} or newer. ^k → Check for updates."
        ),
    }
}

/// OSC 52: the terminal puts it on the clipboard, over SSH too.
pub fn copy(text: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, text));
    let _ = out.flush();
}

fn page(h: usize) -> isize {
    isize::try_from(h.max(4)).unwrap_or(20) - 2
}

pub(super) fn cycle(i: usize, d: isize, n: usize) -> usize {
    let n = isize::try_from(n).unwrap_or(1).max(1);
    usize::try_from((isize::try_from(i).unwrap_or(0) + d).rem_euclid(n)).unwrap_or(0)
}

fn merge_toggles(have: &mut Vec<Toggle>, all: &[Toggle], on: bool) {
    for t in all {
        match have.iter_mut().find(|h| h.id == t.id) {
            Some(h) => h.name.clone_from(&t.name),
            None => have.push(Toggle { id: t.id.clone(), name: t.name.clone(), on }),
        }
    }
}

/// Case-insensitive substring match: for long fields where a subsequence matches almost anything.
fn contains(query: &str, fields: &[&str]) -> bool {
    let q = query.to_lowercase();
    fields.iter().any(|f| f.to_lowercase().contains(&q))
}

/// Subsequence match, case-insensitive, over any of the fields.
pub fn fuzzy(query: &str, fields: &[&str]) -> bool {
    let q: Vec<char> = query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return true;
    }
    fields.iter().any(|f| {
        let mut it = q.iter().peekable();
        for c in f.to_lowercase().chars() {
            if it.peek() == Some(&&c) {
                it.next();
            }
        }
        it.peek().is_none()
    })
}

pub fn tilde(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => path.to_owned(),
    }
}

#[cfg(test)]
fn test_app() -> App {
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    App::new(Client::new("http://127.0.0.1:1", None), tx, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_is_a_subsequence_match() {
        assert!(fuzzy("rx", &["Rex"]));
        assert!(fuzzy("", &["anything"]));
        assert!(!fuzzy("xr", &["Rex"]));
        assert!(fuzzy("web", &["Rex", "~/mycode/web"]));
    }

    #[test]
    fn lanes_split_the_chat_from_its_threads() {
        let g = Bot::parse(&json!({"id": "g", "kind": "group", "members": ["a", "b"], "workingThread": "r"}))
            .expect("a bot with an id parses");
        assert!(g.group && g.members == ["a", "b"]);
        assert!(g.works_in(Some("r")) && !g.works_in(None));
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new("http://127.0.0.1:1", None), tx, String::new());
        for (seq, thread) in [(1, Value::Null), (2, json!("r")), (3, Value::Null)] {
            let v = json!({"botId": "g", "id": format!("e{seq}"), "seq": seq, "turn": seq, "kind": "user", "threadId": thread});
            let (bot, e) = Entry::parse(&v).expect("a full entry parses");
            app.entries.entry(bot).or_default().insert(e.seq, e);
        }
        let ids = |app: &App| app.lane("g").iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&app), ["e1", "e3"]);
        assert_eq!(app.lane_turns("g"), [1, 3]);
        app.thread = Some("r".into());
        assert_eq!(ids(&app), ["e2"]);
        assert_eq!(app.lane_turns("g"), [2]);
    }
}
