//! A bot as the host sends it (`Bot::parse`), and the models its agent offers.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::agent::fallback::Reason;

use super::model::{Mark, Status};
use super::tilde;

/// The harness's own memory as the bot's Memory screen shows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OwnMemory {
    /// Codync can turn it off: kept on, or off.
    Switch(bool),
    /// One memory for all the harness's sessions that Codync can't turn off (`own_memory::shared`).
    Shared,
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
    /// The agent mode it works in (`agent::modes`); none for the agent's default.
    pub mode: Option<String>,
    pub model: Option<String>,
    /// The model's own settings (effort, fast…): option id → value.
    pub model_options: BTreeMap<String, String>,
    pub fast_mode: bool,
    /// What the agent says about Fast mode: `unknown`, `offered`, `unsupported` or `refused`.
    pub fast_support: String,
    /// Why the agent refused it.
    pub fast_issue: Option<String>,
    pub pinned: bool,
    /// The computer's lead bot (`chat::lead`), placed like any bot; `*` makes another bot the lead.
    pub lead: bool,
    /// Where the user dragged it (see `App::roster`).
    pub position: i64,
    pub hidden: bool,
    /// Its roster section; `None` for none (the host trims and checks the name).
    pub section: Option<String>,
    pub notify: bool,
    pub computer: bool,
    /// It creates bots and changes any bot's setup (`chat::manage`); `None` from a host without it.
    pub manage_team: Option<bool>,
    /// `None`: the host has no per-bot network switches.
    pub offered_to_network: Option<bool>,
    pub reachable_by_assistants: Option<bool>,
    /// The bot that created this one; `None` when the user did.
    pub created_by: Option<String>,
    pub created_at: i64,
    /// The harness's own memory beside Codync's; `None` where the screen says nothing about it.
    pub harness_memory: Option<OwnMemory>,
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
    pub options_error: Option<String>,
}

/// A model setting (effort, fast…) and its values as `(id, name)`.
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub values: Vec<(String, String)>,
    pub default: Option<String>,
}

impl AgentModel {
    pub fn parse(v: &Value) -> Option<Self> {
        let pair = |v: &Value| {
            let id = v["id"].as_str()?.to_owned();
            Some((v["name"].as_str().unwrap_or(&id).to_owned(), id))
        };
        let (name, id) = pair(v)?;
        let options = v["options"].as_array().into_iter().flatten().filter_map(|o| {
            let (name, id) = pair(o)?;
            let values = o["values"].as_array()?.iter().filter_map(|v| pair(v).map(|(n, i)| (i, n))).collect();
            Some(ModelOption { id, name, values, default: o["default"].as_str().map(str::to_owned) })
        });
        Some(Self {
            id,
            name,
            options: options.collect(),
            options_error: v["optionsError"].as_str().map(str::to_owned),
        })
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
            mode: v["mode"].as_str().map(str::to_owned),
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
            lead: v["lead"] == true,
            position: v["position"].as_i64().unwrap_or(0),
            hidden: v["hidden"].as_bool().unwrap_or(false),
            section: v["section"].as_str().map(str::to_owned),
            notify: v["notify"].as_bool().unwrap_or(true),
            computer: v["computer"].as_bool().unwrap_or(false),
            manage_team: v["manageTeam"].as_bool(),
            offered_to_network: v["offeredToNetwork"].as_bool(),
            reachable_by_assistants: v["reachableByAssistants"].as_bool(),
            created_by: v["createdBy"].as_str().map(str::to_owned),
            created_at: v["createdAt"].as_i64().unwrap_or(0),
            harness_memory: v["harnessMemory"]
                .as_bool()
                .map(OwnMemory::Switch)
                .or_else(|| (v["harnessMemoryShared"] == true).then_some(OwnMemory::Shared)),
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

    /// Its band in the roster: pinned bots, then the rest. Reordering stays inside a band.
    pub fn band(&self) -> u8 {
        u8::from(!self.pinned)
    }

    /// Agent `agent`'s rest, while it lasts at `now`.
    pub fn rests(&self, agent: &str, now: i64) -> Option<&Rest> {
        self.resting.iter().find(|r| r.agent == agent && r.until > now)
    }

    /// A turn is running: working, or waiting on the user.
    pub fn busy(&self) -> bool {
        matches!(self.status, Status::Working | Status::NeedsInput)
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
