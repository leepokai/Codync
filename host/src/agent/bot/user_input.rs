//! Building a user turn without mixing lanes, proactive input, or revoked platform messages.

use super::{Actor, Done, Queued};
use crate::store::Lane;
use tokio::sync::mpsc;

#[derive(PartialEq, Eq)]
pub(super) enum Progress {
    Continue,
    Deferred,
}

impl Actor {
    pub(super) async fn advance_user_input(&mut self, main: &Lane, done_tx: &mpsc::UnboundedSender<Done>) -> Progress {
        // Messages sent while the agent worked go out together as its next turn
        // without crossing a bot request or reply or another lane: each gets its own reply.
        let Some(Queued::User { lane, entry_id, .. }) = self.queue.front() else { return Progress::Continue };
        let proactive = self.proactive_input(entry_id);
        let lane = lane.clone();
        let mut batch = Vec::new();
        let mut claim_deferred = false;
        while let Some(Queued::User { lane: l, entry_id, .. }) = self.queue.front()
            && *l == lane
            && self.proactive_input(entry_id) == proactive
            && (!proactive || batch.is_empty())
        {
            let ahead = self.queue.len().saturating_sub(1);
            if let Some(Queued::User { entry_id, text, queued_at, .. }) = self.queue.pop_front() {
                crate::analytics::turn_queued(
                    &self.hub,
                    &self.cfg.id,
                    i64::try_from(queued_at.elapsed().as_millis()).unwrap_or(i64::MAX),
                    ahead,
                );
                match super::channel_input::claim(&self.hub, &entry_id) {
                    Ok(true) => {}
                    Ok(false) => {
                        super::channel_input::cancel(&self.hub, &entry_id);
                        continue;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Channel input authorization failed; retaining queued input");
                        self.queue.push_front(Queued::User { lane: lane.clone(), entry_id, text, queued_at });
                        claim_deferred = true;
                        break;
                    }
                }
                batch.push((entry_id, text));
            }
        }
        let ids: Vec<String> = batch.iter().map(|(id, _)| id.clone()).collect();
        if batch.is_empty() {
            return if claim_deferred { Progress::Deferred } else { Progress::Continue };
        }
        let text = batch.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n\n");
        // A handoff waiting in the chat starts a new conversation with this message, as does one already taken
        // for a turn that couldn't start.
        if &lane == main {
            self.handoff = self.take_handoff().or_else(|| self.handoff.take());
        }
        if let Err(e) = self.start_turn(lane, &ids, &text, false, done_tx).await {
            self.start_error(e, done_tx).await;
        }
        Progress::Continue
    }
}
