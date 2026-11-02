//! TUI state and everything that changes it: host events, keys, mouse, replies.

mod analytics;
mod approvals;
mod bot;
mod editor;
mod events;
mod exchange;
mod fallbacks;
mod forms;
mod group_form;
mod input;
mod model;
mod navigate;
mod new_bot;
mod overlay_keys;
mod roster;
mod section_order;
mod slash;
mod steps;
mod ui;

pub use approvals::parse_modes;
pub use bot::{AgentModel, Bot, ModelOption, OwnMemory};
pub use editor::Editor;
pub use exchange::{ConversationRow, Exchange, Item, Outcome, conversation, group, rows};
pub use model::{After, Entry, Kind, Mark, Msg, Status};
pub use roster::{RosterLine, cursor_line, rename_fold, roster_lines, stops};
pub use slash::SlashAction;
pub(super) use steps::{running_turn, working_steps};
pub use ui::{
    ACTIONS, Action, COLORS, Click, Confirm, ConfirmAct, Dir, FIELDS, FILTERS, Field, Focus, FolderMode, FolderPicker,
    Form, Goto, GotoItem, GroupField, GroupForm, Hits, NewBot, NewBotField, Overlay, PairCode, SHAPES, Toast, Toggle,
    TraceMode, Width,
};

use crate::tui::l10n::tr;
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
    /// The host's version (`hello`): features newer than `MIN_HOST` check it.
    pub host_version: String,
    pub online: bool,
    pub error: Option<String>,
    /// This client and the host can't work together until one of them updates.
    pub mismatch: Option<crate::compat::Mismatch>,
    pub bots: HashMap<String, Bot>,
    pub entries: HashMap<String, BTreeMap<i64, Entry>>,
    pub usage: Value,
    pub screen: Value,
    /// This computer's network (`networkView`'s shape); null on hosts without one.
    pub network: Value,
    pub backends: Vec<Value>,
    pub custom_icons: bool,
    pub selected: Option<String>,
    pub focus: Focus,
    pub typing: bool,
    /// Narrow layout: showing the chat page (else the roster page).
    pub chat_page: bool,
    /// The composer's draft in each chat (bot id).
    pub drafts: HashMap<String, Editor>,
    /// The `/` list's highlighted row and the draft it was picked for.
    pub slash_pick: (String, usize),
    /// Files dropped on the composer (pasted paths), per draft like the text.
    pub files: HashMap<String, Vec<std::path::PathBuf>>,
    /// What was pasted into each draft, sent along so memory never takes it as the user's own words.
    pub pasted: HashMap<String, Vec<String>>,
    pub download: Option<super::net::files::Download>,
    /// Each artifact's newest title, for its cards.
    pub artifact_titles: super::manage::ArtifactTitles,
    sends: HashMap<String, PendingSend>,
    /// Lines scrolled up from the bottom of the chat (0 = follow new messages).
    pub chat_scroll: usize,
    pub trace_scroll: usize,
    pub trace: TraceMode,
    /// Turn shown in the trace; `None` follows the latest.
    pub trace_turn: Option<i64>,
    /// The bot and turn whose working steps are unfolded.
    pub steps_open: Option<(String, i64)>,
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
    /// The message the next one sent replies to (`r` on a picked message; esc cancels).
    pub reply: Option<String>,
    /// A message picked to act on: reply, react, copy, save its files (`r`, then j/k and ↵).
    pub pick: Option<String>,
    /// Chat lines showing their full text: a handoff (`o` on the picked line).
    pub unfolded: HashSet<String>,
    /// Roster sections showing their label alone (`z`); this run only.
    pub folded: HashSet<String>,
    /// Section order for this terminal session; the empty name is the unsectioned Bots group.
    pub section_order: Vec<String>,
    /// Voice calls' own messages, by call id (`callEntries`): the chat loaded here may not reach back to them.
    pub call_entries: HashMap<String, Vec<Entry>>,
    pub market: (Vec<Toggle>, Vec<Toggle>),
    /// Installed connectors and skills as the host lists them.
    pub plugins: (Vec<Value>, Vec<Value>),
    /// Models each agent offers (`None` while loading).
    pub models: HashMap<String, Option<Vec<AgentModel>>>,
    /// Agents whose model list came with Fast mode.
    pub fast_mode: HashSet<String>,
    /// The modes each agent's model list came with.
    pub modes: HashMap<String, Vec<crate::agent::modes::Mode>>,
    /// A setup terminal holding the screen.
    pub shell: Option<Shell>,
    /// Chats marked read since their bot's unread count last changed.
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
    /// Whether this computer shares usage analytics (`None`: before the first `hello`).
    pub analytics: Option<bool>,
    /// What usage analytics has reported this run (`app_opened`, `screen_viewed`).
    seen: analytics::Seen,
}

