//! What the host sends: transcript entries and the messages that carry them, and the states a bot shows (the bot
//! itself is in `bot`).

use serde_json::Value;

use super::super::manage::Reply;

pub enum Msg {
    Online(bool),
    /// A new events connection asked for everything after `since`; its `hello` starts the catch-up.
    Connected {
        since: i64,
    },
    FileDownload {
        id: String,
        status: Result<String, String>,
        done: bool,
    },
    Event(Value),
    Reply(After, Result<Value, String>),
    /// The stream is starting over from rev 0: drop what we have.
    Rewind,
    /// A setup terminal's output or exit.
    Term(String, Value),
}

/// What to do with a command's reply.
#[derive(Clone)]
pub enum After {
    Nothing,
    /// A permission card's answer arrived (or couldn't).
    Answered(String),
    Sent(String),
    Connection(String, super::super::connections::Step),
    Hello,
    /// `track`: fire and forget; analytics never surfaces an error.
    Tracked,
    Analytics,
    History(String),
    Dirs,
    Pairing,
    Backends,
    Update,
    Saved,
    Connectors,
    Skills,
    Installed,
    /// Memory, routines, marketplace, sign-in (`manage`).
    Sheet(Reply),
    /// The network sheet's calls.
    Network(super::super::manage::NetReply),
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
        match self.data["callSeconds"].as_u64() {
            Some(s) => format!("{} · {:02}:{:02}", self.text(), s / 60, s % 60),
            None => self.text().to_owned(),
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

    /// A notice for a bot-to-bot message or request (`chat::team`).
    pub fn is_bot_message(&self) -> bool {
        self.kind == Kind::Notice
            && self.data["botMessage"].is_object()
            && ["sourceBotId", "targetBotId", "text"].iter().all(|k| self.data["botMessage"][k].is_string())
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
            created_at: 0,
        };
        assert!(e.pending());
        let kinds: Vec<String> = e.options().into_iter().map(|o| o.2).collect();
        assert_eq!(kinds, ["allow_once", "reject_once"]);
    }
}
