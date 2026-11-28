//! Who may do what through the `codync` MCP tools ([MCP access](../../../../../docs/features/mcp-access.md)): the
//! access levels a user gives an outside app, the class every tool is declared under (in [`class`], the one table),
//! and the check every call passes before its handler runs ([`admit`]).

use crate::store::Permission;
use codync_network::Access;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// How far an outside app may go, least first. An app approved higher still acts under the computer's maximum
/// (`mcpMaxAccess`) and under any level it narrows itself to (`accessLimit`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    /// It reads threads, routines and workflows; it changes nothing.
    #[default]
    ReadOnly,
    /// It starts and steers threads whose every request is an approval card on the user's devices, its agent in its
    /// most careful mode, calling only the host tools that read or answer.
    Ask,
    /// As Ask, its agent in its own auto mode: it does by itself what it judges safe, and what it asks for is a card.
    Auto,
    /// It may also run threads that approve requests by themselves in the agent mode it picks, and start the owner's
    /// own work (routine and workflow runs).
    Full,
}

impl Level {
    /// Every level, least first.
    pub const ALL: [Self; 4] = [Self::ReadOnly, Self::Ask, Self::Auto, Self::Full];

    pub fn word(self) -> &'static str {
        match self {
            Self::ReadOnly => "readOnly",
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Full => "full",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        serde_json::from_value(Value::String(word.to_owned())).ok()
    }

    /// The network ceiling a thread this level started works under (`work::access_of`): its agent's mode is no
    /// freer than it, nothing is approved for it, and it calls only the host tools that read or answer.
    pub fn ceiling(self) -> Access {
        match self {
            Self::ReadOnly | Self::Ask => Access::Ask,
            Self::Auto => Access::Auto,
            Self::Full => Access::Allow,
        }
    }

    /// How a thread at this level is approved: Ask, else requests are approved by themselves (under a ceiling below
    /// Full, which approves nothing by itself).
    pub fn permission(self) -> Permission {
        if self >= Self::Auto { Permission::Auto } else { Permission::Ask }
    }

    /// What a thread the user set up themselves (`permission`, the agent's own `mode`) is to these levels: only an
    /// asking thread in the agent's default mode is Ask; one that approves by itself or picks a mode is Full.
    pub fn of_setup(permission: Permission, mode: Option<&str>) -> Self {
        if permission == Permission::Ask && mode.is_none() { Self::Ask } else { Self::Full }
    }
}

/// What a tool does, declared once per tool in [`class`]: what its caller must be allowed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// The messaging tools that came before levels (`list_bots`, `message_bot`, `read_replies`, the mailbox): kept
    /// as they were, under their own switches (Cloud assistants, each bot's Reachable by cloud assistants).
    Messages,
    /// Reads only.
    Reads,
    /// Starts a thread, at a level no higher than the caller's.
    StartsThreads,
    /// Writes to, steers or answers a thread the caller sees, whose own level is no higher than the caller's.
    WritesThreads,
    /// Starts the owner's own work, which picks its own approvals (a routine's or a workflow's run): Full only.
    WritesOwnerWork,
}

impl Class {
    pub fn word(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Reads => "reads",
            Self::StartsThreads => "startsThreads",
            Self::WritesThreads => "writesThreads",
            Self::WritesOwnerWork => "writesOwnerWork",
        }
    }
}

