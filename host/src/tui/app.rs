//! TUI state and everything that changes it: host events, keys, mouse, replies.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write as _;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

use super::net::Client;

pub enum Msg {
    Online(bool),
    Event(Value),
    Reply(After, Result<Value, String>),
    /// The stream is starting over from rev 0: drop what we have.
    Rewind,
}

/// What to do with a command's reply.
#[derive(Clone)]
pub enum After {
    Nothing,
    Hello,
    History(String),
    Thread,
    Dirs,
    Pairing,
    Backends,
    Saved,
    Connectors,
    Skills,
    Installed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Idle,
    Working,
    NeedsInput,
    Error,
}

/// What a bot's row shows, highest priority first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    Need,
    Error,
    Unread,
    Work,
    Idle,
}

impl Mark {
    pub fn priority(self) -> u8 {
        match self {
            Self::Need => 4,
            Self::Error => 3,
            Self::Unread => 2,
            Self::Work => 1,
            Self::Idle => 0,
        }
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools, reason = "mirrors the host's bot JSON")]
pub struct Bot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub shape: String,
    pub backend: String,
    pub cwd: String,
    pub auto: bool,
    pub model: Option<String>,
    pub pinned: bool,
    pub hidden: bool,
    pub notify: bool,
    pub connectors: Vec<String>,
    pub skills: Vec<String>,
    pub status: Status,
    pub activity: String,
    pub started_at: Option<i64>,
    pub unread: i64,
    pub last_message: String,
    pub last_at: i64,
    /// A group chat: several bots and the user, no agent of its own.
    pub group: bool,
    pub members: Vec<String>,
    /// Where the running turn talks: a chat (`None` = its own) and a thread in it.
    pub working_chat: Option<String>,
    pub working_thread: Option<String>,
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}

