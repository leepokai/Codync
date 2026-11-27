//! Work an agent runs apart from its turn, as claude-agent-acp reports it (`async_task_spawned`,
//! `async_task_progress`, `async_task_state_update`) to a client that asked for it
//! ([`background_tasks_meta`](crate::acp::claude::background_tasks_meta)): a command or a workflow it started in
//! the background, a monitor, and a turn it began by itself (to answer what such work said when it ended).

use crate::acp::text;
use serde_json::Value;

/// One report about a piece of background work.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Task {
    /// The adapter's id for it.
    pub id: String,
    pub change: TaskChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskChange {
    /// It began: what it is, and the tool call that started it (none for a turn the agent began by itself).
    Started { kind: TaskKind, name: String, description: Option<String>, tool_call: Option<String> },
    /// What it does now (a workflow's phase and agent, `Count: two`), when this report says.
    Progress { activity: Option<String> },
    /// It ended.
    Ended(TaskEnd),
}

/// What a piece of background work is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// A command that went to the background.
    Shell,
    /// A workflow (Claude's `Workflow` tool).
    Workflow,
    /// A monitor watching something (Claude's `Monitor` tool).
    Monitor,
    /// A turn the agent began by itself, with no prompt: it answers what background work said as it ended.
    Turn,
    /// A kind this crate doesn't know yet.
    Other,
}

/// How a piece of background work ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskEnd {
    Completed,
    Failed,
    /// Stopped by the user or the agent. The agent hears nothing of it, so no turn of its own follows.
    Stopped,
}

impl Task {
    /// The report in an `async_task_*` update; None for any other update, and for a state that isn't an end
    /// (running, paused).
    pub(super) fn parse(u: &Value) -> Option<Self> {
        let id = text(&u["asyncTaskId"])?;
        let change = match u["sessionUpdate"].as_str()? {
            "async_task_spawned" => TaskChange::Started {
                kind: TaskKind::of(u["taskType"].as_str()),
                name: text(&u["name"]).unwrap_or_else(|| "Background task".to_owned()),
                description: text(&u["description"]),
                tool_call: text(&u["toolCallId"]),
            },
            "async_task_progress" => TaskChange::Progress { activity: text(&u["description"]) },
            "async_task_state_update" => TaskChange::Ended(match u["state"].as_str()? {
                "completed" => TaskEnd::Completed,
                "failed" => TaskEnd::Failed,
                "stopped" => TaskEnd::Stopped,
                _ => return None,
            }),
            _ => return None,
        };
        Some(Self { id, change })
    }
}

impl TaskKind {
    fn of(kind: Option<&str>) -> Self {
        match kind {
            Some("shell") => Self::Shell,
            Some("workflow") => Self::Workflow,
            Some("monitor") => Self::Monitor,
            Some("turn") => Self::Turn,
            _ => Self::Other,
        }
    }
}