/// The class of every tool on the `codync` server and the scope it needs; `None` for a name it doesn't have. A tool
/// that isn't here can't be called (the test below checks every listed tool is).
pub fn declared(tool: &str, args: &Value) -> Option<(Class, Option<Scope>)> {
    use {Class as C, Scope as S};
    Some(match tool {
        "list_bots" | "message_bot" | "read_replies" | "whoami" | "set_name" | "check_inbox" => (C::Messages, None),
        "worker_capabilities" => (C::Reads, None),
        // Listing and reading what waits on the user only reads; answering a choice writes to the thread.
        "thread_pending" if args["action"] == "answer" => (C::WritesThreads, Some(S::Threads)),
        "thread_list" | "thread_wait" | "thread_read" | "thread_pending" => (C::Reads, Some(S::Threads)),
        "thread_launch" => (C::StartsThreads, Some(S::Threads)),
        "thread_send" | "thread_steer" | "thread_interrupt" => (C::WritesThreads, Some(S::Threads)),
        "routine_list" => (C::Reads, Some(S::Routines)),
        "routine_run" => (C::WritesOwnerWork, Some(S::Routines)),
        "workflow_list" => (C::Reads, Some(S::Workflows)),
        "workflow_run" => (C::WritesOwnerWork, Some(S::Workflows)),
        _ => return None,
    })
}

/// A tool's class ([`declared`]).
pub fn class(tool: &str, args: &Value) -> Option<Class> {
    declared(tool, args).map(|(class, _)| class)
}

/// What a caller may reach besides its level.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// Start threads and act on the ones it started.
    Threads,
    /// Also see and act on the other threads of the bots it may reach (never another app's or the user's threads
    /// without a bot).
    AllThreads,
    Routines,
    Workflows,
}

/// Why a call was refused, with the code the caller reads (`code` in the tool's error).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    CapabilityDenied,
    ModeEscalationDenied,
    NotFound,
    InvalidRequest,
    RateLimited,
    Revoked,
    Expired,
}

#[derive(Debug)]
pub struct Refusal {
    pub code: Code,
    message: String,
}

impl Refusal {
    pub fn err(code: Code, message: impl Into<String>) -> anyhow::Error {
        Self { code, message: message.into() }.into()
    }
}

/// The tool error an app reads: the words, then the code it can act on.
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (code: {})", self.message, json!(self.code).as_str().unwrap_or_default())
    }
}

impl std::error::Error for Refusal {}

/// What is never done for an app: approving a request. Said in every refusal of a write, so the app tells the user.
pub const ACT_DIRECTLY: &str = "This call does not create an approval request; have the user act directly in Codync.";

/// A call that passed [`admit`]: its handler runs only with one (the field keeps it from being made elsewhere).
pub struct Admitted {
    /// The level it acts under: what the app was approved, the computer's maximum and its own limit, the least.
    pub ceiling: Level,
    _made_here: (),
}

/// The level a caller approved at `approved` acts under: the computer's `max`, and the `limit` it asks for itself
/// (it may narrow, never widen).
pub fn ceiling(approved: Level, max: Level, limit: Option<Level>) -> Level {
    approved.min(max).min(limit.unwrap_or(Level::Full))
}

/// Lets `tool` through for a caller acting under `ceiling` with `scopes`, or says why not. What each handler checks
/// further (the thread's own level, a requested level, the bots and folders it may reach) is its own.
pub fn admit(tool: &str, args: &Value, ceiling: Level, scopes: &[Scope]) -> anyhow::Result<Admitted> {
    let (class, needs) =
        declared(tool, args).ok_or_else(|| Refusal::err(Code::NotFound, format!("unknown tool: {tool}")))?;
    let denied = |why: String| Err(Refusal::err(Code::CapabilityDenied, why));
    if ceiling == Level::ReadOnly && !matches!(class, Class::Messages | Class::Reads) {
        return denied(format!("{tool} changes things, and this app may only read here. {ACT_DIRECTLY}"));
    }
    if class == Class::WritesOwnerWork && ceiling < Level::Full {
        return denied(format!(
            "{tool} starts the user's own work, which picks its own approvals: only an app with Full access may. \
             {ACT_DIRECTLY}"
        ));
    }
    if let Some(scope) = needs.filter(|s| !scopes.contains(s)) {
        return denied(format!("this app wasn't given {} access here", json!(scope).as_str().unwrap_or_default()));
    }
    Ok(Admitted { ceiling, _made_here: () })
}