impl App {
    pub fn new(client: Client, tx: UnboundedSender<Msg>, url: String) -> Self {
        Self {
            download: None,
            artifact_titles: super::manage::ArtifactTitles::default(),
            client,
            tx,
            url,
            host: String::new(),
            home: String::new(),
            host_version: String::new(),
            online: false,
            error: None,
            mismatch: None,
            bots: HashMap::new(),
            entries: HashMap::new(),
            usage: Value::Null,
            screen: Value::Null,
            network: Value::Null,
            backends: vec![],
            custom_icons: false,
            selected: None,
            focus: Focus::Roster,
            typing: false,
            chat_page: false,
            drafts: HashMap::new(),
            slash_pick: (String::new(), 0),
            files: HashMap::new(),
            pasted: HashMap::new(),
            sends: HashMap::new(),
            chat_scroll: 0,
            trace_scroll: 0,
            trace: TraceMode::Off,
            trace_turn: None,
            steps_open: None,
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
            reply: None,
            pick: None,
            unfolded: HashSet::new(),
            folded: HashSet::new(),
            section_order: Vec::new(),
            call_entries: HashMap::new(),
            market: (vec![], vec![]),
            plugins: (vec![], vec![]),
            models: HashMap::new(),
            fast_mode: HashSet::new(),
            modes: HashMap::new(),
            shell: None,
            reading: HashSet::new(),
            history_busy: HashSet::new(),
            answering: HashMap::new(),
            history_done: HashSet::new(),
            stream_since: 0,
            floors: HashMap::new(),
            hint: None,
            analytics: None,
            seen: analytics::Seen::default(),
        }
    }

    pub fn call(&self, method: &'static str, body: Value, after: After) {
        self.client.spawn_call(method, body, after, self.tx.clone());
    }

    // ---------- derived ----------

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

    /// A chat's entries, in display order.
    pub fn lane(&self, bot: &str) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.entries.get(bot).map(|m| m.values().collect()).unwrap_or_default();
        // What a voice call carries shows behind its line (`o`), on whichever device the call is: the host writes the
        // line when the call ends, or when its app went away. A file or an artifact the bot sent stays, to be opened.
        let opens = |e: &Entry| e.artifact().is_some() || e.data["files"].as_array().is_some_and(|f| !f.is_empty());
        v.retain(|e| e.kind == Kind::Notice || e.call().is_none() || opens(e));
        // A message sent mid-turn belongs after that turn's reply.
        v.sort_by_key(|e| (e.turn, e.seq));
        v
    }

    pub fn pending(&self) -> Option<&Entry> {
        let id = self.selected.as_ref()?;
        self.lane(id).into_iter().rev().find(|e| e.pending())
    }

    /// The message the composer replies to, in the selected chat.
    pub fn replying_to(&self) -> Option<&Entry> {
        let (id, reply) = (self.selected.as_ref()?, self.reply.as_ref()?);
        self.entries.get(id)?.values().find(|e| &e.id == reply)
    }

    pub fn draft(&self) -> Editor {
        self.selected.as_ref().and_then(|id| self.drafts.get(id).cloned()).unwrap_or_default()
    }

    pub fn draft_files(&self) -> &[std::path::PathBuf] {
        self.selected.as_ref().and_then(|id| self.files.get(id)).map_or(&[], Vec::as_slice)
    }

    /// Name of an entry's author (a group reply's bot), as the apps show it.
    pub fn author_name(&self, id: Option<&str>) -> String {
        id.and_then(|id| self.bots.get(id)).map_or_else(|| "A deleted bot".into(), |b| b.name.clone())
    }

    /// The chat's turns, oldest first: the trace steps through these.
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
            || self.oldest(id).is_some_and(|s| s <= 1)
    }

    fn oldest(&self, id: &str) -> Option<i64> {
        self.entries.get(id)?.values().next().map(|e| e.seq)
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
            self.reply = None;
            self.pick = None;
            self.chat_scroll = 0;
            self.trace_scroll = 0;
            self.trace_turn = None;
        }
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
            }
            None => self.flash(match want {
                Mark::Need => "No bot needs you",
                Mark::Unread => "Nothing unread",
                _ => "No match",
            }),
        }
    }

    pub fn flash(&mut self, s: &str) {
        self.hint = Some((tr(s).into_owned(), Instant::now() + Duration::from_secs(3)));
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
        {
            let id = b.id.clone();
            if self.reading.insert(id.clone()) {
                self.call("markRead", json!({"botId": id}), After::Nothing);
            }
        }
        if self.chat_top && self.chat_visible() {
            self.load_history();
        }
    }

    fn load_history(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        if self.history_busy.contains(&id) || self.history_done.contains(&id) {
            return;
        }
        let oldest = self.oldest(&id).unwrap_or(i64::MAX);
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

/// Names the bot's connectors / skills, and lists the installed ones it doesn't use, off.
fn merge_toggles(have: &mut Vec<Toggle>, all: &[Toggle]) {
    for t in all {
        match have.iter_mut().find(|h| h.id == t.id) {
            Some(h) => h.name.clone_from(&t.name),
            None => have.push(Toggle { id: t.id.clone(), name: t.name.clone(), on: false }),
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
pub(in crate::tui) fn test_app() -> App {
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    App::new(Client::new("http://127.0.0.1:1", None), tx, String::new())
}

#[cfg(test)]
mod tests;
