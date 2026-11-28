//! A bot as the host sends it (`Bot::parse`), and the models its agent offers.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::agent::fallback::Reason;

use super::model::{Mark, Status};
use super::tilde;
use super::work::{self, WorkTask};

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
    /// The login profile it runs on; none for the computer's own sign-in.
    pub login_profile: Option<String>,
    pub cwd: String,
    pub managed_workspace: bool,
    pub auto: bool,
    /// The agent mode it works in (`codync_agent::modes`); none for the agent's default.
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
    /// When Codync summarizes its chat; `None` where it doesn't.
    pub compaction: Option<codync_mem::compaction::Threshold>,
    /// The sizes it may take; `None` until the agent has told its window.
    pub compaction_sizes: Option<codync_mem::compaction::Sizes>,
    /// Agents that carry on, in order, when its own can't answer (`agent::fallback`).
    pub fallbacks: Vec<crate::store::Fallback>,
    /// The bot may set its own fallbacks.
    pub self_fallbacks: bool,
    /// Advisor mode: the agent its threads run on; `None`: off.
    pub thread_agent: Option<crate::store::Fallback>,
    /// The threads' fallbacks; `None`: the host has no Advisor mode.
    pub thread_fallbacks: Option<Vec<crate::store::Fallback>>,
    /// The fallback agent it's on now; `None` on its own.
    pub agent_step: Option<String>,
    /// The login profile of that agent the step runs on; `None`: its default one.
    pub step_login_profile: Option<String>,
    /// Agents of its chain resting now.
    pub resting: Vec<Rest>,
    /// What it keeps running.
    pub dev_servers: Vec<super::super::manage::DevServer>,
    pub connectors: Vec<String>,
    pub skills: Vec<String>,
    /// The `/` commands of the agent it runs on now, as `(name, description)`.
    pub agent_commands: Vec<(String, String)>,
    pub status: Status,
    pub activity: String,
    pub started_at: Option<i64>,
    pub unread: i64,
    /// How many of `unread` are cards waiting on the user: the badge turns amber.
    pub waiting: i64,
    pub last_message: String,
    pub last_at: i64,
    /// A group chat: several bots and the user, no agent of its own.
    pub group: bool,
    pub members: Vec<String>,
    /// Where the running turn talks: a chat (`None` = its own).
    pub working_chat: Option<String>,
    /// Its event box: the working tasks, then the waiting in the order they will be taken, then those ended in the
    /// last day. Its own `status` stays the chat's.
    pub work: Vec<WorkTask>,
    /// The most threads it runs at once; `None` from a host before the box.
    pub max_threads: Option<u8>,
    /// How it stands outside its chat (`bot.outside.status`); `None` from a host before it.
    pub outside: Option<Status>,
    /// A thread that starts beside another in the same git repository gets a worktree of its own; `None` from a host
    /// before worktrees.
    pub thread_worktrees: Option<bool>,
    /// Its folder is in a git repository: where `thread_worktrees` matters.
    pub git_repo: bool,
    /// The host can run git; true from a host that doesn't say.
    pub git_installed: bool,
    /// The folders its threads work in, the first its default (`hello.projects`); `cwd` is then always its home.
    pub projects: Vec<String>,
    /// Which of `projects` is in a git repository, in the same order.
    pub project_git_repos: Vec<bool>,
    /// When a thread gets a worktree of its own; `None` from a host before projects.
    pub worktree_mode: Option<crate::store::WorktreeMode>,
    /// Per git project, the folders linked into each of its new worktrees (`hello.worktreeFill`).
    pub shared_folders: BTreeMap<String, Vec<String>>,
}

/// A login of a bot's chain (an agent on one of its login profiles, or none: its default one) that rests
/// (`agent::fallback::resting`) until `until` (epoch ms).
#[derive(Deserialize, Clone, Debug)]
pub struct Rest {
    pub agent: String,
    #[serde(default, rename = "loginProfile")]
    pub login_profile: Option<String>,
    pub until: i64,
    pub reason: Reason,
}

