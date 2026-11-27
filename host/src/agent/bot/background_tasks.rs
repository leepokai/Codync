//! What a thread's agent reports running apart from its turn ([`Event::Task`], `Background::Reported`): a command
//! or a workflow it started in the background, a monitor. Its task waits on that work between turns, the list shows
//! in the thread, and the turn the agent begins by itself once such work ends is a turn of the thread's, whose last
//! message is the task's report when nothing else still runs.

use super::{Actor, Done};
use crate::analytics::{self, Event as Analytics};
use crate::hub::{BackgroundKind, BackgroundTask, BotStatus};
use codync_agent::events::{Task, TaskChange, TaskEnd, TaskKind};
use serde_json::json;

/// A turn the agent began by itself, by the adapter's id for it.
pub(super) enum AgentTurn {
    /// No turn of the thread ran: it is one, and its end ends it.
    Own(String),
    /// A turn of the thread ran: that turn goes on with it, and ends once both have (`turn_ended`).
    Joined(String),
}

impl AgentTurn {
    fn id(&self) -> &str {
        match self {
            Self::Own(id) | Self::Joined(id) => id,
        }
    }
}

impl Actor {
    /// A report about the agent's background work, from session `session`.
    pub(super) fn on_task(&mut self, task: Task, session: Option<&str>) {
        let Task { id, change, .. } = task;
        match change {
            TaskChange::Started { kind: TaskKind::Turn, .. } => self.agent_turn_began(id, session),
            TaskChange::Started { kind, name, description, .. } => {
                let kind = kind_of(kind);
                let activity = description.filter(|d| *d != name);
                let task = BackgroundTask { id, kind, name, activity, started_at: crate::store::now_ms() };
                self.capture(kind, "started");
                self.hub.set_runtime(&self.cfg.id, |r| r.background.push(task));
            }
            TaskChange::Progress { activity: Some(activity) } => self.hub.set_runtime(&self.cfg.id, |r| {
                if let Some(task) = r.background.iter_mut().find(|t| t.id == id) {
                    task.activity = Some(activity);
                }
            }),
            TaskChange::Ended(end) if self.agent_turn.as_ref().is_some_and(|t| t.id() == id) => {
                self.agent_turn_ended(end);
            }
            TaskChange::Ended(end) => self.background_ended(&id, end),
            _ => {}
        }
    }

    /// Whether the agent still runs something it reported, or a turn it began by itself.
    pub(super) fn runs_in_background(&self) -> bool {
        self.agent_turn.is_some() || !self.hub.runtime(&self.cfg.id).background.is_empty()
    }

    /// Whether a turn that ends now leaves its task waiting on what the agent reports running: on anything but a
    /// command, or on commands while nothing it runs listens. A server left listening is meant to stay, as one left
    /// detached is ([`left_detached`](super::background_work)).
    // ponytail: a command can't be told from the server it may be, so with any server under the agent no command holds
    // the task; map each command to its processes if a build beside a dev server must hold it.
    pub(super) async fn waits_on_background(&self) -> bool {
        let background = self.hub.runtime(&self.cfg.id).background;
        if background.iter().any(|t| t.kind != BackgroundKind::Shell) {
            return true;
        }
        !background.is_empty() && !self.hub.dev_servers.agent_serves(&self.cfg.id).await
    }

    fn background_ended(&mut self, id: &str, end: TaskEnd) {
        let mut ended = None;
        self.hub.set_runtime(&self.cfg.id, |r| {
            if let Some(at) = r.background.iter().position(|t| t.id == id) {
                ended = Some(r.background.remove(at));
            }
        });
        // An end the adapter reports twice (with its output file the second time) is one.
        let Some(task) = ended else { return };
        self.capture(task.kind, outcome(end));
        // The agent hears nothing of a stop, so no turn of its own follows: one that ended its task's last work
        // is asked to report.
        if end == TaskEnd::Stopped && self.turn.is_none() && !self.runs_in_background() {
            crate::chat::work::ask_report(&self.hub, &self.cfg, &task.name);
        }
    }

