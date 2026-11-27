//! What the host sends: transcript entries and the messages that carry them, and the states a bot shows (the bot
//! itself is in `bot`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::LockExt;
use crate::chat::decision;

use super::super::manage::Reply;

pub enum Msg {
    Online,
    Disconnected {
        since: i64,
    },
    /// A new events connection asked for everything after `since`; its `hello` starts the catch-up.
    Connected {
        since: i64,
        connection: u64,
        send_receipts: SendReceipts,
    },
    FileDownload {
        id: String,
        status: Result<String, String>,
        done: bool,
    },
    Event(Value),
    Reply(After, Result<Value, String>),
    CompletionHistory {
        bot: String,
        epoch: u64,
        request: uuid::Uuid,
        replied: bool,
    },
    /// The stream is starting over from rev 0: drop what we have.
    Rewind(SendReceipts),
    /// Another replay from zero while holding the confirmed mirror.
    ReplayStarted,
    /// The recovery replay can confirm receipts, even before a live revision arrives.
    ReplayComplete,
    /// A setup terminal's output or exit.
    Term(String, Value),
}

/// Connection and RPC revision evidence for this mirror.
#[derive(Clone, Default)]
pub struct SendReceipts(Arc<Mutex<SendReceiptState>>);

#[derive(Default)]
struct SendReceiptState {
    connection: u64,
    awaiting_hello: bool,
    rewound: bool,
    thread_rev: i64,
    rows: HashMap<String, HashMap<i64, SendReceipt>>,
}

struct SendReceipt {
    id: String,
    rev: Option<i64>,
}

impl SendReceipts {
    pub(in crate::tui) fn for_connection(connection: u64) -> Self {
        Self(Arc::new(Mutex::new(SendReceiptState { connection, awaiting_hello: true, ..SendReceiptState::default() })))
    }

    pub(in crate::tui) fn begin_request(&self) -> (u64, i64) {
        let mut state = self.0.locked();
        state.connection += 1;
        state.awaiting_hello = true;
        let rev = state.rows.values().flat_map(HashMap::values).filter_map(|r| r.rev).max().unwrap_or_default();
        (state.connection, rev.max(state.thread_rev))
    }

    pub(in crate::tui) fn invalidate_connection(&self) {
        let mut state = self.0.locked();
        state.connection += 1;
        state.awaiting_hello = false;
    }

    pub(super) fn received_hello(&self, connection: u64) {
        let mut state = self.0.locked();
        if state.connection == connection {
            state.awaiting_hello = false;
        }
    }

    pub(in crate::tui) fn thread_read_connection(&self) -> Option<u64> {
        let state = self.0.locked();
        (!state.awaiting_hello && !state.rewound).then_some(state.connection)
    }

    /// The next request samples this revision under the same lock that accepts the rows.
    pub(in crate::tui) fn accept_thread_rows(&self, connection: u64, rev: i64) -> bool {
        let mut state = self.0.locked();
        if state.connection != connection || state.awaiting_hello || state.rewound {
            return false;
        }
        state.thread_rev = state.thread_rev.max(rev);
        true
    }

    /// Marks the old mirror before the app drains its queued replies.
    pub(in crate::tui) fn invalidate_mirror(&self) {
        let mut state = self.0.locked();
        state.connection += 1;
        state.rewound = true;
    }

    pub(super) fn rewound(&self) -> bool {
        self.0.locked().rewound
    }

    pub(super) fn is_current(&self, connection: u64) -> bool {
        self.0.locked().connection == connection
    }

    pub(super) fn confirm(&self, connection: u64, row: Option<(&str, &Entry)>) -> bool {
        let mut state = self.0.locked();
        if state.connection != connection {
            return false;
        }
        if let Some((bot, entry)) = row {
            let receipt = SendReceipt { id: entry.id.clone(), rev: entry.rev };
            state.rows.entry(bot.to_owned()).or_default().insert(entry.seq, receipt);
        }
        true
    }

    pub(super) fn observe(&self, bot: &str, entry: &Entry, held: Option<&Entry>) -> bool {
        let mut state = self.0.locked();
        let receipts = &mut state.rows;
        let Some(rows) = receipts.get_mut(bot) else { return false };
        let Some(receipt) = rows.get(&entry.seq) else { return false };
        let replaced = receipt.id != entry.id;
        let displaces_held = replaced && held.is_some_and(|row| row.id == receipt.id && row.rev == receipt.rev);
        if replaced || receipt.rev <= entry.rev {
            rows.remove(&entry.seq);
        }
        if rows.is_empty() {
            receipts.remove(bot);
        }
        displaces_held
    }