/// A status as the wire names it; none for one this terminal doesn't know.
fn status(v: &Value) -> Option<Status> {
    match v.as_str()? {
        "idle" => Some(Status::Idle),
        "working" => Some(Status::Working),
        "needsInput" => Some(Status::NeedsInput),
        "error" => Some(Status::Error),
        _ => None,
    }
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

    /// Where it works: its default project, else (None) its own folder, else (an older host) its project folder.
    pub fn work_folder(&self) -> Option<&str> {
        match self.projects.first() {
            Some(project) => Some(project),
            None if self.managed_workspace => None,
            None => Some(&self.cwd),
        }
    }

    /// Where it works, as the header shows it.
    pub fn folder(&self, home: &str) -> String {
        self.work_folder().map_or_else(|| "own folder".to_owned(), |folder| tilde(folder, home))
    }

    /// Whether `project` is one of its projects in a git repository.
    pub fn git_project(&self, project: &str) -> bool {
        self.projects.iter().zip(&self.project_git_repos).any(|(p, git)| *git && p == project)
    }

    pub(in crate::tui) fn parse(v: &Value) -> Option<Self> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            name: s("name"),
            description: s("description"),
            color: v["avatarColor"].as_str().unwrap_or("blue").to_owned(),
            shape: v["avatarShape"].as_str().unwrap_or("blob").to_owned(),
            backend: s("backend"),
            login_profile: v["loginProfile"].as_str().map(str::to_owned),
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
            compaction: serde_json::from_value(v["compaction"].clone()).ok().flatten(),
            compaction_sizes: serde_json::from_value(v["compactionTokens"].clone()).ok().flatten(),
            fallbacks: serde_json::from_value(v["fallbacks"].clone()).unwrap_or_default(),
            self_fallbacks: v["selfFallbacks"].as_bool().unwrap_or(true),
            thread_agent: serde_json::from_value(v["threadAgent"].clone()).ok().flatten(),
            thread_fallbacks: serde_json::from_value(v["threadFallbacks"].clone()).ok(),
            agent_step: v["agentStep"]["agent"].as_str().map(str::to_owned),
            step_login_profile: v["agentStep"]["loginProfile"].as_str().map(str::to_owned),
            resting: serde_json::from_value(v["resting"].clone()).unwrap_or_default(),
            dev_servers: serde_json::from_value(v["devServers"].clone()).unwrap_or_default(),
            connectors: strs(&v["connectors"]),
            skills: strs(&v["skills"]),
            agent_commands: v["agentCommands"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| {
                    // What it says it does, else what it takes after its name, as the other apps show.
                    let about = c["description"].as_str().filter(|d| !d.is_empty()).or(c["hint"].as_str());
                    Some((c["name"].as_str()?.to_owned(), about.unwrap_or_default().to_owned()))
                })
                .collect(),
            status: status(&v["status"]).unwrap_or(Status::Idle),
            outside: status(&v["outside"]["status"]),
            activity: s("activity"),
            started_at: v["startedAt"].as_i64(),
            unread: v["unread"].as_i64().unwrap_or(0),
            waiting: v["waiting"].as_i64().unwrap_or(0),
            last_message: s("lastMessage"),
            last_at: v["lastAt"].as_i64().unwrap_or(0),
            group: v["kind"] == "group",
            members: strs(&v["members"]),
            working_chat: v["workingChat"].as_str().map(str::to_owned),
            work: WorkTask::list(&v["work"]),
            max_threads: work::max_threads(&v["maxThreads"]),
            thread_worktrees: v["threadWorktrees"].as_bool(),
            git_repo: v["gitRepo"] == true,
            git_installed: v["gitInstalled"] != false,
            projects: strs(&v["projects"]),
            project_git_repos: v["projectGitRepos"].as_array().into_iter().flatten().map(|g| *g == true).collect(),
            worktree_mode: serde_json::from_value(v["worktreeMode"].clone()).ok(),
            shared_folders: serde_json::from_value(v["sharedFolders"].clone()).unwrap_or_default(),
        })
    }

    /// What Settings says under its folder; nothing from a host before worktrees, which doesn't tell.
    pub fn git_line(&self) -> Option<&'static str> {
        self.thread_worktrees?;
        Some(match (self.git_installed, self.git_repo) {
            (false, _) => "Git isn't installed on this computer",
            (true, true) => "Git repository",
            (true, false) => "Not a git repository",
        })
    }

    /// Its band in the roster: pinned bots, then the rest. Reordering stays inside a band.
    pub fn band(&self) -> u8 {
        u8::from(!self.pinned)
    }

    /// The rest of agent `agent` on `login_profile`, while it lasts at `now`.
    pub fn rests(&self, agent: &str, login_profile: Option<&str>, now: i64) -> Option<&Rest> {
        self.resting.iter().find(|r| r.agent == agent && r.login_profile.as_deref() == login_profile && r.until > now)
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

    /// What its chat shows: the chat's own status.
    pub fn mark(&self) -> Mark {
        self.mark_of(self.status)
    }

    /// What its row in the roster shows: how it stands outside its chat, where its box counts too (it works while
    /// a task works or waits, and needs you while one waits on a card). The chat's own on a host before `outside`.
    pub fn roster_mark(&self) -> Mark {
        self.mark_of(self.outside.unwrap_or(self.status))
    }

    /// The messages in the badge the user hasn't read: what `markRead` clears (waiting cards stay counted).
    pub fn unseen(&self) -> i64 {
        self.unread - self.waiting
    }

    fn mark_of(&self, status: Status) -> Mark {
        match status {
            Status::NeedsInput => Mark::Need,
            Status::Error => Mark::Error,
            Status::Working => Mark::Work,
            Status::Idle if self.unread > 0 => Mark::Unread,
            Status::Idle => Mark::Idle,
        }
    }
}
