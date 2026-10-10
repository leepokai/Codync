//! What the host sends: bots, transcript entries, and the messages that carry them.

use serde_json::Value;

use super::super::manage::Reply;
use super::tilde;

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
    /// `track`: fire and forget (an older host doesn't know it).
    Tracked,
    Analytics,
    History(String),
    Thread,
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
    pub managed_workspace: bool,
    pub auto: bool,
    pub model: Option<String>,
    pub pinned: bool,
    pub hidden: bool,
    pub notify: bool,
    pub computer: bool,
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
    /// Where it works, as the header shows it.
    pub fn folder(&self, home: &str) -> String {
        if self.managed_workspace { "personal workspace".to_owned() } else { tilde(&self.cwd, home) }
    }

    pub(super) fn parse(v: &Value) -> Option<Self> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            name: s("name"),
            description: s("description"),
            color: v["avatarColor"].as_str().unwrap_or("blue").to_owned(),
            shape: v["avatarShape"].as_str().unwrap_or("blob").to_owned(),
            backend: s("backend"),
            cwd: s("cwd"),
            managed_workspace: v["managedWorkspace"] == true,
            auto: v["permission"] == "auto",
            model: v["model"].as_str().filter(|m| !m.is_empty()).map(str::to_owned),
            pinned: v["pinned"].as_bool().unwrap_or(false),
            hidden: v["hidden"].as_bool().unwrap_or(false),
            notify: v["notify"].as_bool().unwrap_or(true),
            computer: v["computer"].as_bool().unwrap_or(false),
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
        self.away().is_none() && self.working_thread.as_deref() == thread
    }

    /// The other chat (a group) the running turn talks in, if it isn't this bot's own.
    pub fn away(&self) -> Option<&str> {
        self.working_chat.as_deref().filter(|c| *c != self.id)
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
                thread_id: v["threadId"].as_str().map(str::to_owned),
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

    /// What the chat shows as a message: the user's, and each turn's final reply.
    pub fn is_message(&self) -> bool {
        self.kind == Kind::User || self.is_final() || self.data["connectionRequest"].is_object()
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
            thread_id: None,
        };
        assert!(e.pending());
        let kinds: Vec<String> = e.options().into_iter().map(|o| o.2).collect();
        assert_eq!(kinds, ["allow_once", "reject_once"]);
    }
}
