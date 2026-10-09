//! Starting a turn and wrapping it up: final reply, notices, replies to waiters, alerts and memory.

use super::{Actor, Done, NoticeStyle, Queued, Seg};
use crate::agent::acp::{self, Incoming};
use crate::chat::context::{self, Snapshot};
use crate::chat::memory;
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{Lane, now_ms};
use anyhow::{Context, Result, anyhow};
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;

impl Actor {
    /// This session's frozen instructions (reads memory files when re-rendering).
    pub(super) async fn snapshot(&self, session: &str) -> Result<Snapshot> {
        let (hub, cfg, session) = (self.hub.clone(), self.cfg.clone(), session.to_owned());
        tokio::task::spawn_blocking(move || context::resolve(&hub.store, &cfg, &session)).await?
    }

    /// Starts a turn in `lane` for the given user entries (none for a hidden turn).
    pub(super) async fn start_turn(
        &mut self,
        lane: Lane,
        entry_ids: &[String],
        text: &str,
        hidden: bool,
        done_tx: &mpsc::UnboundedSender<Done>,
    ) -> Result<()> {
        let turn = entry_ids.last().and_then(|id| self.hub.store.entry(id)).map_or_else(
            || {
                self.hub.store.max_turn(&lane.chat)
                    + i64::from(self.active_group.is_some() || self.active_routine.is_some())
            },
            |e| e.turn,
        );
        for id in entry_ids {
            if let Some(mut e) = self.hub.store.entry(id) {
                e.data["status"] = json!(crate::chat::team::RequestStatus::Sent);
                self.hub.set_entry(id, &e.data);
            }
        }
        self.lane = lane.clone();
        self.turn = Some(turn);
        self.turn_text = (!hidden).then(|| text.to_owned());
        self.turn_memory_revision = memory::maintenance::revision(&self.hub.store, &self.cfg.id);
        self.announce = None;
        self.stop_requested = false;
        self.hub.team.start_turn(&self.cfg.id, self.active_request.as_ref());
        self.exit_tail = None;
        self.seg = Seg::None;
        self.tools.clear();
        self.plan_entry = None;
        self.last_text = None;
        self.sent.clear();
        // A delegated or group turn has no live waiter after a host restart. Its persisted
        // notices are marked interrupted instead of silently repeating work.
        self.set_inflight(
            self.active_request.is_none() && self.active_group.is_none() && self.active_routine.is_none(),
        );
        self.hub.set_runtime(&self.id(), |r| {
            r.status = BotStatus::Working;
            r.activity = if hidden { "Picking up where it left off…" } else { "Starting…" }.into();
            r.started_at = Some(now_ms());
            r.lane = Some(lane.clone());
        });

        let slot = self.slot(&lane);
        if let Some(deadline) = self.routine_deadline {
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), self.ensure_session(slot.as_deref()))
                .await
                .context("routine session setup timed out")??;
        } else {
            self.ensure_session(slot.as_deref()).await?;
        }
        if hidden && self.session_fresh {
            // A new session must never silently repeat an interrupted routine.
            self.finish_routine(
                crate::routines::Status::Interrupted,
                Some("The agent could not reload this run's saved session; inspect before retrying".into()),
                None,
            );
            self.turn = None;
            if self.routine_completion.is_none() {
                self.set_inflight(false);
            }
            self.hub.team.cancel_from(&self.cfg.id);
            self.hub.set_runtime(&self.id(), |r| {
                r.status = BotStatus::Idle;
                r.activity = String::new();
                r.started_at = None;
                r.lane = None;
            });
            return Ok(());
        }
        // Session-start chatter (banners, command lists) isn't part of the reply.
        // Some adapters (pi-acp) send it on a timer right after session/new.
        if self.session_fresh {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        if let Some(conn) = self.conn.as_mut() {
            while let Ok(inc) = conn.rx.try_recv() {
                if let Incoming::Request { id, .. } = inc {
                    let _ = conn.acp.respond_error(id, -32601, "not supported").await;
                }
            }
        }
        let claude = self.conn.as_ref().is_some_and(|c| c.claude);
        let sid = self.turn_session.clone().ok_or_else(|| anyhow!("no session"))?;
        let memory_session = memory::lifecycle::begin(&self.hub, &self.cfg.id, &sid).await?;
        self.memory_session = Some(memory_session.clone());
        if !hidden && let Err(error) = memory::lifecycle::capture_prompt(&self.cfg.id, &memory_session, text).await {
            tracing::warn!(bot = %self.cfg.id, %error, "memory prompt capture failed");
        }
        let snapshot = self.snapshot(&sid).await?;
        let mut prompt = text.to_owned();
        if let Some(notice) = memory::lifecycle::change_notice(&self.hub, &self.cfg.id, &sid) {
            prompt = format!("{notice}\n\n{prompt}");
        }
        if let Some(intro) = self.thread_intro.take() {
            prompt = format!("{intro}\n\n{prompt}");
        }
        if self.session_fresh {
            self.session_fresh = false;
            // Claude already has the instructions as its system prompt.
            if !claude {
                prompt = format!("<bot-profile>\n{}\n</bot-profile>\n\n{prompt}", snapshot.system);
            }
        }
        if let Some((update, identity)) = context::profile_update(&self.hub.store, &snapshot, &self.cfg) {
            prompt = format!("{prompt}\n\n{update}");
            self.announce = Some((snapshot, identity));
        }
        self.hub.set_runtime(&self.id(), |r| r.activity = "Thinking…".into());
        let acp = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?.acp.clone();
        let params = json!({
            "sessionId": sid,
            "prompt": [{"type": "text", "text": prompt}],
        });
        if let Some(id) = &self.active_routine {
            self.hub.routines.mark_started(&self.hub.store, id)?;
            self.set_inflight(true);
        }
        // Written before this returns, so a Stop right after is sent after the prompt.
        let answer = acp.send("session/prompt", params).await?;
        if let Err(error) = memory::lifecycle::mark_announced(&self.hub, &self.cfg.id, &sid, &self.turn_memory_revision)
        {
            tracing::warn!(%error, "could not acknowledge memory update notice");
        }
        let done_tx = done_tx.clone();
        tokio::spawn(async move {
            let _ = done_tx.send(answer.response().await);
        });
        Ok(())
    }

    pub(super) fn finish_turn(&mut self, done: &Done) {
        if self.turn.is_none() {
            return;
        }
        self.flush(true);
        self.seg = Seg::None;
        // Unanswered permission cards can't be answered after the turn.
        let ids: Vec<String> = self.perms.drain().map(|(k, _)| k).collect();
        for id in ids {
            if let Some(mut e) = self.hub.store.entry(&id) {
                e.data["status"] = "expired".into();
                self.hub.set_entry(&id, &e.data);
            }
        }
        let stopped = self.stop_requested || matches!(&done, Ok(v) if v["stopReason"] == "cancelled");
        // Tool calls the agent never closed out would spin forever in the trace.
        for entry_id in self.tools.values() {
            if let Some(mut e) = self.hub.store.entry(entry_id)
                && matches!(e.data["status"].as_str(), Some("pending" | "in_progress"))
            {
                e.data["status"] = if stopped || done.is_err() { "failed" } else { "completed" }.into();
                self.hub.set_entry(entry_id, &e.data);
            }
        }
        let mut final_text = None;
        let routine = self.active_routine.is_some();
        let grouped = self.active_group.is_some() || routine;
        let sent = std::mem::take(&mut self.sent);
        if !sent.is_empty() {
            // The bot talked through send_message: what it wrote as its reply stays in the trace.
            self.last_text = None;
            final_text = Some(sent.join("\n\n"));
        }
        if let Some(id) = self.last_text.take().filter(|_| !stopped)
            && let Some(mut e) = self.hub.store.entry(&id)
        {
            let text = e.data["text"].as_str().map(str::to_owned);
            // A pass in a room stays in the trace; the room doesn't see it.
            if !(grouped && text.as_deref().is_none_or(crate::chat::group::is_pass)) {
                e.data["final"] = true.into();
                self.hub.set_entry(&id, &e.data);
                final_text = text;
            }
        }
        let stop_reason = match &done {
            Ok(v) => v["stopReason"].as_str().unwrap_or("end_turn").to_owned(),
            Err(_) => "error".into(),
        };
        match (&done, stop_reason.as_str()) {
            (Err(e), _) if !self.stop_requested => {
                let tail = self.exit_tail.take().filter(|t| !t.is_empty());
                let detail = tail.map(|t| format!("\n\n{}", acp::truncate(&t, 1200))).unwrap_or_default();
                self.notice(&format!("The agent failed: {e}{detail}"), NoticeStyle::Error);
            }
            (_, "cancelled") | (Err(_), _) => self.notice("Stopped.", NoticeStyle::Info),
            (_, "max_tokens") => self.notice("The agent hit its output limit.", NoticeStyle::Info),
            (_, "max_turn_requests") => self.notice("The agent hit its step limit for this turn.", NoticeStyle::Info),
            (_, "refusal") => self.notice("The agent declined to continue.", NoticeStyle::Info),
            _ => {}
        }
        let failed = done.is_err() && !self.stop_requested;
        let delegated = self.active_request.as_ref().is_some_and(crate::chat::team::BotRequest::expects_reply);
        let reply = if stopped {
            Err(anyhow!("recipient was stopped; partial work may have happened"))
        } else if stop_reason != "end_turn" {
            Err(anyhow!("recipient did not finish ({stop_reason}); partial work may have happened"))
        } else {
            final_text
                .clone()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| anyhow!("recipient finished without a text reply"))
        };
        if routine {
            let (status, detail) = if self.routine_timed_out {
                (
                    crate::routines::Status::Failed,
                    Some("Routine exceeded its execution time limit; its agent process was stopped".into()),
                )
            } else if stopped {
                (crate::routines::Status::Cancelled, Some("Stopped by user".into()))
            } else if failed || stop_reason != "end_turn" {
                (
                    crate::routines::Status::Failed,
                    Some(format!("Routine did not finish ({stop_reason}); inspect the run conversation")),
                )
            } else {
                (crate::routines::Status::Succeeded, None)
            };
            let text = (status == crate::routines::Status::Succeeded).then(|| final_text.clone()).flatten();
            self.finish_routine(status, detail, text);
        }
        let message_failed =
            !stopped && reply.is_err() && self.active_request.as_ref().is_some_and(|r| !r.expects_reply());
        self.complete_request(reply, stopped);
        self.complete_group(if stopped || failed || stop_reason != "end_turn" {
            Err(anyhow!("turn did not complete ({stop_reason})"))
        } else {
            Ok(final_text.clone())
        });
        self.turn = None;
        if self.routine_completion.is_none() {
            self.set_inflight(false);
        }
        // The message reached the agent: the profile update it carried is now known to it.
        if let (Ok(_), Some((snapshot, identity))) = (&done, self.announce.take())
            && let Err(error) = context::mark_announced(&self.hub.store, &self.cfg.id, &snapshot, identity)
        {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the profile update");
        }
        if let (Some(user), Some(agent), "end_turn") = (self.turn_text.take(), &final_text, stop_reason.as_str())
            && memory::is_memorable(&user)
        {
            let _ = self.keeper.send(memory::KeeperEvent::Exchange(memory::Exchange {
                user,
                agent: agent.clone(),
                at: now_ms(),
                session: self.memory_session.clone().unwrap_or_default(),
                revision: self.turn_memory_revision.clone(),
            }));
        }
        self.hub.set_runtime(&self.id(), |r| {
            r.status = if failed { BotStatus::Error } else { BotStatus::Idle };
            r.activity = String::new();
            r.started_at = None;
            r.lane = None;
        });
        self.hub.team.cancel_from(&self.cfg.id);
        // A room turn's news comes from the group once the room is done (see `group`).
        let more = self.queue.iter().any(|q| matches!(q, Queued::User { .. }));
        if !self.stop_requested
            && !delegated
            && !grouped
            && let Some(kind) = turn_alert(failed || message_failed, &stop_reason, more)
        {
            let body = final_text.unwrap_or_else(|| match kind {
                AlertKind::Done => "Finished.".into(),
                _ => "The agent didn't finish.".into(),
            });
            push::notify(&self.hub, &self.cfg, None, &self.cfg.name, &body, kind);
        }
    }
}

/// A finished turn alerts once the user's work is done: not while their next message
/// is already queued. A turn that ended short of `end_turn` always reports the failure.
fn turn_alert(failed: bool, stop_reason: &str, more: bool) -> Option<AlertKind> {
    if failed || !matches!(stop_reason, "end_turn" | "cancelled") {
        return Some(AlertKind::Failed);
    }
    (stop_reason == "end_turn" && !more).then_some(AlertKind::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_finished_tasks_alert() {
        assert_eq!(turn_alert(false, "end_turn", false), Some(AlertKind::Done));
        assert_eq!(turn_alert(false, "end_turn", true), None, "next message queued");
        assert_eq!(turn_alert(false, "cancelled", false), None);
        assert_eq!(turn_alert(false, "max_turn_requests", false), Some(AlertKind::Failed));
        assert_eq!(turn_alert(true, "error", true), Some(AlertKind::Failed));
    }
}
