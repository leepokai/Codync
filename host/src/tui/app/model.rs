//! What the host sends: bots, transcript entries, and the messages that carry them.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::agent::fallback::Reason;

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
    /// The model's own settings (effort, fast…): option id → value.
    pub model_options: BTreeMap<String, String>,
    pub fast_mode: bool,
    /// What the agent says about Fast mode: `unknown`, `offered`, `unsupported` or `refused`.
    pub fast_support: String,
    /// Why the agent refused it.
    pub fast_issue: Option<String>,
    pub pinned: bool,
    /// Where the user dragged it (see `App::roster`).
    pub position: i64,
    pub hidden: bool,
    pub notify: bool,
    pub computer: bool,
    /// It creates bots and changes any bot's setup (`chat::manage`); `None` from a host without it.
    pub manage_team: Option<bool>,
    /// The bot that created this one; `None` when the user did.
    pub created_by: Option<String>,
    pub created_at: i64,
    /// The harness's own memory kept on beside Codync's; `None` where it has no switch.
    pub harness_memory: Option<bool>,
    /// Agents that carry on, in order, when its own can't answer (`agent::fallback`).
    pub fallbacks: Vec<crate::store::Fallback>,
    /// The bot may set its own fallbacks.
    pub self_fallbacks: bool,
    /// The fallback agent it's on now; `None` on its own.
    pub agent_step: Option<String>,
    /// Agents of its chain resting now.
    pub resting: Vec<Rest>,
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
    /// Where the running turn talks: a chat (`None` = its own).
    pub working_chat: Option<String>,
}

/// An agent of a bot's chain that rests (`agent::fallback::resting`) until `until` (epoch ms).
#[derive(Deserialize, Clone, Debug)]
pub struct Rest {
    pub agent: String,
    pub until: i64,
    pub reason: Reason,
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}

/// A model an agent offers (`agentModels`), with its own settings.
pub struct AgentModel {
    pub id: String,
    pub name: String,
    pub options: Vec<ModelOption>,
}

/// A model setting (effort, fast…) and its values as `(id, name)`.
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub values: Vec<(String, String)>,
}

impl AgentModel {
    /// The models an `agentModels` answer lists.
    pub fn list(catalog: &Value) -> Vec<Self> {
        catalog["models"].as_array().into_iter().flatten().filter_map(Self::parse).collect()
    }

    pub fn parse(v: &Value) -> Option<Self> {
        let pair = |v: &Value| {
            let id = v["id"].as_str()?.to_owned();
            Some((v["name"].as_str().unwrap_or(&id).to_owned(), id))
        };
        let (name, id) = pair(v)?;
        let options = v["options"].as_array().into_iter().flatten().filter_map(|o| {
            let (name, id) = pair(o)?;
            let values = o["values"].as_array()?.iter().filter_map(|v| pair(v).map(|(n, i)| (i, n))).collect();
            Some(ModelOption { id, name, values })
        });
        Some(Self { id, name, options: options.collect() })
    }
}

impl Bot {
    /// An agent bot unless its agent said it has no Fast mode (an unknown one may: its first
    /// session settles it), or one that has it on (so it can be turned off).
    pub fn fast_shown(&self) -> bool {
        !self.group && (self.fast_mode || self.fast_support != "unsupported")
    }

    /// Why Fast mode isn't in effect, when the agent turned it off itself.
    pub fn fast_refusal(&self) -> Option<String> {
        (self.fast_mode && self.fast_support == "refused").then(|| {
            format!(
                "The agent turned it off: {}. Switch it off and on to try again.",
                self.fast_issue.as_deref().unwrap_or("not available")
            )
        })
    }

    /// What picking `/fast` does, or why Fast mode isn't in effect.
    pub fn fast_detail(&self) -> String {
        match self.fast_refusal() {
            Some(why) => why,
            None if self.fast_mode => "Turn off and return to standard speed".to_owned(),
            None => "Turn on for faster replies and more usage".to_owned(),
        }
    }

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
            model_options: v["modelOptions"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect(),
            fast_mode: v["fastMode"].as_bool().unwrap_or(false),
            fast_support: v["fastModeSupport"].as_str().unwrap_or("unknown").to_owned(),
            fast_issue: v["fastModeIssue"].as_str().map(str::to_owned),
            pinned: v["pinned"].as_bool().unwrap_or(false),
            position: v["position"].as_i64().unwrap_or(0),
            hidden: v["hidden"].as_bool().unwrap_or(false),
            notify: v["notify"].as_bool().unwrap_or(true),
            computer: v["computer"].as_bool().unwrap_or(false),
            manage_team: v["manageTeam"].as_bool(),
            created_by: v["createdBy"].as_str().map(str::to_owned),
            created_at: v["createdAt"].as_i64().unwrap_or(0),
            harness_memory: v["harnessMemory"].as_bool(),
            fallbacks: serde_json::from_value(v["fallbacks"].clone()).unwrap_or_default(),
            self_fallbacks: v["selfFallbacks"].as_bool().unwrap_or(true),
            agent_step: v["agentStep"]["agent"].as_str().map(str::to_owned),
            resting: serde_json::from_value(v["resting"].clone()).unwrap_or_default(),
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
        })
    }

    /// Agent `agent`'s rest, while it lasts at `now`.
    pub fn rests(&self, agent: &str, now: i64) -> Option<&Rest> {
        self.resting.iter().find(|r| r.agent == agent && r.until > now)
    }

    /// Whether the running turn talks in this chat.
    pub fn works_here(&self) -> bool {
        self.away().is_none()
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

    /// A line that folds away a longer text: a handoff (`o` shows it).
    pub fn unfolds(&self) -> bool {
        crate::tui::view::unfoldable(self).is_some()
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
