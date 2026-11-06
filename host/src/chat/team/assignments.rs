//! Records both deferred messages and synchronous asks in the fork's job ledger.

use super::RequestStatus;
use crate::hub::Hub;
use crate::store::BotConfig;
use crate::store::runtime::{
    AssignmentMode, AssignmentOrigin, AssignmentOutcome, DEFAULT_STEP_BUDGET, NewAssignment, ReplyMode,
};
use anyhow::Result;

pub(super) fn enqueue(hub: &Hub, id: &str, source: &BotConfig, target: &BotConfig, waiting: bool) -> Result<()> {
    hub.store.enqueue_assignment(&NewAssignment {
        assignment_id: id,
        parent_id: None,
        origin: AssignmentOrigin::Person,
        requester_bot_id: Some(&source.id),
        assignee_bot_id: &target.id,
        thread_id: None,
        mode: if waiting { AssignmentMode::Work } else { AssignmentMode::Notify },
        reply_mode: ReplyMode::Resume,
        own_permission: target.permission,
        inherited_permission: source.permission,
        step_budget: DEFAULT_STEP_BUDGET,
    })?;
    Ok(())
}

pub(super) fn started(hub: &Hub, id: &str) {
    if let Err(error) = hub.store.mark_assignment_started(id) {
        tracing::warn!(assignment = id, error = format!("{error:#}"), "couldn't mark bot assignment started");
    }
}

pub(super) fn settle(hub: &Hub, id: &str, status: RequestStatus, detail: &str) {
    let outcome = match status {
        RequestStatus::Completed => AssignmentOutcome::Completed,
        RequestStatus::Failed => AssignmentOutcome::Failed,
        RequestStatus::Cancelled => AssignmentOutcome::Cancelled,
        RequestStatus::Queued | RequestStatus::Sent => return,
    };
    if let Err(error) = hub.store.settle_assignment(id, outcome, Some(detail)) {
        tracing::warn!(assignment = id, error = format!("{error:#}"), "couldn't settle bot assignment");
    }
}