    /// The agent began a turn by itself (to answer what its background work said as it ended): while no turn of the
    /// thread runs, it is one, so what it does shows and asks as a turn's does, and its end ends it as one.
    fn agent_turn_began(&mut self, id: String, session: Option<&str>) {
        if self.conn.is_none() || !self.cfg.is_thread() {
            return;
        }
        if self.turn.is_some() {
            self.agent_turn = Some(AgentTurn::Joined(id));
            return;
        }
        let turn = self.hub.store.max_turn(&self.lane.chat) + 1;
        self.turn = Some(turn);
        self.agent_turn = Some(AgentTurn::Own(id));
        if let Some(session) = session {
            self.turn_session = Some(session.to_owned());
        }
        self.turn_text = None;
        self.turn_entries.clear();
        self.turn_call = None;
        self.reset_turn_state();
        let (lane, session) = (self.lane.clone(), self.turn_session.clone());
        self.hub.set_runtime(&self.cfg.id, |r| {
            r.status = BotStatus::Working;
            r.activity = "Thinking…".into();
            r.started_at = Some(crate::store::now_ms());
            r.lane = Some(lane);
            r.message = None;
            r.session = session;
        });
        let properties = json!({"agent": self.cfg.backend});
        analytics::capture(&self.hub, Analytics::ThreadAgentTurn, properties.as_object().cloned().unwrap_or_default());
    }

    /// The turn the agent began by itself ended: the thread's turn ends as a prompt's does (`turn_ended`), with how
    /// the prompt it joined ended, if it did. A stop the user asked for still reads as one there (`stop_requested`).
    fn agent_turn_ended(&mut self, end: TaskEnd) {
        self.end_agent_turn(match end {
            TaskEnd::Completed | TaskEnd::Stopped => Ok(json!({"stopReason": "end_turn"})),
            TaskEnd::Failed => Err(anyhow::anyhow!("its agent's turn failed")),
        });
    }

    /// No prompt answers a turn the agent began by itself, so it ends here, with `own`: one it began with no turn of
    /// the thread's running; one it joined ends as the prompt did, once that has (still none while the prompt runs).
    fn end_agent_turn(&mut self, own: Done) {
        let done = match self.agent_turn.take() {
            Some(AgentTurn::Own(_)) => Some(own),
            Some(AgentTurn::Joined(_)) => self.held_done.take(),
            None => None,
        };
        if let (Some(done), Some(done_tx)) = (done, &self.done_tx) {
            let _ = done_tx.send(done);
        }
    }

    /// The agent stopped (or crashed): the work it ran in the background is gone with it, and so is a turn it began
    /// by itself. Between turns, the task that waited on that work fails.
    pub(super) fn background_lost(&mut self) {
        self.end_agent_turn(Err(anyhow::anyhow!("its agent stopped")));
        if self.hub.runtime(&self.cfg.id).background.is_empty() {
            return;
        }
        self.hub.set_runtime(&self.cfg.id, |r| r.background.clear());
        if self.turn.is_none() {
            let why = "its agent stopped while its background work still ran".to_owned();
            self.turn_over(crate::chat::work::Outcome::Failed(crate::chat::work::Failure::Agent, why));
        }
    }

    fn capture(&self, kind: BackgroundKind, outcome: &str) {
        let properties = json!({"agent": self.cfg.backend, "kind": kind, "outcome": outcome});
        analytics::capture(&self.hub, Analytics::ThreadBackground, properties.as_object().cloned().unwrap_or_default());
    }
}

fn kind_of(kind: TaskKind) -> BackgroundKind {
    match kind {
        TaskKind::Shell => BackgroundKind::Shell,
        TaskKind::Workflow => BackgroundKind::Workflow,
        TaskKind::Monitor => BackgroundKind::Monitor,
        _ => BackgroundKind::Other,
    }
}

fn outcome(end: TaskEnd) -> &'static str {
    match end {
        TaskEnd::Completed => "completed",
        TaskEnd::Failed => "failed",
        TaskEnd::Stopped => "stopped",
    }
}
