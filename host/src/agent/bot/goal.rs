//! Cancellation of the current goal step. Ordinary user turns are never cancelled here.
use super::{Actor, Done};
use crate::chat::goal::{self, Stopped};
use crate::store::now_ms;

/// Retains the turn's goal identity even when an approval has ended automatic continuation.
pub(super) struct Step {
    pub id: String,
    pub deadline: i64,
    pub stopped: Option<Stopped>,
}

impl Actor {
    pub(super) fn goal_ended(&mut self, stopped: bool, failed: bool, reply: Option<&str>, closing: Option<&str>) {
        if let Some(step) = self.goal_step.take() {
            let id = step.id;
            if let Some(why) = step.stopped {
                // Retry a stop that could not be persisted when the approval appeared, never queue work.
                goal::finish(&self.hub, &self.cfg.id, Some(&id), why);
                return;
            }
            let reply = reply.unwrap_or_default();
            let why = if stopped {
                Some(Stopped::Manual)
            } else if failed {
                Some(Stopped::Failed)
            } else if goal::done(reply) || closing.is_some_and(goal::done) {
                Some(Stopped::Done)
            } else if goal::asks_user(reply) || closing.is_some_and(goal::asks_user) {
                Some(Stopped::NeedsUser)
            } else {
                None
            };
            if let Some(why) = why {
                goal::finish(&self.hub, &self.cfg.id, Some(&id), why);
            } else if let Err(error) = self.hub.send_cmd(&self.cfg.id, super::Cmd::Goal(id.clone())) {
                tracing::error!(%error, "couldn't queue goal step");
                goal::finish(&self.hub, &self.cfg.id, Some(&id), Stopped::Failed);
            }
        }
    }

    pub(super) async fn dequeue_goal(&mut self, id: String, done_tx: &tokio::sync::mpsc::UnboundedSender<Done>) {
        let backend = self.next_agent_backend();
        match goal::begin(&self.hub, &self.cfg.id, &id, &backend) {
            Ok(Some(text)) => self.start_goal(id, text, done_tx).await,
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, "couldn't begin goal step");
                goal::finish(&self.hub, &self.cfg.id, Some(&id), Stopped::Failed);
            }
        }
    }

    /// Startup awaits the agent; recheck after those awaits before sending any work.
    pub(super) fn goal_can_prompt(&self) -> bool {
        let Some(step) = self.goal_step.as_ref() else { return true };
        if step.stopped.is_some() {
            return false;
        }
        let id = step.id.as_str();
        let Some(g) = goal::get(&self.hub, &self.cfg.id).filter(|g| g.id == id && g.stopped.is_none()) else {
            return false;
        };
        if now_ms().saturating_sub(g.started_at) >= i64::from(g.max_minutes) * 60_000 {
            goal::finish(&self.hub, &self.cfg.id, Some(id), Stopped::Limit);
            return false;
        }
        if crate::usage::near_limit(&self.hub, &self.cfg.backend) {
            goal::finish(&self.hub, &self.cfg.id, Some(id), Stopped::Usage);
            return false;
        }
        true
    }

    pub(super) async fn start_goal(
        &mut self,
        id: String,
        text: String,
        done_tx: &tokio::sync::mpsc::UnboundedSender<Done>,
    ) {
        let Some(g) = goal::get(&self.hub, &self.cfg.id).filter(|g| g.id == id && g.stopped.is_none()) else { return };
        let deadline = g.started_at + i64::from(g.max_minutes) * 60_000;
        let remaining = deadline.saturating_sub(now_ms()).max(0);
        let timeout = std::time::Duration::from_millis(u64::try_from(remaining).unwrap_or(0));
        self.goal_step = Some(Step { id: id.clone(), deadline, stopped: None });
        let lane = crate::store::Lane::main(&self.cfg.id);
        let result = tokio::time::timeout(timeout, self.start_turn(lane, &[], &text, false, done_tx)).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => self.start_error(error, done_tx).await,
            Err(_) => {
                self.drop_unanswered().await;
                goal::finish(&self.hub, &self.cfg.id, Some(&id), Stopped::Limit);
                self.start_error(anyhow::anyhow!("goal time limit reached while starting the agent"), done_tx).await;
            }
        }
    }

    pub(super) async fn check_goal_deadline(&mut self) {
        // Time includes waiting in the queue, but that must not cancel an unrelated user turn.
        let current = goal::get(&self.hub, &self.cfg.id);
        let now = now_ms();
        let active_expired = current
            .as_ref()
            .is_some_and(|g| g.stopped.is_none() && now >= g.started_at + i64::from(g.max_minutes) * 60_000);
        if active_expired && let Some(g) = &current {
            goal::finish(&self.hub, &self.cfg.id, Some(&g.id), Stopped::Limit);
        }
        let Some(step) = self.goal_step.as_ref() else { return };
        let id = step.id.as_str();
        let step_expired = now >= step.deadline;
        if self.turn.is_none() || self.stop_requested {
            return;
        }
        // A user message, approval, or replacement ends automatic continuation, but the step it
        // interrupted keeps the deadline it started with.
        let cancel = match current {
            Some(g) if g.id == id => step_expired || matches!(g.stopped, Some(Stopped::Manual | Stopped::Limit)),
            // Replacing/clearing the goal also cancels the old generation's running step.
            _ => true,
        };
        if cancel {
            self.cancel_turn().await;
        }
    }
}
