//! What reaches a bot's actor ([`Cmd`]) and the turns that wait in its queue ([`Queued`]).

use crate::analytics::TurnSource;
use crate::chat::group::GroupTurn;
use crate::store::{BotConfig, Lane};
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

pub enum Cmd {
    /// A connector the bot has on was edited or set up: its agent starts again for its next turn (`restart_stale`).
    RefreshTools,
    /// A cloud computer's keeper started again, forgetting the grants the agent was started with.
    KeeperRestarted,
    Routine(String),
    /// A follow-up the bot set for itself is due (`routines::follow_up`): a turn in its own chat.
    FollowUp(String),
    /// One of the bot's tasks ended (`chat::work`): its report, for a turn in the bot's chat.
    TaskReport(crate::chat::work::Report),
    /// One assistant step of a workflow. `step_id` is `workflow_steps.id`.
    WorkflowStep {
        step_id: String,
    },
    /// Stop that step if it is the current turn, or drop it if it is still queued.
    WorkflowStop {
        step_id: String,
    },
    /// One of the bot's tasks looks stuck (`chat::work::watch`): a turn in its chat to look at it.
    Watchdog(crate::chat::work::Stuck),
    /// A thread answered the bot's `ask_task` after the call gave up waiting, or asks the bot something
    /// (`ask_chat`): a turn in its chat with it.
    TaskAnswer {
        prompt: String,
        /// The task's hops: a task added in that turn carries them on.
        hops: u8,
    },
    /// The user wrote in the bot's chat.
    Send {
        lane: Lane,
        entry_id: String,
        text: String,
    },
    /// The user wants a queued message of theirs handled now (`steer`).
    Steer {
        entry_id: String,
    },
    /// What the agent made of a steered message (`_session/steering`).
    Steered {
        entry_id: String,
        outcome: Result<codync_agent::acp::Steered>,
    },
    /// The user took back a queued message of theirs (`unqueue`).
    Unqueue {
        entry_id: String,
    },
    BotRequest(crate::chat::team::BotRequest),
    /// How a request this bot sent came out: a turn of its own, with any others that arrive meanwhile.
    BotReply(crate::chat::team::BotReply),
    /// A turn the host starts in the bot's own chat, with no message of the user's (the lead opening a call): its
    /// reply is that call's (`chat::calls`).
    Prompt {
        text: String,
        call: Option<String>,
    },
    /// `send_message` from the bot's `chat` MCP server: a message for the user, now.
    SendToUser {
        text: String,
        /// The message it quotes (`reply_to`), as `chat::replies::quote` stores it.
        quote: Option<Value>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    /// `react` from the bot's `chat` MCP server: an emoji on the message the turn answers.
    React {
        emoji: String,
        /// In a room: whose message (see `chat::reactions::in_room`).
        to: Option<String>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    SendFile {
        path: String,
        name: Option<String>,
        /// A voice note's message: the file plays at the top of this text's bubble.
        text: Option<String>,
        /// The message it quotes (`reply_to`).
        quote: Option<Value>,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    /// An artifact the bot just published (`chat::artifacts`): its card (the entry's `data`) in the turn's chat. Answers
    /// the card's entry id.
    PostArtifact {
        data: Value,
        reply: tokio::sync::oneshot::Sender<Result<String>>,
    },
    /// Its turn in a group's room turn.
    Group(GroupTurn),
    /// The group's room turns stopped: drop its queued turns, stop a running one.
    CancelGroup {
        chat: String,
    },
    Stop,
    /// A `request_decision` call (`chat::decision`): its card in the turn's chat, the call waiting.
    Ask {
        data: Value,
        started_at: i64,
        /// When the call's own timeout gives up (`chat::decision::WAIT` from its start).
        deadline: std::time::Instant,
        reply: tokio::sync::oneshot::Sender<Result<Value>>,
    },
    /// The user pressed one of a card's options (`respondDecision`).
    Decision {
        entry_id: String,
        choice: String,
        reply: tokio::sync::oneshot::Sender<Result<Value>>,
    },
    Permission {
        entry_id: String,
        option_id: Option<String>,
    },
    /// A Codync tool asks the user before it acts (`chat::manage`), on the agent's kind of approval card.
    Approve {
        title: String,
        detail: String,
        reply: tokio::sync::oneshot::Sender<bool>,
    },
    NewSession,
    /// The bot's setup as it was just saved. `pause_generation` is the save number it took
    /// (`Hub::pause_generation`), or `None` for a snapshot that isn't a full save (a fallback or lead change): its
    /// pause flag is then left to the pause commands, and it carries no ordering.
    Reconfigure {
        base: Box<BotConfig>,
        pause_generation: Option<u64>,
    },
    /// Its bot's pause changed and this actor is one of its threads: the owner's config is the one that says so.
    /// `pause_generation` is that save's number: it orders the notice against a later save whose command arrived first.
    Paused {
        paused: bool,
        pause_generation: u64,
    },
    /// The user wants the bot's own agent again (`switchBack`).
    SwitchBack,
    /// A voice call ended (`chat::calls`): its replies show in the chat again, and dreaming reads it.
    CallEnded {
        call: String,
    },
    Shutdown,
}

pub(super) enum Queued {
    Routine(String),
    FollowUp(String),
    TaskReport(crate::chat::work::Report),
    WorkflowStep(String),
    Watchdog(crate::chat::work::Stuck),
    TaskAnswer {
        prompt: String,
        hops: u8,
    },
    User {
        lane: Lane,
        entry_id: String,
        text: String,
    },
    BotRequest(crate::chat::team::BotRequest),
    BotReply(crate::chat::team::BotReply),
    Prompt {
        text: String,
        call: Option<String>,
    },
    /// A compaction the host decided on (see `compaction`); `idle` while the user is away.
    Compact {
        idle: bool,
    },
    Group(GroupTurn),
}

/// A turn the host starts in the bot's own chat, with no message of the user's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HostTurn {
    /// The lead opening a call (`Cmd::Prompt`): heard on the call, so it alerts no one and isn't resumed after a
    /// restart (the call is over).
    Opening,
    /// A follow-up the bot set for itself (`Cmd::FollowUp`): alerts like a reply, unless it had nothing to say,
    /// and is resumed after a restart as the follow-up it is.
    FollowUp,
    /// The reports of the bot's tasks (`Cmd::TaskReport`): the bot tells the user what they found, which alerts like
    /// a reply. A restart doesn't resume it: the reports it didn't answer come again (`work::restore`).
    TaskReport,
    /// The watchdog's look at a task that seems stuck (`Cmd::Watchdog`): the bot acts on it, which alerts like a
    /// reply, or passes. A restart doesn't resume it: the next look asks again if the task still seems stuck.
    Watchdog,
    /// A thread's answer to the bot's `ask_task` that came after the call gave up, or a thread's question
    /// (`ask_chat`) (`Cmd::TaskAnswer`): the bot acts on it, which alerts like a reply. A restart doesn't resume it.
    TaskAnswer,
}

impl HostTurn {
    /// Whether a host restart in the middle of it resumes it.
    pub(super) fn resumed(self) -> bool {
        match self {
            Self::Opening | Self::TaskReport | Self::Watchdog | Self::TaskAnswer => false,
            Self::FollowUp => true,
        }
    }

    /// Whether its end alerts the user, as a reply's does.
    pub(super) fn alerts(self) -> bool {
        match self {
            Self::Opening => false,
            Self::FollowUp | Self::TaskReport | Self::Watchdog | Self::TaskAnswer => true,
        }
    }

    /// What `turn_finished` reports as its source.
    pub(super) fn source(self) -> TurnSource {
        match self {
            Self::Opening | Self::TaskReport | Self::TaskAnswer => TurnSource::Host,
            Self::FollowUp => TurnSource::FollowUp,
            Self::Watchdog => TurnSource::Watchdog,
        }
    }
}