    pub(super) fn retain(&self, bot: &str, mut keep: impl FnMut(&i64) -> bool) {
        let mut state = self.0.locked();
        let receipts = &mut state.rows;
        if let Some(rows) = receipts.get_mut(bot) {
            rows.retain(|seq, _| keep(seq));
            if rows.is_empty() {
                receipts.remove(bot);
            }
        }
    }
}

/// What to do with a command's reply.
#[derive(Clone)]
pub enum After {
    Nothing,
    Queued(u64),
    /// A permission card's answer arrived (or couldn't).
    Answered(String),
    Sent {
        key: String,
        nonce: String,
        epoch: u64,
    },
    CheckSendReceipt {
        key: String,
        request: uuid::Uuid,
        epoch: u64,
        connection: u64,
    },
    Connection(String, super::super::connections::Step),
    Hello,
    /// `track`: fire and forget; analytics never surfaces an error.
    Tracked,
    Analytics,
    AutoOrder,
    ThreadKeep,
    CredentialsReset,
    History(String, uuid::Uuid),
    Dirs(String),
    WorkspaceSelected(String),
    Pairing,
    /// The pair sheet's `cloudStatus` and `pairingRequests`.
    Cloud,
    PairingRequests,
    /// The pair sheet's answer to a waiting phone.
    PairingDecided,
    Backends,
    Update,
    Saved(u64),
    Connectors,
    Skills,
    Installed,
    /// Memory, routines, marketplace, sign-in (`manage`).
    Sheet(Reply),
    /// The network sheet's calls.
    Network(super::super::manage::NetReply),
    /// A move in the Usage chooser, to this row.
    UsageOrder(usize),
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    User,
    Agent,
    Thought,
    Tool,
    Plan,
    Permission,
    /// A `request_decision` card: the bot asks the user to pick one of its options.
    Decision,
    Notice,
    Other,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub seq: i64,
    pub(in crate::tui) rev: Option<i64>,
    pub turn: i64,
    pub kind: Kind,
    pub data: Value,
    pub created_at: i64,
}

impl Entry {
    pub(in crate::tui) fn parse(v: &Value) -> Option<(String, Self)> {
        let kind = match v["kind"].as_str()? {
            "user" => Kind::User,
            "agent" => Kind::Agent,
            "thought" => Kind::Thought,
            "tool" => Kind::Tool,
            "plan" => Kind::Plan,
            "permission" => Kind::Permission,
            "notice" if v["data"]["decision"] == true => Kind::Decision,
            "notice" => Kind::Notice,
            _ => Kind::Other,
        };
        Some((
            v["botId"].as_str()?.to_owned(),
            Self {
                id: v["id"].as_str()?.to_owned(),
                seq: v["seq"].as_i64()?,
                rev: v["rev"].as_i64(),
                turn: v["turn"].as_i64().unwrap_or(0),
                kind,
                data: v["data"].clone(),
                created_at: v["createdAt"].as_i64().unwrap_or(0),
            },
        ))
    }

    pub fn text(&self) -> &str {
        self.data["text"].as_str().unwrap_or_default()
    }

    /// Files sent with a user message: `(name, size)`.
    pub fn attachments(&self) -> Vec<(&str, u64)> {
        self.data["attachments"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| Some((a["name"].as_str()?, a["size"].as_u64().unwrap_or(0))))
            .collect()
    }

    /// A notice's line; a voice call gets its length ("Voice chat · 00:16").
    pub fn notice_text(&self) -> String {
        let text = super::super::l10n::tr(self.text());
        match self.data["callSeconds"].as_u64() {
            Some(s) => format!("{text} · {:02}:{:02}", s / 60, s % 60),
            None => text.into_owned(),
        }
    }

    pub fn is_final(&self) -> bool {
        self.kind == Kind::Agent && self.data["final"].as_bool().unwrap_or(false)
    }

    /// A step the working line unfolds: thinking, narration, a tool call or a plan, never the bot's bookkeeping
    /// (nor a call an older host didn't flag either way).
    pub fn is_working_step(&self) -> bool {
        match self.kind {
            Kind::Thought => !self.text().trim().is_empty(),
            Kind::Agent => !self.is_final() && !crate::chat::group::is_pass(self.text()),
            Kind::Tool => self.data["bookkeeping"] == false,
            Kind::Plan => true,
            _ => false,
        }
    }

    /// A message the user can answer with a reply: theirs, or a final reply (the host checks too).
    pub fn takes_reply(&self) -> bool {
        self.kind == Kind::User || self.is_final()
    }