/// Refuses a thread at `level` to a caller under `ceiling`: it could make the thread work more freely than it may.
pub fn within(level: Level, ceiling: Level, what: &str) -> anyhow::Result<()> {
    if level > ceiling {
        return Err(Refusal::err(
            Code::ModeEscalationDenied,
            format!(
                "{what} runs at {}, above the {} this app may use here. {ACT_DIRECTLY}",
                level.word(),
                ceiling.word()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_on_the_codync_server_has_a_class() {
        let tools = crate::chat::team::assistant::tools();
        for tool in tools.as_array().expect("a list") {
            let name = tool["name"].as_str().expect("a name");
            assert!(class(name, &json!({})).is_some(), "{name} has no access class");
        }
        assert_eq!(class("thread_pending", &json!({"action": "answer"})), Some(Class::WritesThreads));
        assert_eq!(class("thread_pending", &json!({"action": "list"})), Some(Class::Reads));
        assert_eq!(class("a_tool_added_later", &json!({})), None);
    }

    #[test]
    fn a_ceiling_only_narrows() {
        assert_eq!(ceiling(Level::Full, Level::Auto, None), Level::Auto, "the computer's maximum");
        assert_eq!(ceiling(Level::Ask, Level::Full, Some(Level::Full)), Level::Ask, "a limit never widens");
        assert_eq!(ceiling(Level::Full, Level::Full, Some(Level::ReadOnly)), Level::ReadOnly);
        assert!(Level::ReadOnly < Level::Ask && Level::Ask < Level::Auto && Level::Auto < Level::Full);
    }

    #[test]
    fn a_read_only_app_is_refused_every_write_before_anything_else() {
        let all = [Scope::Threads, Scope::Routines, Scope::Workflows];
        for tool in ["thread_launch", "thread_send", "thread_steer", "thread_interrupt", "routine_run", "workflow_run"]
        {
            let refused = admit(tool, &json!({}), Level::ReadOnly, &all).err().expect("refused");
            let code = refused.downcast_ref::<Refusal>().expect("a refusal").code;
            assert_eq!(code, Code::CapabilityDenied, "{tool}");
            assert!(refused.to_string().contains(ACT_DIRECTLY), "{refused}");
        }
        assert!(admit("thread_pending", &json!({"action": "answer"}), Level::ReadOnly, &all).is_err());
        for tool in ["thread_list", "thread_read", "thread_wait", "worker_capabilities", "message_bot"] {
            assert!(admit(tool, &json!({}), Level::ReadOnly, &all).is_ok(), "{tool}");
        }
    }

    #[test]
    fn owner_work_needs_full_and_each_area_its_scope() {
        let all = [Scope::Threads, Scope::Routines, Scope::Workflows];
        assert!(admit("routine_run", &json!({}), Level::Auto, &all).is_err());
        assert!(admit("workflow_run", &json!({}), Level::Full, &all).is_ok());
        assert!(admit("routine_list", &json!({}), Level::Full, &[Scope::Threads]).is_err(), "no routines scope");
        assert!(admit("thread_launch", &json!({}), Level::Ask, &[Scope::Routines]).is_err(), "no threads scope");
    }

    #[test]
    fn a_thread_above_the_ceiling_is_an_escalation() {
        let refused = within(Level::Full, Level::Auto, "That thread").expect_err("refused");
        assert_eq!(refused.downcast_ref::<Refusal>().map(|r| r.code), Some(Code::ModeEscalationDenied));
        assert!(within(Level::Ask, Level::Auto, "x").is_ok());
        assert_eq!(Level::of_setup(Permission::Ask, None), Level::Ask);
        assert_eq!(Level::of_setup(Permission::Ask, Some("plan")), Level::Full, "a mode it picked is the agent's own");
        assert_eq!(Level::of_setup(Permission::Auto, None), Level::Full);
        assert_eq!(
            (Level::Ask.ceiling(), Level::Auto.ceiling(), Level::Full.ceiling()),
            (Access::Ask, Access::Auto, Access::Allow)
        );
    }
}