impl Bot {
    fn parse(v: &Value) -> Option<Self> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            name: s("name"),
            description: s("description"),
            color: v["avatarColor"].as_str().unwrap_or("blue").to_owned(),
            shape: v["avatarShape"].as_str().unwrap_or("blob").to_owned(),
            backend: s("backend"),
            cwd: s("cwd"),
            auto: v["permission"] == "auto",
            model: v["model"].as_str().filter(|m| !m.is_empty()).map(str::to_owned),
            pinned: v["pinned"].as_bool().unwrap_or(false),
            hidden: v["hidden"].as_bool().unwrap_or(false),
            notify: v["notify"].as_bool().unwrap_or(true),
            connectors: strs(&v["connectors"]),
            skills: strs(&v["skills"]),
            status: match v["status"].as_str() {
                Some("working") => Status::Working,
                Some("needsInput") => Status::NeedsInput,
                Some("error") => Status::Error,
                _ => Status::Idle,
            },
            activity: s("activity"),
            started_at: v["startedAt"].as_i64(),
            unread: v["unread"].as_i64().unwrap_or(0),
            last_message: s("lastMessage"),
            last_at: v["lastAt"].as_i64().unwrap_or(0),
            group: v["kind"] == "group",
            members: strs(&v["members"]),
            working_chat: v["workingChat"].as_str().map(str::to_owned),
            working_thread: v["workingThread"].as_str().map(str::to_owned),
        })
    }

    /// Whether the running turn talks in this chat: its main lane (`None`) or that thread.
    pub fn works_in(&self, thread: Option<&str>) -> bool {
        self.working_chat.as_deref().unwrap_or(&self.id) == self.id && self.working_thread.as_deref() == thread
    }

    pub fn mark(&self) -> Mark {
        match self.status {
            Status::NeedsInput => Mark::Need,
            Status::Error => Mark::Error,
            Status::Working => Mark::Work,
            Status::Idle if self.unread > 0 => Mark::Unread,
            Status::Idle => Mark::Idle,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    User,
    Agent,
    Thought,
    Tool,
    Plan,
    Permission,
    Notice,
    Other,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub seq: i64,
    pub turn: i64,
    pub kind: Kind,
    pub data: Value,
    pub created_at: i64,
    /// The main-chat message whose thread this is in; `None` is the main chat.
    pub thread_id: Option<String>,
}

impl Entry {
    fn parse(v: &Value) -> Option<(String, Self)> {
        let kind = match v["kind"].as_str()? {
            "user" => Kind::User,
            "agent" => Kind::Agent,
            "thought" => Kind::Thought,
            "tool" => Kind::Tool,
            "plan" => Kind::Plan,
            "permission" => Kind::Permission,
            "notice" => Kind::Notice,
            _ => Kind::Other,
        };
        Some((
            v["botId"].as_str()?.to_owned(),
            Self {
                id: v["id"].as_str()?.to_owned(),
                seq: v["seq"].as_i64()?,
                turn: v["turn"].as_i64().unwrap_or(0),
                kind,
                data: v["data"].clone(),
                created_at: v["createdAt"].as_i64().unwrap_or(0),
                thread_id: v["threadId"].as_str().map(str::to_owned),
            },
        ))
    }

    pub fn text(&self) -> &str {
        self.data["text"].as_str().unwrap_or_default()
    }

    pub fn is_final(&self) -> bool {
        self.kind == Kind::Agent && self.data["final"].as_bool().unwrap_or(false)
    }

    /// What the chat shows as a message: the user's, and each turn's final reply.
    pub fn is_message(&self) -> bool {
        self.kind == Kind::User || self.is_final()
    }

    pub fn pending(&self) -> bool {
        self.kind == Kind::Permission && self.data["status"] == "pending"
    }

    /// Approval options in the apps' order: allow once, allow always, reject once, reject always.
    pub fn options(&self) -> Vec<(String, String, String)> {
        let rank = |k: &str| match k {
            "allow_once" => 0,
            "allow_always" => 1,
            "reject_once" => 2,
            "reject_always" => 3,
            _ => 9,
        };
        let mut o: Vec<(String, String, String)> = self.data["options"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|o| {
                let s = |k: &str| o[k].as_str().unwrap_or_default().to_owned();
                (s("optionId"), s("name"), s("kind"))
            })
            .collect();
        o.sort_by_key(|(_, _, k)| rank(k));
        o
    }
}

/// A one-line-or-more text field with readline-style editing.
#[derive(Clone, Default, Debug)]
pub struct Editor {
    pub text: String,
    /// Byte offset, always on a char boundary.
    pub cursor: usize,
}

impl Editor {
    pub fn with(text: &str) -> Self {
        Self { text: text.to_owned(), cursor: text.len() }
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    fn prev(&self, i: usize) -> usize {
        self.text[..i].char_indices().next_back().map_or(0, |(j, _)| j)
    }

    fn next(&self, i: usize) -> usize {
        self.text[i..].chars().next().map_or(i, |c| i + c.len_utf8())
    }

    fn word_left(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && self.text[..i].ends_with(char::is_whitespace) {
            i = self.prev(i);
        }
        while i > 0 && !self.text[..i].ends_with(char::is_whitespace) {
            i = self.prev(i);
        }
        i
    }

    fn word_right(&self) -> usize {
        let mut i = self.cursor;
        while i < self.text.len() && self.text[i..].starts_with(char::is_whitespace) {
            i = self.next(i);
        }
        while i < self.text.len() && !self.text[i..].starts_with(char::is_whitespace) {
            i = self.next(i);
        }
        i
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..].find('\n').map_or(self.text.len(), |i| self.cursor + i)
    }

    /// Editing keys shared by every text field. Returns whether the key was used.
    pub fn key(&mut self, k: KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::Char('a') if ctrl => self.cursor = self.line_start(),
            KeyCode::Char('e') if ctrl => self.cursor = self.line_end(),
            KeyCode::Char('b') if ctrl => self.cursor = self.prev(self.cursor),
            KeyCode::Char('f') if ctrl => self.cursor = self.next(self.cursor),
            KeyCode::Char('b') if alt => self.cursor = self.word_left(),
            KeyCode::Char('f') if alt => self.cursor = self.word_right(),
            KeyCode::Char('w') if ctrl => self.cut(self.word_left(), self.cursor),
            KeyCode::Backspace if alt || ctrl => self.cut(self.word_left(), self.cursor),
            KeyCode::Char('d') if alt => self.cut(self.cursor, self.word_right()),
            KeyCode::Char('u') if ctrl => self.cut(self.line_start(), self.cursor),
            KeyCode::Char('h') if ctrl => self.cut(self.prev(self.cursor), self.cursor),
            KeyCode::Char('d') if ctrl => self.cut(self.cursor, self.next(self.cursor)),
            KeyCode::Left if alt || ctrl => self.cursor = self.word_left(),
            KeyCode::Right if alt || ctrl => self.cursor = self.word_right(),
            KeyCode::Left => self.cursor = self.prev(self.cursor),
            KeyCode::Right => self.cursor = self.next(self.cursor),
            KeyCode::Home => self.cursor = self.line_start(),
            KeyCode::End => self.cursor = self.line_end(),
            KeyCode::Backspace => self.cut(self.prev(self.cursor), self.cursor),
            KeyCode::Delete => self.cut(self.cursor, self.next(self.cursor)),
            KeyCode::Char(c) if !ctrl && !alt => self.insert(c.encode_utf8(&mut [0; 4])),
            KeyCode::Char(c) if alt && !ctrl && !c.is_ascii_alphabetic() => self.insert(c.encode_utf8(&mut [0; 4])),
            _ => return false,
        }
        true
    }

    fn cut(&mut self, from: usize, to: usize) {
        if from < to {
            self.text.replace_range(from..to, "");
            self.cursor = from;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Roster,
    Chat,
    Trace,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraceMode {
    Off,
    Pane,
    Full,
}

/// Width class, set by the last draw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Width {
    /// < 80 columns: roster and chat are separate pages.
    Narrow,
    /// 80–99: compact roster.
    Mid,
    /// 100–149.
    Wide,
    /// ≥ 150.
    Huge,
}

pub const FILTERS: [(&str, Option<Mark>); 5] = [
    ("all", None),
    ("needs you", Some(Mark::Need)),
    ("working", Some(Mark::Work)),
    ("unread", Some(Mark::Unread)),
    ("error", Some(Mark::Error)),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    NewBot,
    NewGroup,
    RefreshAgents,
    Pair,
    Usage,
    Keys,
}

pub const ACTIONS: [(&str, &str, Action); 6] = [
    ("New bot…", "n", Action::NewBot),
    ("New group chat…", "m", Action::NewGroup),
    ("Refresh agents", "", Action::RefreshAgents),
    ("Pair a phone…", "P", Action::Pair),
    ("Usage", "U", Action::Usage),
    ("Keys", "?", Action::Keys),
];

pub enum GotoItem {
    Bot(String),
    Action(Action),
}

pub struct Goto {
    pub query: Editor,
    pub filter: usize,
    pub cursor: usize,
}

pub enum ConfirmAct {
    Delete(String),
    NewSession(String),
}

pub struct Confirm {
    pub title: String,
    pub detail: String,
    pub note: String,
    pub button: &'static str,
    pub act: ConfirmAct,
}

pub struct AgentPicker {
    pub query: Editor,
    pub cursor: usize,
}

#[derive(Clone)]
pub struct Dir {
    pub name: String,
    pub path: String,
    pub git: bool,
}

pub struct FolderPicker {
    pub path: String,
    pub parent: Option<String>,
    pub dirs: Vec<Dir>,
    pub query: Editor,
    pub cursor: usize,
    pub error: Option<String>,
    /// `Some(backend)`: step 2 of a new bot. `None`: picking for the open form.
    pub new_bot: Option<String>,
}

pub const COLORS: [&str; 11] =
    ["black", "brown", "red", "orange", "yellow", "green", "cyan", "blue", "violet", "magenta", "gray"];
pub const SHAPES: [&str; 8] = ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Name,
    Instructions,
    Agent,
    Model,
    Folder,
    Approvals,
    Notify,
    Color,
    Shape,
    Connectors,
    Skills,
}

pub const FIELDS: [Field; 11] = [
    Field::Name,
    Field::Instructions,
    Field::Agent,
    Field::Model,
    Field::Folder,
    Field::Approvals,
    Field::Notify,
    Field::Color,
    Field::Shape,
    Field::Connectors,
    Field::Skills,
];

pub struct Toggle {
    pub id: String,
    pub name: String,
    pub on: bool,
}

pub struct Form {
    pub bot_id: Option<String>,
    pub name: Editor,
    pub instructions: Editor,
    pub backend: String,
    pub model: Editor,
    pub cwd: String,
    pub auto: bool,
    pub notify: bool,
    pub color: usize,
    pub shape: usize,
    pub connectors: Vec<Toggle>,
    pub skills: Vec<Toggle>,
    pub field: usize,
    /// Cursor inside the connector / skill toggles.
    pub sub: usize,
    pub error: Option<String>,
    pub saving: bool,
}

impl Form {
    pub fn current(&self) -> Field {
        FIELDS[self.field]
    }
}

/// Create a group chat, or rename one and change who's in it.
pub struct GroupForm {
    /// The group being edited; `None` creates one.
    pub group_id: Option<String>,
    pub name: Editor,
    /// Picked bot ids, in the order they were picked.
    pub members: Vec<String>,
    /// Typing the name (else moving in the bot list).
    pub on_name: bool,
    pub cursor: usize,
    pub error: Option<String>,
    pub saving: bool,
}

pub enum Overlay {
    Goto(Goto),
    Help(Editor),
    Confirm(Confirm),
    Agents(AgentPicker),
    Folder(FolderPicker),
    Form(Box<Form>),
    Group(GroupForm),
    Usage,
    Pair(Option<String>),
}

pub struct Toast {
    pub text: String,
    pub need: bool,
    pub until: Instant,
}

#[derive(Clone)]
pub enum Click {
    Bot(String),
    Option {
        entry: String,
        option: String,
    },
    Filter(usize),
    Composer,
    /// The replies line under a message: opens its thread.
    Thread(String),
}

#[derive(Default)]
pub struct Hits {
    pub roster: Rect,
    pub chat: Rect,
    pub trace: Rect,
    pub clicks: Vec<(Rect, Click)>,
}

#[allow(clippy::struct_excessive_bools, reason = "independent UI flags, not a state machine")]
pub struct App {
    pub client: Client,
    tx: UnboundedSender<Msg>,
    pub url: String,
    pub host: String,
    pub home: String,
    pub online: bool,
    pub error: Option<String>,
    pub bots: HashMap<String, Bot>,
    pub entries: HashMap<String, BTreeMap<i64, Entry>>,
    pub usage: Value,
    pub backends: Vec<Value>,
    pub selected: Option<String>,
    pub focus: Focus,
    pub typing: bool,
    /// Narrow layout: showing the chat page (else the roster page).
    pub chat_page: bool,
    pub drafts: HashMap<String, Editor>,
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
    reading: HashSet<String>,
    history_busy: HashSet<String>,
    history_done: HashSet<String>,
    pub hint: Option<(String, Instant)>,
}

impl App {
    pub fn new(client: Client, tx: UnboundedSender<Msg>, url: String) -> Self {
        Self {
            client,
            tx,
            url,
            host: String::new(),
            home: String::new(),
            online: false,
            error: None,
            bots: HashMap::new(),
            entries: HashMap::new(),
            usage: Value::Null,
            backends: vec![],
            selected: None,
            focus: Focus::Roster,
            typing: false,
            chat_page: false,
            drafts: HashMap::new(),
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
            reading: HashSet::new(),
            history_busy: HashSet::new(),
            history_done: HashSet::new(),
            hint: None,
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

    /// Name of an entry's author (a group reply's bot), as the apps show it.
    pub fn author_name(&self, id: Option<&str>) -> String {
        id.and_then(|id| self.bots.get(id)).map_or_else(|| "A deleted bot".into(), |b| b.name.clone())
    }

    pub fn latest_turn(&self, bot: &str) -> Option<i64> {
        self.entries.get(bot)?.values().map(|e| e.turn).max()
    }

    /// Whether every older entry of this bot is loaded.
    pub fn history_complete(&self, id: &str) -> bool {
        self.history_done.contains(id) || self.oldest_main(id).is_none_or(|s| s <= 1)
    }

    /// Paging (`history`) covers the main chat only; thread replies don't count.
    fn oldest_main(&self, id: &str) -> Option<i64> {
        self.entries.get(id)?.values().find(|e| e.thread_id.is_none()).map(|e| e.seq)
    }

    pub fn chat_visible(&self) -> bool {
        self.selected.is_some() && (self.width != Width::Narrow || self.chat_page) && self.trace != TraceMode::Full
    }

    pub fn animating(&self) -> bool {
        !self.toasts.is_empty() || self.hint.is_some() || self.bots.values().any(|b| b.status == Status::Working)
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
        let ids: Vec<(String, Mark)> = self.roster().iter().map(|b| (b.id.clone(), b.mark())).collect();
        let start = self.selected.as_ref().and_then(|s| ids.iter().position(|(x, _)| x == s)).map_or(0, |i| i + 1);
        let found = (0..ids.len()).map(|k| &ids[(start + k) % ids.len()]).find(|(_, m)| *m == want).cloned();
        match found {
            Some((id, _)) => {
                self.select(&id);
                self.chat_page = true;
                self.focus = Focus::Chat;
                // A card waiting in a thread shows there.
                let here = self.bots.get(&id).filter(|b| b.working_chat.as_deref().unwrap_or(&b.id) == b.id);
                if let Some(root) = here.and_then(|b| b.working_thread.clone()) {
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
            self.call("markRead", json!({"botId": id}), After::Nothing);
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
        let Some(oldest) = self.oldest_main(&id) else { return };
        if oldest <= 1 {
            self.history_done.insert(id);
            return;
        }
        self.history_busy.insert(id.clone());
        self.call("history", json!({"botId": id, "beforeSeq": oldest, "limit": 200}), After::History(id));
    }

    // ---------- host messages ----------

    pub fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Online(on) => {
                self.online = on;
                if !on && self.error.is_none() && self.bots.is_empty() {
                    // Say why instead of spinning on "Connecting…".
                    self.call("hello", json!({}), After::Hello);
                }
                if on {
                    self.error = None;
                    if self.host.is_empty() {
                        self.call("hello", json!({}), After::Hello);
                    }
                }
            }
            Msg::Rewind => {
                self.bots.clear();
                self.entries.clear();
                self.history_done.clear();
            }
            Msg::Event(v) => self.on_event(&v),
            Msg::Reply(after, r) => self.on_reply(after, r),
        }
    }

    fn on_event(&mut self, v: &Value) {
        match v["type"].as_str() {
            Some("hello") if !v["usage"].is_null() => self.usage = v["usage"].clone(),
            Some("usage") => self.usage = v["usage"].clone(),
            Some("bot") => self.upsert_bot(&v["bot"]),
            Some("entry") => {
                if let Some((bot, e)) = Entry::parse(&v["entry"]) {
                    self.entries.entry(bot).or_default().insert(e.seq, e);
                }
            }
            _ => {}
        }
    }

    fn upsert_bot(&mut self, v: &Value) {
        let Some(id) = v["id"].as_str() else { return };
        if v["deleted"].as_bool().unwrap_or(false) {
            self.bots.remove(id);
            self.entries.remove(id);
            if self.selected.as_deref() == Some(id) {
                self.selected = None;
                self.thread = None;
                self.pick = None;
                self.chat_page = false;
                self.typing = false;
            }
            return;
        }
        let Some(new) = Bot::parse(v) else { return };
        if new.unread == 0 {
            self.reading.remove(&new.id);
        }
        let old = self.bots.insert(new.id.clone(), new.clone());
        let Some(old) = old else { return };
        let watching = self.selected.as_deref() == Some(new.id.as_str()) && self.chat_visible() && self.term_focused;
        if !new.notify || watching {
            return;
        }
        if new.status == Status::NeedsInput && old.status != Status::NeedsInput {
            self.toast(format!("{} needs you", new.name), true);
        } else if old.status == Status::Working && new.status == Status::Idle {
            self.toast(format!("{} is done", new.name), false);
        } else if old.status != Status::Error && new.status == Status::Error {
            self.toast(format!("{} hit an error", new.name), true);
        }
    }

    fn on_reply(&mut self, after: After, r: Result<Value, String>) {
        let v = match r {
            Ok(v) => v,
            Err(e) => {
                match after {
                    After::Hello => self.error = Some(e),
                    After::History(id) => {
                        self.history_busy.remove(&id);
                    }
                    After::Saved => match self.overlays.last_mut() {
                        Some(Overlay::Form(f)) => {
                            f.saving = false;
                            f.error = Some(e);
                        }
                        Some(Overlay::Group(g)) => {
                            g.saving = false;
                            g.error = Some(e);
                        }
                        _ => {}
                    },
                    After::Dirs => {
                        if let Some(Overlay::Folder(p)) = self.overlays.last_mut() {
                            p.error = Some(e);
                        }
                    }
                    _ => self.flash(&e),
                }
                return;
            }
        };
        match after {
            After::Nothing => {}
            After::Installed => {
                self.error = None;
                self.flash("Host installed; connecting…");
                self.call("hello", json!({}), After::Hello);
            }
            After::Hello => {
                v["name"].as_str().unwrap_or("computer").clone_into(&mut self.host);
                v["home"].as_str().unwrap_or_default().clone_into(&mut self.home);
                self.backends = v["backends"].as_array().cloned().unwrap_or_default();
            }
            After::Backends => {
                self.backends = v["backends"].as_array().cloned().unwrap_or_default();
                self.flash("Agents refreshed");
            }
            After::Thread => {
                for e in v["entries"].as_array().into_iter().flatten() {
                    if let Some((bot, e)) = Entry::parse(e) {
                        self.entries.entry(bot).or_default().entry(e.seq).or_insert(e);
                    }
                }
            }
            After::History(id) => {
                self.history_busy.remove(&id);
                let list = v["entries"].as_array().cloned().unwrap_or_default();
                if list.is_empty() {
                    self.history_done.insert(id);
                }
                for e in &list {
                    if let Some((bot, e)) = Entry::parse(e) {
                        self.entries.entry(bot).or_default().entry(e.seq).or_insert(e);
                    }
                }
            }
            After::Dirs => {
                if let Some(Overlay::Folder(p)) = self.overlays.last_mut() {
                    v["path"].as_str().unwrap_or_default().clone_into(&mut p.path);
                    p.parent = v["parent"].as_str().map(str::to_owned);
                    p.dirs = v["dirs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|d| Dir {
                            name: d["name"].as_str().unwrap_or_default().to_owned(),
                            path: d["path"].as_str().unwrap_or_default().to_owned(),
                            git: d["isGit"].as_bool().unwrap_or(false),
                        })
                        .collect();
                    p.query.clear();
                    p.cursor = 0;
                    p.error = None;
                }
            }
            After::Pairing => {
                if let Some(Overlay::Pair(url)) = self.overlays.last_mut() {
                    *url = v["pairingUrl"].as_str().map(str::to_owned);
                }
            }
            After::Saved => {
                let saved = self.overlays.iter().rposition(|o| matches!(o, Overlay::Form(_) | Overlay::Group(_)));
                if let Some(i) = saved {
                    self.overlays.truncate(i);
                }
                if let Some(id) = v["bot"]["id"].as_str() {
                    let id = id.to_owned();
                    self.upsert_bot(&v["bot"]);
                    self.select(&id);
                    self.chat_page = true;
                    self.focus = Focus::Chat;
                    self.typing = true;
                }
            }
            After::Connectors | After::Skills => {
                let list: Vec<Toggle> = v["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| {
                        let id = c["id"].as_str()?.to_owned();
                        let name = c["name"].as_str().unwrap_or(&id).to_owned();
                        Some(Toggle { id, name, on: false })
                    })
                    .collect();
                let connectors = matches!(after, After::Connectors);
                if connectors {
                    self.market.0 = list;
                } else {
                    self.market.1 = list;
                }
                for o in &mut self.overlays {
                    if let Overlay::Form(f) = o {
                        if connectors {
                            merge_toggles(&mut f.connectors, &self.market.0);
                        } else {
                            merge_toggles(&mut f.skills, &self.market.1);
                        }
                    }
                }
            }
        }
    }

    // ---------- input ----------

    pub fn on_paste(&mut self, s: &str) {
        let s = s.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(ed) = self.top_editor() {
            ed.insert(&s);
        } else if let Some(key) = self.draft_key() {
            self.typing = true;
            self.focus = Focus::Chat;
            self.drafts.entry(key).or_default().insert(&s);
        }
    }

    fn top_editor(&mut self) -> Option<&mut Editor> {
        match self.overlays.last_mut()? {
            Overlay::Goto(g) => Some(&mut g.query),
            Overlay::Help(e) => Some(e),
            Overlay::Agents(a) => Some(&mut a.query),
            Overlay::Folder(p) => Some(&mut p.query),
            Overlay::Group(g) if g.on_name => Some(&mut g.name),
            Overlay::Form(f) => match f.current() {
                Field::Name => Some(&mut f.name),
                Field::Instructions => Some(&mut f.instructions),
                Field::Model => Some(&mut f.model),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('k') {
            if matches!(self.overlays.last(), Some(Overlay::Goto(_))) {
                self.overlays.pop();
            } else {
                self.overlays.push(Overlay::Goto(Goto { query: Editor::default(), filter: 0, cursor: 0 }));
            }
            return;
        }
        if !self.overlays.is_empty() {
            self.overlay_key(k);
            return;
        }
        if self.typing {
            self.composer_key(k);
            return;
        }
        self.nav_key(k);
    }

    fn composer_key(&mut self, k: KeyEvent) {
        let (Some(id), Some(key)) = (self.selected.clone(), self.draft_key()) else {
            self.typing = false;
            return;
        };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let newline = k.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT);
        let thread = self.thread.clone();
        let draft = self.drafts.entry(key).or_default();
        match k.code {
            KeyCode::Esc => self.typing = false,
            KeyCode::Enter if newline => draft.insert("\n"),
            KeyCode::Char('j') if ctrl => draft.insert("\n"),
            KeyCode::Enter => {
                let text = draft.text.trim().to_owned();
                if text.is_empty() {
                    return;
                }
                draft.clear();
                self.chat_scroll = 0;
                let nonce = uuid::Uuid::new_v4().to_string();
                let mut body = json!({"botId": id, "text": text, "clientNonce": nonce});
                if let Some(root) = thread {
                    body["threadId"] = root.into();
                }
                self.call("send", body, After::Nothing);
            }
            KeyCode::Char('c') if ctrl => {
                if draft.text.is_empty() {
                    self.flash("esc then s stops the bot");
                } else {
                    draft.clear();
                }
            }
            KeyCode::Up if draft.text.is_empty() => self.scroll_chat(1),
            KeyCode::Down if draft.text.is_empty() => self.scroll_chat(-1),
            KeyCode::PageUp => self.scroll_chat(page(self.chat_height)),
            KeyCode::PageDown => self.scroll_chat(-page(self.chat_height)),
            KeyCode::Up => move_line(draft, -1),
            KeyCode::Down => move_line(draft, 1),
            _ => {
                draft.key(k);
            }
        }
    }

    fn scroll_chat(&mut self, delta: isize) {
        self.chat_scroll = self.chat_scroll.saturating_add_signed(delta);
    }

    fn scroll_trace(&mut self, delta: isize) {
        self.trace_scroll = self.trace_scroll.saturating_add_signed(delta);
    }

    fn nav_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let half = page(self.chat_height) / 2;
        let narrow = self.width == Width::Narrow;
        let roster_page = narrow && !self.chat_page;
        if self.pick.is_some() && self.pick_key(k) {
            return;
        }
        match k.code {
            KeyCode::Char('c') if ctrl => self.flash("q quits"),
            KeyCode::Char('d') if ctrl => self.scroll_focused(-half),
            KeyCode::Char('u') if ctrl => self.scroll_focused(half),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('I') if self.error.is_some() && self.bots.is_empty() => self.install_host(),
            KeyCode::Char('?') => self.overlays.push(Overlay::Help(Editor::default())),
            KeyCode::Char('!') => self.jump(Mark::Need),
            KeyCode::Char('u') => self.jump(Mark::Unread),
            KeyCode::Char('[') => self.move_bot(-1),
            KeyCode::Char(']') => self.move_bot(1),
            KeyCode::Char(c @ '1'..='9') => {
                let i = (c as usize) - ('1' as usize);
                let pending = if roster_page { None } else { self.pending().map(|e| (e.id.clone(), e.options())) };
                match pending {
                    Some((entry, opts)) if !opts.is_empty() => {
                        if let Some((option, _, _)) = opts.get(i) {
                            self.answer(&entry, option);
                        }
                    }
                    _ => {
                        if let Some(id) = self.roster().get(i).map(|b| b.id.clone()) {
                            self.select(&id);
                        }
                    }
                }
            }
            KeyCode::Char(c @ ('y' | 'a' | 'n' | 'N')) if !roster_page && self.pending().is_some() => {
                self.answer_kind(c);
            }
            KeyCode::Char('n') => self.new_bot(),
            KeyCode::Char('m') => self.new_group(),
            KeyCode::Char('i') if !roster_page => self.start_typing(),
            KeyCode::Char('r') if !roster_page && self.selected.is_some() => {
                if self.thread.is_some() {
                    self.start_typing();
                } else {
                    self.chat_page = true;
                    self.focus = Focus::Chat;
                    self.step_pick(0);
                }
            }
            KeyCode::Enter => {
                if roster_page || self.focus == Focus::Roster {
                    if self.selected.is_some() {
                        self.chat_page = true;
                        self.focus = Focus::Chat;
                        self.start_typing();
                    }
                } else {
                    self.start_typing();
                }
            }
            KeyCode::Esc | KeyCode::Backspace => {
                if self.trace == TraceMode::Full {
                    self.trace = TraceMode::Off;
                    self.focus = Focus::Chat;
                } else if self.thread.is_some() {
                    self.thread = None;
                    self.chat_scroll = 0;
                } else if narrow && self.chat_page {
                    self.chat_page = false;
                    self.focus = Focus::Roster;
                } else if self.focus == Focus::Trace {
                    self.focus = Focus::Chat;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => self.cycle_focus(k.code == KeyCode::BackTab),
            KeyCode::Char('h') | KeyCode::Left => self.focus_side(-1),
            KeyCode::Char('l') | KeyCode::Right => self.focus_side(1),
            KeyCode::Char('j') | KeyCode::Down => self.line(1),
            KeyCode::Char('k') | KeyCode::Up => self.line(-1),
            KeyCode::PageDown => self.scroll_focused(-page(self.chat_height)),
            KeyCode::PageUp => self.scroll_focused(page(self.chat_height)),
            KeyCode::Char('g') | KeyCode::Home => {
                if self.focus == Focus::Trace {
                    self.trace_scroll = 0;
                } else {
                    self.chat_scroll = usize::MAX / 2;
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if self.focus == Focus::Trace {
                    self.trace_scroll = usize::MAX / 2;
                } else {
                    self.chat_scroll = 0;
                }
            }
            KeyCode::Char('{') => self.step_turn(-1),
            KeyCode::Char('}') => self.step_turn(1),
            KeyCode::Char('t') => {
                self.trace = if self.trace == TraceMode::Pane { TraceMode::Off } else { TraceMode::Pane };
                if self.trace == TraceMode::Off && self.focus == Focus::Trace {
                    self.focus = Focus::Chat;
                }
                if self.width == Width::Narrow || self.width == Width::Mid {
                    self.trace = if self.trace == TraceMode::Off { TraceMode::Off } else { TraceMode::Full };
                }
            }
            KeyCode::Char('T') => {
                self.trace = if self.trace == TraceMode::Full { TraceMode::Off } else { TraceMode::Full };
                self.focus = if self.trace == TraceMode::Full { Focus::Trace } else { Focus::Chat };
            }
            KeyCode::Char('o') => {
                let Some(id) = self.selected.clone() else { return };
                // The newest turn that did something.
                self.trace_turn = self
                    .entries
                    .get(&id)
                    .and_then(|m| m.values().rev().find(|e| matches!(e.kind, Kind::Tool | Kind::Plan)).map(|e| e.turn));
                self.trace_scroll = 0;
                if self.trace == TraceMode::Off {
                    self.trace = if matches!(self.width, Width::Narrow | Width::Mid) {
                        TraceMode::Full
                    } else {
                        TraceMode::Pane
                    };
                }
                self.focus = Focus::Trace;
            }
            KeyCode::Char('b') => self.compact = !self.compact,
            KeyCode::Char('U') => self.overlays.push(Overlay::Usage),
            KeyCode::Char('P') => self.open_pair(),
            KeyCode::Char('s') => {
                if let Some(b) = self.bot() {
                    if matches!(b.status, Status::Working | Status::NeedsInput) {
                        let id = b.id.clone();
                        self.call("stop", json!({"botId": id}), After::Nothing);
                        self.flash("Stopping…");
                    } else {
                        self.flash("Not working right now");
                    }
                }
            }
            KeyCode::Char('S') => {
                if self.bot().is_some_and(|b| b.group) {
                    self.flash("A group has no session of its own");
                } else if let Some(b) = self.bot() {
                    let c = Confirm {
                        title: format!("Start a new session for {}?", b.name),
                        detail: "The agent forgets this conversation's context.".into(),
                        note: "The transcript stays here.".into(),
                        button: "New session",
                        act: ConfirmAct::NewSession(b.id.clone()),
                    };
                    self.overlays.push(Overlay::Confirm(c));
                }
            }
            KeyCode::Char('x') => {
                if let Some(b) = self.bot() {
                    let (detail, note) = if b.group {
                        ("Its conversation goes away.".into(), "Its bots stay.".into())
                    } else {
                        (
                            "Its conversation and settings go away.".into(),
                            format!("Files in {} stay where they are.", tilde(&b.cwd, &self.home)),
                        )
                    };
                    let c = Confirm {
                        title: format!("Delete {}?", b.name),
                        detail,
                        note,
                        button: "Delete",
                        act: ConfirmAct::Delete(b.id.clone()),
                    };
                    self.overlays.push(Overlay::Confirm(c));
                }
            }
            KeyCode::Char('e') => self.edit_bot(),
            KeyCode::Char('p') => {
                if let Some(b) = self.bot() {
                    let (id, pinned) = (b.id.clone(), !b.pinned);
                    self.call("updateBot", json!({"id": id, "pinned": pinned}), After::Nothing);
                }
            }
            KeyCode::Char('c') => self.copy_last(),
            _ => {}
        }
    }

    /// `codync-host install` with this same binary, for when nothing is listening yet.
    fn install_host(&mut self) {
        let Ok(exe) = std::env::current_exe() else { return };
        self.flash("Installing the host…");
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let out = tokio::process::Command::new(exe).arg("install").output().await;
            let r = match out {
                Ok(o) if o.status.success() => Ok(Value::Null),
                Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_owned()),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(Msg::Reply(After::Installed, r));
        });
    }

    fn start_typing(&mut self) {
        if self.selected.is_some() {
            self.typing = true;
            self.chat_page = true;
            self.focus = Focus::Chat;
            if self.trace == TraceMode::Full {
                self.trace = TraceMode::Off;
            }
        }
    }

    fn line(&mut self, d: isize) {
        match self.focus {
            Focus::Roster => self.move_bot(d),
            Focus::Chat if self.width == Width::Narrow && !self.chat_page => self.move_bot(d),
            Focus::Chat => self.scroll_chat(-d),
            Focus::Trace => self.scroll_trace(d),
        }
    }

    fn scroll_focused(&mut self, up: isize) {
        if self.focus == Focus::Trace {
            self.scroll_trace(-up);
        } else {
            self.scroll_chat(up);
        }
    }

    fn panes(&self) -> Vec<Focus> {
        let mut p = vec![];
        if self.width != Width::Narrow {
            p.push(Focus::Roster);
        }
        if self.trace != TraceMode::Full {
            p.push(Focus::Chat);
        }
        if self.trace != TraceMode::Off {
            p.push(Focus::Trace);
        }
        p
    }

    fn cycle_focus(&mut self, back: bool) {
        let p = self.panes();
        let i = p.iter().position(|f| *f == self.focus).unwrap_or(0);
        let n = p.len();
        self.focus = p[if back { (i + n - 1) % n } else { (i + 1) % n }];
    }

    fn focus_side(&mut self, d: isize) {
        let p = self.panes();
        let i = p.iter().position(|f| *f == self.focus).unwrap_or(0);
        let j = i.saturating_add_signed(d).min(p.len() - 1);
        self.focus = p[j];
    }

    fn step_turn(&mut self, d: i64) {
        let Some(id) = self.selected.clone() else { return };
        let Some(latest) = self.latest_turn(&id) else { return };
        let cur = self.trace_turn.unwrap_or(latest);
        let next = (cur + d).clamp(1, latest);
        self.trace_turn = if next == latest { None } else { Some(next) };
        self.trace_scroll = 0;
        if self.trace == TraceMode::Off {
            self.trace =
                if matches!(self.width, Width::Narrow | Width::Mid) { TraceMode::Full } else { TraceMode::Pane };
        }
    }

    fn answer(&mut self, entry: &str, option: &str) {
        self.call("respondPermission", json!({"entryId": entry, "optionId": option}), After::Nothing);
    }

    fn answer_kind(&mut self, key: char) {
        let Some(e) = self.pending() else { return };
        let (entry, opts) = (e.id.clone(), e.options());
        let want: &[&str] = match key {
            'y' => &["allow_once", "allow_always"],
            'a' => &["allow_always", "allow_once"],
            _ => &["reject_once", "reject_always"],
        };
        let pick = want.iter().find_map(|k| opts.iter().find(|(_, _, kind)| kind == k));
        match pick {
            Some((option, _, _)) => {
                let option = option.clone();
                self.answer(&entry, &option);
                if key == 'N' {
                    self.start_typing();
                }
            }
            None => self.flash("That choice isn't offered; use 1–9"),
        }
    }

    /// `r`: moves the pick through the main chat's messages (none picked: the newest).
    fn step_pick(&mut self, d: isize) {
        let Some(id) = self.selected.clone() else { return };
        let ids: Vec<String> = self.lane(&id).into_iter().filter(|e| e.is_message()).map(|e| e.id.clone()).collect();
        if ids.is_empty() {
            self.pick = None;
            self.flash("No message to reply to yet");
            return;
        }
        let at = self.pick.as_ref().and_then(|p| ids.iter().position(|x| x == p));
        let i = at.map_or(ids.len() - 1, |i| i.saturating_add_signed(d).min(ids.len() - 1));
        self.pick = Some(ids[i].clone());
    }

    /// Keys while a message is picked. Returns whether the key was used.
    fn pick_key(&mut self, k: KeyEvent) -> bool {
        match k.code {
            KeyCode::Char('j') | KeyCode::Down => self.step_pick(1),
            KeyCode::Char('k') | KeyCode::Up => self.step_pick(-1),
            KeyCode::Enter | KeyCode::Char('r') => {
                if let Some(root) = self.pick.take() {
                    self.open_thread(root);
                    self.start_typing();
                }
            }
            KeyCode::Esc => self.pick = None,
            _ => return false,
        }
        true
    }

    fn open_thread(&mut self, root: String) {
        let Some(id) = self.selected.clone() else { return };
        self.pick = None;
        self.chat_scroll = 0;
        self.chat_page = true;
        self.focus = Focus::Chat;
        if self.trace == TraceMode::Full {
            self.trace = TraceMode::Off;
        }
        self.call("thread", json!({"botId": id, "rootId": root}), After::Thread);
        self.thread = Some(root);
    }

    fn copy_last(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        let Some(text) = self.lane(&id).into_iter().rev().find(|e| e.is_final()).map(|e| e.text().to_owned()) else {
            self.flash("No reply to copy");
            return;
        };
        // OSC 52: the terminal puts it on the clipboard, over SSH too.
        let mut out = std::io::stdout();
        let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
        let _ = out.flush();
        self.flash("Copied the last reply");
    }

    fn open_pair(&mut self) {
        self.overlays.push(Overlay::Pair(None));
        self.call("pairing", json!({}), After::Pairing);
    }

    fn new_bot(&mut self) {
        if self.backends.is_empty() {
            self.call("hello", json!({}), After::Hello);
        }
        self.overlays.push(Overlay::Agents(AgentPicker { query: Editor::default(), cursor: 0 }));
    }

    fn new_group(&mut self) {
        self.overlays.push(Overlay::Group(GroupForm {
            group_id: None,
            name: Editor::default(),
            members: vec![],
            on_name: false,
            cursor: 0,
            error: None,
            saving: false,
        }));
    }

    /// Bots a group can hold: the roster's agents, plus hidden ones already in it.
    pub fn group_candidates(&self, members: &[String]) -> Vec<&Bot> {
        let mut v: Vec<&Bot> = self.roster().into_iter().filter(|b| !b.group).collect();
        v.extend(self.bots.values().filter(|b| b.hidden && !b.group && members.contains(&b.id)));
        v
    }

    /// "Alice, Bob" when no name is typed.
    pub fn group_default_name(&self, members: &[String]) -> String {
        let names: Vec<&str> = members.iter().filter_map(|m| self.bots.get(m)).map(|b| b.name.as_str()).collect();
        names.join(", ")
    }

    fn group_key(&mut self, mut g: GroupForm, k: KeyEvent) -> Option<Overlay> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let n = self.group_candidates(&g.members).len();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => self.save_group(&mut g),
            KeyCode::Char('s') if ctrl => self.save_group(&mut g),
            KeyCode::Tab | KeyCode::BackTab => g.on_name = !g.on_name,
            KeyCode::Down if g.on_name => g.on_name = false,
            KeyCode::Down => g.cursor = (g.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up if g.cursor == 0 => g.on_name = true,
            KeyCode::Up if !g.on_name => g.cursor -= 1,
            KeyCode::Char(' ') if !g.on_name => {
                let id = self.group_candidates(&g.members).get(g.cursor).map(|b| b.id.clone());
                if let Some(id) = id {
                    g.error = None;
                    if let Some(i) = g.members.iter().position(|m| *m == id) {
                        g.members.remove(i);
                    } else {
                        g.members.push(id);
                    }
                }
            }
            _ if g.on_name => {
                g.name.key(k);
            }
            _ => {}
        }
        Some(Overlay::Group(g))
    }

    fn save_group(&mut self, g: &mut GroupForm) {
        if g.saving {
            return;
        }
        if g.members.is_empty() {
            g.error = Some("Pick at least one bot.".into());
            return;
        }
        let typed = g.name.text.trim();
        let name = if typed.is_empty() { self.group_default_name(&g.members) } else { typed.to_owned() };
        let mut body = json!({"name": name, "members": g.members});
        g.saving = true;
        g.error = None;
        if let Some(id) = &g.group_id {
            body["id"] = id.clone().into();
            self.call("updateBot", body, After::Saved);
        } else {
            body["id"] = "".into();
            body["kind"] = "group".into();
            self.call("createBot", body, After::Saved);
        }
    }

    fn edit_bot(&mut self) {
        let Some(b) = self.bot().cloned() else { return };
        if b.group {
            self.overlays.push(Overlay::Group(GroupForm {
                group_id: Some(b.id.clone()),
                name: Editor::with(&b.name),
                members: b.members.clone(),
                on_name: true,
                cursor: 0,
                error: None,
                saving: false,
            }));
            return;
        }
        let mut f = Form {
            bot_id: Some(b.id.clone()),
            name: Editor::with(&b.name),
            instructions: Editor::with(&b.description),
            backend: b.backend.clone(),
            model: Editor::with(b.model.as_deref().unwrap_or_default()),
            cwd: b.cwd.clone(),
            auto: b.auto,
            notify: b.notify,
            color: COLORS.iter().position(|c| *c == b.color).unwrap_or(7),
            shape: SHAPES.iter().position(|s| *s == b.shape).unwrap_or(0),
            connectors: b.connectors.iter().map(|id| Toggle { id: id.clone(), name: id.clone(), on: true }).collect(),
            skills: b.skills.iter().map(|id| Toggle { id: id.clone(), name: id.clone(), on: true }).collect(),
            field: 0,
            sub: 0,
            error: None,
            saving: false,
        };
        merge_toggles(&mut f.connectors, &self.market.0);
        merge_toggles(&mut f.skills, &self.market.1);
        self.overlays.push(Overlay::Form(Box::new(f)));
        self.call("connectors", json!({}), After::Connectors);
        self.call("skills", json!({}), After::Skills);
    }

    /// Agents offered for a new bot: runnable ones on this computer first.
    pub fn agent_choices(&self, query: &str) -> Vec<&Value> {
        let mut v: Vec<&Value> = self
            .backends
            .iter()
            .filter(|b| b["available"].as_bool().unwrap_or(false) || b["registry"].is_string())
            .filter(|b| fuzzy(query, &[b["name"].as_str().unwrap_or_default(), b["id"].as_str().unwrap_or_default()]))
            .collect();
        v.sort_by_key(|b| !b["installed"].as_bool().unwrap_or(false));
        v
    }

    pub fn goto_items(&self, g: &Goto) -> Vec<GotoItem> {
        let q = g.query.text.trim();
        let want = FILTERS[g.filter].1;
        let mut bots: Vec<&Bot> = self
            .bots
            .values()
            .filter(|b| want.is_none_or(|m| b.mark() == m))
            .filter(|b| fuzzy(q, &[&b.name, &b.backend]) || contains(q, &[&b.cwd, &b.last_message]))
            .collect();
        bots.sort_by(|a, b| {
            b.mark().priority().cmp(&a.mark().priority()).then(b.last_at.cmp(&a.last_at)).then(a.name.cmp(&b.name))
        });
        let mut items: Vec<GotoItem> = bots.into_iter().map(|b| GotoItem::Bot(b.id.clone())).collect();
        if want.is_none() {
            items.extend(
                ACTIONS.iter().filter(|(label, _, _)| fuzzy(q, &[label])).map(|(_, _, a)| GotoItem::Action(*a)),
            );
        }
        items
    }

    fn run_action(&mut self, a: Action) {
        match a {
            Action::NewBot => self.new_bot(),
            Action::NewGroup => self.new_group(),
            Action::RefreshAgents => self.call("refreshBackends", json!({}), After::Backends),
            Action::Pair => self.open_pair(),
            Action::Usage => self.overlays.push(Overlay::Usage),
            Action::Keys => self.overlays.push(Overlay::Help(Editor::default())),
        }
    }

    fn overlay_key(&mut self, k: KeyEvent) {
        let Some(top) = self.overlays.pop() else { return };
        let depth = self.overlays.len();
        let keep = match top {
            Overlay::Goto(g) => self.goto_key(g, k),
            Overlay::Help(mut e) => {
                if k.code == KeyCode::Esc || k.code == KeyCode::Char('?') && e.text.is_empty() {
                    None
                } else {
                    if k.code != KeyCode::Char('/') || !e.text.is_empty() {
                        e.key(k);
                    }
                    Some(Overlay::Help(e))
                }
            }
            Overlay::Confirm(c) => match k.code {
                KeyCode::Enter => {
                    match &c.act {
                        ConfirmAct::Delete(id) => self.call("deleteBot", json!({"botId": id}), After::Nothing),
                        ConfirmAct::NewSession(id) => self.call("newSession", json!({"botId": id}), After::Nothing),
                    }
                    None
                }
                KeyCode::Esc | KeyCode::Char('q') => None,
                _ => Some(Overlay::Confirm(c)),
            },
            Overlay::Agents(a) => self.agents_key(a, k),
            Overlay::Folder(p) => self.folder_key(p, k),
            Overlay::Form(f) => self.form_key(f, k),
            Overlay::Group(g) => self.group_key(g, k),
            o @ (Overlay::Usage | Overlay::Pair(_)) => {
                if matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'U' | 'P')) {
                    None
                } else {
                    Some(o)
                }
            }
        };
        if let Some(o) = keep {
            // Anything the handler opened stays on top of the overlay it came from.
            self.overlays.insert(depth.min(self.overlays.len()), o);
        }
    }

    fn goto_key(&mut self, mut g: Goto, k: KeyEvent) -> Option<Overlay> {
        let items_len = self.goto_items(&g).len();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Tab => {
                g.filter = (g.filter + 1) % FILTERS.len();
                g.cursor = 0;
            }
            KeyCode::BackTab => {
                g.filter = (g.filter + FILTERS.len() - 1) % FILTERS.len();
                g.cursor = 0;
            }
            KeyCode::Down => g.cursor = (g.cursor + 1).min(items_len.saturating_sub(1)),
            KeyCode::Up => g.cursor = g.cursor.saturating_sub(1),
            KeyCode::Char('n' | 'j') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                g.cursor = (g.cursor + 1).min(items_len.saturating_sub(1));
            }
            KeyCode::Char('p') if k.modifiers.contains(KeyModifiers::CONTROL) => g.cursor = g.cursor.saturating_sub(1),
            KeyCode::Enter => {
                let items = self.goto_items(&g);
                match items.get(g.cursor) {
                    Some(GotoItem::Bot(id)) => {
                        let id = id.clone();
                        self.select(&id);
                        self.start_typing();
                    }
                    Some(GotoItem::Action(a)) => self.run_action(*a),
                    None => {}
                }
                return None;
            }
            _ => {
                if g.query.key(k) {
                    g.cursor = 0;
                }
            }
        }
        Some(Overlay::Goto(g))
    }

    fn agents_key(&mut self, mut a: AgentPicker, k: KeyEvent) -> Option<Overlay> {
        let n = self.agent_choices(&a.query.text).len();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Down => a.cursor = (a.cursor + 1).min(n.saturating_sub(1)),
            KeyCode::Up => a.cursor = a.cursor.saturating_sub(1),
            KeyCode::Enter => {
                let backend =
                    self.agent_choices(&a.query.text).get(a.cursor).and_then(|b| b["id"].as_str()).map(str::to_owned);
                if let Some(backend) = backend {
                    let start = self.bot().map(|b| parent_dir(&b.cwd)).unwrap_or_default();
                    self.overlays.push(Overlay::Agents(a));
                    self.open_folder(Some(backend), &start);
                    return None;
                }
            }
            _ => {
                if a.query.key(k) {
                    a.cursor = 0;
                }
            }
        }
        Some(Overlay::Agents(a))
    }

    fn open_folder(&mut self, new_bot: Option<String>, start: &str) {
        self.overlays.push(Overlay::Folder(FolderPicker {
            path: start.to_owned(),
            parent: None,
            dirs: vec![],
            query: Editor::default(),
            cursor: 0,
            error: None,
            new_bot,
        }));
        let body = if start.is_empty() { json!({}) } else { json!({"path": start}) };
        self.call("listDirs", body, After::Dirs);
    }

    pub fn folder_matches(p: &FolderPicker) -> Vec<&Dir> {
        p.dirs.iter().filter(|d| fuzzy(&p.query.text, &[&d.name])).collect()
    }

    fn folder_key(&mut self, mut p: FolderPicker, k: KeyEvent) -> Option<Overlay> {
        let matches: Vec<Dir> = Self::folder_matches(&p).into_iter().cloned().collect();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Down => p.cursor = (p.cursor + 1).min(matches.len().saturating_sub(1)),
            KeyCode::Up => p.cursor = p.cursor.saturating_sub(1),
            KeyCode::Right | KeyCode::Tab => {
                if let Some(d) = matches.get(p.cursor) {
                    let path = d.path.clone();
                    self.call("listDirs", json!({"path": path}), After::Dirs);
                }
            }
            KeyCode::Left | KeyCode::Backspace if p.query.text.is_empty() => {
                if let Some(parent) = p.parent.clone() {
                    self.call("listDirs", json!({"path": parent}), After::Dirs);
                }
            }
            KeyCode::Enter => {
                // Enter on a highlighted child uses it; with nothing typed and no match, the current folder.
                let chosen = if p.query.text.is_empty() && k.modifiers.contains(KeyModifiers::ALT) {
                    p.path.clone()
                } else {
                    matches.get(p.cursor).map_or_else(|| p.path.clone(), |d| d.path.clone())
                };
                self.folder_chosen(p.new_bot.take(), chosen);
                return None;
            }
            KeyCode::Char('.') if p.query.text.is_empty() => {
                let chosen = p.path.clone();
                self.folder_chosen(p.new_bot.take(), chosen);
                return None;
            }
            _ => {
                if p.query.key(k) {
                    p.cursor = 0;
                }
            }
        }
        Some(Overlay::Folder(p))
    }

    fn folder_chosen(&mut self, new_bot: Option<String>, path: String) {
        if let Some(backend) = new_bot {
            // Step 3: name and looks, prefilled.
            if let Some(i) = self.overlays.iter().rposition(|o| matches!(o, Overlay::Agents(_))) {
                self.overlays.truncate(i);
            }
            let n = self.bots.len() + usize::try_from(crate::store::now_ms() % 97).unwrap_or(0);
            let name = NAMES[n % NAMES.len()].to_owned();
            let f = Form {
                bot_id: None,
                name: Editor::with(&name),
                instructions: Editor::default(),
                backend,
                model: Editor::default(),
                cwd: path,
                auto: false,
                notify: true,
                color: (n * 7) % COLORS.len(),
                shape: n % SHAPES.len(),
                connectors: self
                    .market
                    .0
                    .iter()
                    .map(|t| Toggle { id: t.id.clone(), name: t.name.clone(), on: false })
                    .collect(),
                skills: self
                    .market
                    .1
                    .iter()
                    .map(|t| Toggle { id: t.id.clone(), name: t.name.clone(), on: false })
                    .collect(),
                field: 0,
                sub: 0,
                error: None,
                saving: false,
            };
            self.overlays.push(Overlay::Form(Box::new(f)));
            self.call("connectors", json!({}), After::Connectors);
            self.call("skills", json!({}), After::Skills);
        } else if let Some(Overlay::Form(f)) = self.overlays.last_mut() {
            f.cwd = path;
        }
    }

    fn form_key(&mut self, mut f: Box<Form>, k: KeyEvent) -> Option<Overlay> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let field = f.current();
        match k.code {
            KeyCode::Esc => return None,
            KeyCode::Char('s') if ctrl => {
                self.save_form(&mut f);
            }
            KeyCode::Tab | KeyCode::Down => {
                f.field = (f.field + 1) % FIELDS.len();
                f.sub = 0;
            }
            KeyCode::BackTab | KeyCode::Up => {
                f.field = (f.field + FIELDS.len() - 1) % FIELDS.len();
                f.sub = 0;
            }
            KeyCode::Enter
                if field == Field::Instructions && k.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) =>
            {
                f.instructions.insert("\n");
            }
            KeyCode::Char('j') if ctrl && field == Field::Instructions => f.instructions.insert("\n"),
            KeyCode::Enter if field == Field::Folder => {
                let start = f.cwd.clone();
                self.overlays.push(Overlay::Form(f));
                self.open_folder(None, &start);
                return None;
            }
            KeyCode::Enter => self.save_form(&mut f),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                if matches!(
                    field,
                    Field::Agent
                        | Field::Approvals
                        | Field::Notify
                        | Field::Color
                        | Field::Shape
                        | Field::Connectors
                        | Field::Skills
                ) =>
            {
                let d: isize = if k.code == KeyCode::Left { -1 } else { 1 };
                let space = k.code == KeyCode::Char(' ');
                match field {
                    Field::Agent if !space => {
                        let ids: Vec<String> =
                            self.agent_choices("").iter().filter_map(|b| b["id"].as_str().map(str::to_owned)).collect();
                        if !ids.is_empty() {
                            let i = ids.iter().position(|x| *x == f.backend).unwrap_or(0);
                            f.backend.clone_from(&ids[cycle(i, d, ids.len())]);
                        }
                    }
                    Field::Approvals => f.auto = !f.auto,
                    Field::Notify => f.notify = !f.notify,
                    Field::Color if !space => f.color = cycle(f.color, d, COLORS.len()),
                    Field::Shape if !space => f.shape = cycle(f.shape, d, SHAPES.len()),
                    Field::Connectors | Field::Skills => {
                        let list = if field == Field::Connectors { &mut f.connectors } else { &mut f.skills };
                        if space {
                            if let Some(t) = list.get_mut(f.sub) {
                                t.on = !t.on;
                            }
                        } else if !list.is_empty() {
                            f.sub = cycle(f.sub, d, list.len());
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                let ed = match field {
                    Field::Name => Some(&mut f.name),
                    Field::Instructions => Some(&mut f.instructions),
                    Field::Model => Some(&mut f.model),
                    _ => None,
                };
                if let Some(ed) = ed {
                    ed.key(k);
                }
            }
        }
        Some(Overlay::Form(f))
    }

    fn save_form(&mut self, f: &mut Form) {
        if f.saving {
            return;
        }
        if f.name.text.trim().is_empty() {
            f.error = Some("Give the bot a name.".into());
            return;
        }
        let model = f.model.text.trim();
        let mut body = json!({
            "name": f.name.text.trim(),
            "description": f.instructions.text.trim(),
            "backend": f.backend,
            "cwd": f.cwd,
            "permission": if f.auto { "auto" } else { "ask" },
            "model": if model.is_empty() { Value::Null } else { model.into() },
            "notify": f.notify,
            "avatarColor": COLORS[f.color],
            "avatarShape": SHAPES[f.shape],
            "connectors": f.connectors.iter().filter(|t| t.on).map(|t| t.id.clone()).collect::<Vec<_>>(),
            "skills": f.skills.iter().filter(|t| t.on).map(|t| t.id.clone()).collect::<Vec<_>>(),
        });
        f.saving = true;
        f.error = None;
        match &f.bot_id {
            Some(id) => {
                body["id"] = id.clone().into();
                self.call("updateBot", body, After::Saved);
            }
            None => self.call("createBot", body, After::Saved),
        }
    }

    // ---------- mouse ----------

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let at = Position::new(m.column, m.row);
        match m.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let d: isize = if m.kind == MouseEventKind::ScrollUp { -3 } else { 3 };
                if self.hits.trace.contains(at) {
                    self.scroll_trace(d);
                } else if self.hits.chat.contains(at) {
                    self.scroll_chat(-d);
                } else if self.hits.roster.contains(at) {
                    self.move_bot(d.signum());
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self.hits.clicks.iter().rev().find(|(r, _)| r.contains(at)).map(|(_, c)| c.clone());
                match hit {
                    Some(Click::Bot(id)) => {
                        self.overlays.clear();
                        self.select(&id);
                        self.focus = Focus::Roster;
                        if self.width == Width::Narrow {
                            self.chat_page = true;
                            self.focus = Focus::Chat;
                        }
                    }
                    Some(Click::Option { entry, option }) => self.answer(&entry, &option),
                    Some(Click::Filter(i)) => {
                        if let Some(Overlay::Goto(g)) = self.overlays.last_mut() {
                            g.filter = i;
                            g.cursor = 0;
                        } else {
                            self.overlays.push(Overlay::Goto(Goto { query: Editor::default(), filter: i, cursor: 0 }));
                        }
                    }
                    Some(Click::Composer) => self.start_typing(),
                    Some(Click::Thread(root)) => self.open_thread(root),
                    None => {
                        if self.overlays.is_empty() {
                            if self.hits.trace.contains(at) {
                                self.focus = Focus::Trace;
                            } else if self.hits.chat.contains(at) {
                                self.focus = Focus::Chat;
                            } else if self.hits.roster.contains(at) {
                                self.focus = Focus::Roster;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

const NAMES: [&str; 24] = [
    "Ada", "Rex", "Mia", "Sol", "Kit", "Oli", "Ivy", "Max", "Zoe", "Leo", "Ari", "Bea", "Cal", "Dot", "Eli", "Fay",
    "Gus", "Hal", "Ida", "Jin", "Kai", "Lux", "Nia", "Otto",
];

fn page(h: usize) -> isize {
    isize::try_from(h.max(4)).unwrap_or(20) - 2
}

fn cycle(i: usize, d: isize, n: usize) -> usize {
    let n = isize::try_from(n).unwrap_or(1).max(1);
    usize::try_from((isize::try_from(i).unwrap_or(0) + d).rem_euclid(n)).unwrap_or(0)
}

fn merge_toggles(have: &mut Vec<Toggle>, all: &[Toggle]) {
    for t in all {
        match have.iter_mut().find(|h| h.id == t.id) {
            Some(h) => h.name.clone_from(&t.name),
            None => have.push(Toggle { id: t.id.clone(), name: t.name.clone(), on: false }),
        }
    }
}

fn move_line(e: &mut Editor, d: isize) {
    let start = e.text[..e.cursor].rfind('\n').map_or(0, |i| i + 1);
    let col = e.text[start..e.cursor].chars().count();
    let target_start = if d < 0 {
        if start == 0 {
            return;
        }
        e.text[..start - 1].rfind('\n').map_or(0, |i| i + 1)
    } else {
        match e.text[e.cursor..].find('\n') {
            Some(i) => e.cursor + i + 1,
            None => return,
        }
    };
    let line_end = e.text[target_start..].find('\n').map_or(e.text.len(), |i| target_start + i);
    e.cursor = e.text[target_start..line_end].char_indices().nth(col).map_or(line_end, |(i, _)| target_start + i);
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

fn parent_dir(path: &str) -> String {
    std::path::Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn fuzzy_is_a_subsequence_match() {
        assert!(fuzzy("rx", &["Rex"]));
        assert!(fuzzy("", &["anything"]));
        assert!(!fuzzy("xr", &["Rex"]));
        assert!(fuzzy("web", &["Rex", "~/mycode/web"]));
    }

    #[test]
    fn editor_words_and_cuts() {
        let mut e = Editor::with("hello brave world");
        e.key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(e.text, "hello brave ");
        e.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(e.cursor, 0);
        e.key(KeyEvent::new(KeyCode::Char('你'), KeyModifiers::NONE));
        assert_eq!(e.text, "你hello brave ");
        e.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(e.text, "hello brave ");
    }

    #[test]
    fn permission_options_follow_app_order() {
        let e = Entry {
            id: "p".into(),
            seq: 1,
            turn: 1,
            kind: Kind::Permission,
            data: json!({"status": "pending", "options": [
                {"optionId": "r", "name": "Reject", "kind": "reject_once"},
                {"optionId": "a", "name": "Allow", "kind": "allow_once"},
            ]}),
            created_at: 0,
            thread_id: None,
        };
        assert!(e.pending());
        let kinds: Vec<String> = e.options().into_iter().map(|o| o.2).collect();
        assert_eq!(kinds, ["allow_once", "reject_once"]);
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
            let v = json!({"botId": "g", "id": format!("e{seq}"), "seq": seq, "kind": "user", "threadId": thread});
            let (bot, e) = Entry::parse(&v).expect("a full entry parses");
            app.entries.entry(bot).or_default().insert(e.seq, e);
        }
        let ids = |app: &App| app.lane("g").iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&app), ["e1", "e3"]);
        app.thread = Some("r".into());
        assert_eq!(ids(&app), ["e2"]);
    }

    #[test]
    fn line_moves_keep_the_column() {
        let mut e = Editor::with("abc\nde");
        move_line(&mut e, -1);
        assert_eq!(e.cursor, 2);
        move_line(&mut e, 1);
        assert_eq!(e.cursor, 6);
    }
}