    /// The text a reply quotes: the message's, or its files' names.
    pub fn quoted(&self) -> String {
        crate::store::message_text(&self.data)
    }

    /// What the chat shows as a message: the user's, and each turn's final reply.
    pub fn is_message(&self) -> bool {
        self.kind == Kind::User || self.is_final() || self.data["connectionRequest"].is_object()
    }

    /// The id of the message this one quotes (`replyTo`): `q` shows it.
    pub fn quotes(&self) -> Option<&str> {
        self.data["replyTo"]["id"].as_str()
    }

    /// An artifact card's artifact id (`docs/features/artifacts.md`): `o` opens it, `f` saves it.
    pub fn artifact(&self) -> Option<&str> {
        if self.is_final() { self.data["artifact"]["id"].as_str() } else { None }
    }

    /// A line that folds away a longer text: a handoff, or a voice call's conversation (`o` shows it).
    pub fn unfolds(&self) -> bool {
        crate::tui::view::unfoldable(self).is_some() || (self.kind == Kind::Notice && self.call().is_some())
    }

    /// The voice call this was said in, or that this line ends (`chat::calls`).
    pub fn call(&self) -> Option<&str> {
        self.data["call"].as_str()
    }

    /// A failure notice that offers adding a fallback agent (`A`).
    pub fn adds_fallback(&self) -> bool {
        self.kind == Kind::Notice && self.data["addFallback"] == true
    }

    /// A failed routine run's notice that offers running it again (`F`).
    pub fn retries(&self) -> bool {
        self.kind == Kind::Notice && self.data["retry"] == true && self.data["runId"].is_string()
    }

    /// A chain stopped at its handoff limit, whose notice lets it go on (`F`, `continueChain`).
    pub fn continues(&self) -> bool {
        self.kind == Kind::Notice && self.data["continueChain"] == true
    }

    /// A notice for a bot-to-bot message or request (`chat::team`).
    pub fn is_bot_message(&self) -> bool {
        self.kind == Kind::Notice
            && self.data["botMessage"].is_object()
            && ["sourceBotId", "targetBotId", "text"].iter().all(|k| self.data["botMessage"][k].is_string())
    }

    /// A sign-in card an agent's turn waits on (a background thread's, in its bot's chat).
    pub fn waits_for_sign_in(&self) -> bool {
        let request = &self.data["connectionRequest"];
        request["kind"] == "agent" && request["status"] == "pending"
    }

    /// A sign-in card that holds messages: signing in there is what sends them again.
    pub fn holds_messages(&self) -> bool {
        self.data["connectionRequest"]["entryIds"].as_array().is_some_and(|ids| !ids.is_empty())
    }

    pub fn pending(&self) -> bool {
        match self.kind {
            Kind::Permission => self.data["status"] == "pending",
            Kind::Decision => decision::Status::of(&self.data) == Some(decision::Status::Pending),
            _ => false,
        }
    }

    /// A choice card's options, in the order the bot offered them.
    pub fn choices(&self) -> Vec<String> {
        self.data["choices"].as_array().into_iter().flatten().filter_map(|o| o.as_str().map(str::to_owned)).collect()
    }

    /// Labels sent by numbered/function keys: decisions use their text; permissions keep their option ids.
    pub fn answer_options(&self) -> Vec<String> {
        if self.kind == Kind::Decision { self.choices() } else { self.options().into_iter().map(|o| o.0).collect() }
    }

    /// A message of the user's waiting for its turn: it shows above the composer, not in the chat.
    pub fn queued(&self) -> bool {
        self.kind == Kind::User && self.data["status"] == "queued"
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            rev: None,
            created_at: 0,
        };
        assert!(e.pending());
        let kinds: Vec<String> = e.options().into_iter().map(|o| o.2).collect();
        assert_eq!(kinds, ["allow_once", "reject_once"]);
    }

    #[test]
    fn decision_notice_keys_keep_all_twelve_labels_in_order() {
        let choices: Vec<_> = (1..=12).map(|i| format!("Choice {i}")).collect();
        let (_, entry) = Entry::parse(&json!({"id":"c","botId":"b","seq":1,"kind":"notice",
            "data":{"decision":true,"choices":choices,"status":"pending"}}))
        .unwrap();
        assert_eq!(entry.kind, Kind::Decision);
        assert!(entry.pending());
        assert_eq!(entry.answer_options(), choices);
        assert!(entry.options().is_empty(), "choice labels never become permission option ids");
    }
}
