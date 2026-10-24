//! Starting a turn and wrapping it up: final reply, notices, the reply to a requesting bot or room, alerts and memory.

use super::{Actor, Conn, Done, Instructions, NoticeStyle, Queued, Reacted, Seg};
use crate::agent::acp::{self, Incoming};
use crate::analytics::{self, TurnOutcome, TurnSource};
use crate::chat::context::{self, Snapshot};
use crate::chat::memory;
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{EntryKind, Lane, now_ms};
use anyhow::{Context, Result, anyhow};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Leads a `<bot-profile>` sent again after a compaction, so the agent drops an older copy it may still hold.
const PROFILE_AGAIN: &str = "Your context was summarized. This is your full, current profile and memory; it replaces any earlier bot profile in this conversation.";

impl Actor {
    /// This session's frozen instructions (reads memory files when re-rendering).
    pub(super) async fn snapshot(&self, session: &str) -> Result<Snapshot> {
        let (hub, cfg, session) = (self.hub.clone(), self.cfg.clone(), session.to_owned());
        tokio::task::spawn_blocking(move || context::resolve(&hub.store, &cfg, &session)).await?
    }

    /// The prompt for `text` in session `sid`: what the agent should hear first (its profile, a handoff, a memory
    /// change) and after (a profile update, how to share files).
    async fn compose(&mut self, sid: &str, text: &str) -> Result<String> {
        let instructions = self.conn.as_ref().map_or(Instructions::FirstMessage, Conn::instructions);
        let mut snapshot = self.snapshot(sid).await?;
        // Sessions whose frozen instructions predate send_file or artifacts hear about them each turn.
        let remind_files = !snapshot.system.contains("send_file");
        let remind_artifacts = !snapshot.system.contains("publish_artifact");
        let mut prompt = text.to_owned();
        // The user's handoff first, then what the fallback chain carries in.
        for first in [self.relay.carried.take(), self.handoff.take()].into_iter().flatten() {
            prompt = format!("{first}\n\n{prompt}");
        }
        // A compaction may have dropped the profile the session's first message carried.
        let resend = instructions == Instructions::FirstMessage && self.profile_resend.remove(sid);
        if self.session_fresh || resend {
            // The instructions reaching the agent now hold memory as it was when they were rendered.
            memory::opened(&self.hub.store, &self.cfg.id, sid, &snapshot.memory)?;
        }
        if let Some((notice, revision)) = memory::change_notice(&self.hub.store, &self.cfg, sid) {
            prompt = format!("{notice}\n\n{prompt}");
            self.turn_memory_revision = revision;
        }
        if std::mem::take(&mut self.session_fresh) && instructions == Instructions::FirstMessage {
            prompt = format!("<bot-profile>\n{}\n</bot-profile>\n\n{prompt}", snapshot.system);
        } else if resend {
            context::mark_announced(&self.hub.store, &self.cfg.id, &snapshot, snapshot.system_identity.clone())?;
            snapshot.announced = snapshot.system_identity.clone();
            prompt = format!("<bot-profile>\n{PROFILE_AGAIN}\n\n{}\n</bot-profile>\n\n{prompt}", snapshot.system);
        }
        if let Some((update, identity)) = context::profile_update(&self.hub.store, &snapshot, &self.cfg) {
            prompt = format!("{prompt}\n\n{update}");
            self.announce = Some((snapshot, identity));
        }
        let own_chat = self.active_group.is_none() && self.active_request.is_none() && self.active_routine.is_none();
        if remind_files && own_chat {
            prompt.push_str("\n\n[Codync: send_file(path, name?) shares any regular file up to 100 MiB as a downloadable card. Use it instead of sending a local path.]");
        }
        if remind_artifacts && own_chat {
            prompt.push_str("\n\n[Codync: for a report, plan, tracker or dashboard the user will come back to, write one self-contained HTML file and publish_artifact(path, title) instead of a long message. To change it, edit a copy of its HTML (list_artifacts) and publish_artifact(path, artifact: its id).]");
        }
        Ok(prompt)
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
        // A routine run keeps the turn its notice took when it began (`Routines::begin`).
        let run_turn = self
            .active_routine
            .as_ref()
            .and_then(|id| self.hub.store.entry(&crate::routines::notice_id(id)))
            .map(|e| e.turn)
            .filter(|t| *t > 0);
        let turn = entry_ids.last().and_then(|id| self.hub.store.entry(id)).map_or_else(
            || {
                run_turn.unwrap_or_else(|| {
                    self.hub.store.max_turn(&lane.chat) + i64::from(self.source() != TurnSource::User)
                })
            },
            |e| e.turn,
        );
        for id in entry_ids {
            // Bot-message notices belong to `chat::team`; writing them here would race its final status.
            if let Some(mut e) = self.hub.store.entry(id).filter(|e| e.kind == EntryKind::User.as_str()) {
                e.data["status"] = json!(crate::chat::team::RequestStatus::Sent);
                self.hub.set_entry(id, &e.data);
            }
        }
        self.lane = lane.clone();
        self.turn = Some(turn);
        // Only the user's own words feed memory: not another bot's, a room's or a routine's.
        let from_user = !hidden && self.source() == TurnSource::User;
        self.turn_text = from_user.then(|| text.to_owned());
        self.turn_entries = if from_user { entry_ids.to_vec() } else { Vec::new() };
        self.turn_memory_revision.clear();
        self.announce = None;
        self.stop_requested = false;
        self.cancel_deadline = None;
        let team_turn = if self.active_request.is_some() {
            crate::chat::team::Turn::Request
        } else {
            crate::chat::team::Turn::Chat(self.reply_hops.unwrap_or(0))
        };
        self.hub.team.start_turn(&self.cfg.id, team_turn);
        self.exit_tail = None;
        self.seg = Seg::None;
        self.tools.clear();
        self.plan_entry = None;
        self.last_text = None;
        self.sent.clear();
        self.reacted = Reacted::No;
        self.files.summary = None;
        self.files.cancel();
        if hidden {
            self.hidden_turn();
        } else {
            self.relay_turn(&lane, entry_ids, text).await;
        }
        // A bot request or room turn isn't resumed after a host restart (the room that waited is gone): a request's
        // persisted notices are marked interrupted instead of silently repeating work. A reply turn is resumed.
        self.set_inflight(
            self.active_request.is_none() && self.active_group.is_none() && self.active_routine.is_none(),
        );
        // A message the host wrote in the user's place (`from: host`) isn't the user asking.
        let typed = |id: &&String| self.hub.store.entry(id).is_some_and(|e| e.data["from"] != "host");
        let message = if from_user { entry_ids.iter().rev().find(typed).cloned() } else { None };
        self.hub.set_runtime(&self.id(), |r| {
            r.status = BotStatus::Working;
            r.activity = match (self.compacting.is_some(), hidden) {
                (true, _) => "Summarizing the earlier conversation…",
                (false, true) => "Picking up where it left off…",
                (false, false) => "Starting…",
            }
            .into();
            r.started_at = Some(now_ms());
            r.lane = Some(lane.clone());
            r.message = message;
        });

        // Memory files changed since (in an editor, say) reach the opening or the next notice.
        let (hub, cfg) = (self.hub.clone(), self.cfg.clone());
        let _ = tokio::task::spawn_blocking(move || memory::files::scan(&hub, &cfg)).await;
        let slot = self.slot();
        if let Some(deadline) = self.routine_deadline {
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), self.ensure_session(slot.as_deref()))
                .await
                .context("routine session setup timed out")??;
        } else {
            self.ensure_session(slot.as_deref()).await?;
        }
        if !hidden {
            self.carry_in();
        }
        let session = self.turn_session.clone();
        self.hub.set_runtime(&self.id(), |r| r.session.clone_from(&session));
        if let Some(sid) = session {
            self.sync_fast_mode(&sid).await;
        }
        if hidden && self.session_fresh {
            // A new session must never silently repeat an interrupted routine, and has nothing to compact.
            self.compacting = None;
            self.raw_prompt = false;
            self.finish_routine(
                crate::routines::Status::Interrupted,
                Some("The agent could not reload this run's saved session".into()),
                None,
            );
            self.turn = None;
            if self.routine_completion.is_none() {
                self.set_inflight(false);
            }
            self.hub.team.end_turn(&self.cfg.id);
            self.hub.set_runtime(&self.id(), |r| {
                r.status = BotStatus::Idle;
                r.activity = String::new();
                r.started_at = None;
                r.lane = None;
                r.message = None;
                r.session = None;
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
        let sid = self.turn_session.clone().ok_or_else(|| anyhow!("no session"))?;
        let prompt =
            if std::mem::take(&mut self.raw_prompt) { text.to_owned() } else { self.compose(&sid, text).await? };
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
        // From here what changes in its memory folders is the bot's, however it changed them.
        memory::turns::begin_turn(&self.cfg.id, &sid, self.hub.runtime(&self.cfg.id).message);
        // Written before this returns, so a Stop right after is sent after the prompt.
        let answer = acp.send("session/prompt", params).await?;
        if !self.turn_memory_revision.is_empty()
            && let Err(error) = memory::mark_announced(&self.hub.store, &self.cfg.id, &sid, &self.turn_memory_revision)
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
        self.after_turn(done);
        // The bot's own edits to its memory files, checked and shown in the chat it made them from.
        memory::turns::scan_after_turn(self.hub.clone(), self.cfg.clone());
        if let (Some(conn), Some(sid)) = (self.conn.as_mut(), &self.turn_session)
            && let Some(used) = conn.loaded.get_mut(sid)
        {
            *used = Instant::now();
        }
        let stopped = self.stop_requested || matches!(&done, Ok(v) if v["stopReason"] == "cancelled");
        self.close_attempt(stopped || done.is_err());
        let mut final_text = None;
        let routine = self.active_routine.is_some();
        let grouped = self.active_group.is_some() || routine;
        let delegated = self.active_request.is_some();
        let sent = std::mem::take(&mut self.sent);
        let reacted = std::mem::take(&mut self.reacted);
        if !sent.is_empty() {
            // The bot talked through send_message: what it wrote as its reply stays in the trace.
            self.last_text = None;
            final_text = Some(sent.join("\n\n"));
        } else if !matches!(reacted, Reacted::No) {
            // A reaction (or one it held back) was its whole reply: what it wrote stays in the trace too.
            self.last_text = None;
        }
        if let Some(id) = self.last_text.take().filter(|_| !stopped)
            && let Some(mut e) = self.hub.store.entry(&id)
        {
            let text = e.data["text"].as_str().map(str::to_owned);
            // A pass stays in the trace: neither a room nor the user sees it (a bot request's reply goes back as it is).
            if delegated || !text.as_deref().is_none_or(crate::chat::group::is_pass) {
                // A bot request's answer belongs to the bot that sent it and the exchange notice.
                if !delegated {
                    e.data["final"] = true.into();
                    self.hub.set_entry(&id, &e.data);
                }
                final_text = text;
            }
        }
        if final_text.is_none() {
            final_text = self.files.summary.take();
        }
        let stop_reason = match &done {
            Ok(v) => v["stopReason"].as_str().unwrap_or("end_turn").to_owned(),
            Err(_) => "error".into(),
        };
        match (&done, stop_reason.as_str()) {
            (Err(e), _) if !self.stop_requested => {
                let tail = self.exit_tail.take().filter(|t| !t.is_empty());
                if !self.invite_sign_in(e) {
                    let detail = tail.map(|t| format!("\n\n{}", acp::truncate(&t, 1200))).unwrap_or_default();
                    self.failure_notice(e, &format!("The agent failed: {e}{detail}"));
                }
            }
            (_, "cancelled") | (Err(_), _) => self.notice("Stopped.", NoticeStyle::Info),
            (_, "max_tokens") => self.notice("The agent hit its output limit.", NoticeStyle::Info),
            (_, "max_turn_requests") => self.notice("The agent hit its step limit for this turn.", NoticeStyle::Info),
            (_, "refusal") => self.notice("The agent declined to continue.", NoticeStyle::Info),
            (Ok(_), "end_turn") if !stopped => crate::market::agent_sign_in::answered(&self.hub, &self.cfg.backend),
            _ => {}
        }
        let failure = done.as_ref().err().filter(|_| !self.stop_requested);
        let failed = failure.is_some();
        self.report_turn(stopped, failure, &stop_reason);
        if done.is_ok() && stop_reason == "end_turn" {
            self.agent_answered();
        }
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
        self.complete_request(reply, stopped);
        self.complete_group(if stopped || failed || stop_reason != "end_turn" {
            Err(anyhow!("turn did not complete ({stop_reason})"))
        } else {
            Ok(final_text.clone())
        });
        self.turn = None;
        self.reply_hops = None;
        if self.routine_completion.is_none() {
            self.set_inflight(false);
        }
        // The message reached the agent: the profile update it carried is now known to it.
        if let (Ok(_), Some((snapshot, identity))) = (&done, self.announce.take())
            && let Err(error) = context::mark_announced(&self.hub.store, &self.cfg.id, &snapshot, identity)
        {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the profile update");
        }
        if let (Some(user), Some(agent), "end_turn") = (self.turn_text.take(), &final_text, stop_reason.as_str()) {
            let turn = crate::chat::naming::Turn { user, agent: agent.clone() };
            let _ = self.dreamer.send(memory::DreamEvent::Turn(turn));
        }
        self.hub.set_runtime(&self.id(), |r| {
            r.status = if failed { BotStatus::Error } else { BotStatus::Idle };
            r.activity = String::new();
            r.started_at = None;
            r.lane = None;
            r.message = None;
            r.session = None;
        });
        self.hub.team.end_turn(&self.cfg.id);
        // A room turn's news comes from the group once the room is done (see `group`).
        let more = self.queue.iter().any(|q| matches!(q, Queued::User { .. }));
        // It held back its only answer, a reaction: it said nothing, so there is nothing to tell either.
        let held_back = final_text.is_none() && matches!(reacted, Reacted::Refused);
        if !self.stop_requested
            && !delegated
            && !grouped
            && let Some(kind) = turn_alert(failed, &stop_reason, more || held_back)
        {
            let body = final_text.unwrap_or_else(|| match (kind, reacted) {
                (AlertKind::Done, Reacted::With(emoji)) => format!("Reacted {emoji} to your message"),
                (AlertKind::Done, _) => "Finished.".into(),
                _ => "The agent didn't finish.".into(),
            });
            push::notify(&self.hub, &self.cfg, None, &self.cfg.name, &body, kind);
        }
    }
}

impl Actor {
    /// Ends what an attempt left open: file shares, the last text, unanswered permission cards (they can't be
    /// answered after the turn) and tool calls the agent never closed out (they'd spin forever in the trace).
    pub(super) fn close_attempt(&mut self, failed: bool) {
        self.files.cancel();
        self.flush(true);
        self.seg = Seg::None;
        let ids: Vec<String> = self.perms.drain().map(|(k, _)| k).collect();
        for id in ids {
            if let Some(mut e) = self.hub.store.entry(&id) {
                e.data["status"] = "expired".into();
                self.hub.set_entry(&id, &e.data);
            }
        }
        for entry_id in self.tools.values() {
            if let Some(mut e) = self.hub.store.entry(entry_id)
                && matches!(e.data["status"].as_str(), Some("pending" | "in_progress"))
            {
                e.data["status"] = if failed { "failed" } else { "completed" }.into();
                self.hub.set_entry(entry_id, &e.data);
            }
        }
    }

    /// `turn_finished`, while the turn's start time is still in the runtime.
    pub(super) fn report_turn(&self, stopped: bool, failure: Option<&anyhow::Error>, stop_reason: &str) {
        let outcome = if stopped {
            TurnOutcome::Stopped
        } else if failure.is_some() {
            TurnOutcome::Failed
        } else if stop_reason == "end_turn" {
            TurnOutcome::Done
        } else {
            TurnOutcome::Incomplete
        };
        let started = self.hub.runtime(&self.id()).started_at.unwrap_or_else(now_ms);
        let ms = now_ms() - started;
        analytics::turn_finished(&self.hub, &self.cfg.backend, outcome, stop_reason, failure, self.source(), ms);
    }

    /// What started the turn that is starting or running.
    fn source(&self) -> TurnSource {
        if self.active_routine.is_some() {
            TurnSource::Routine
        } else if self.active_group.is_some() {
            TurnSource::Group
        } else if self.active_request.is_some() || self.reply_hops.is_some() {
            TurnSource::Bot
        } else {
            TurnSource::User
        }
    }
}

/// A finished turn alerts once the user's work is done: not when it's `quiet` (their next message
/// is already queued, or it said nothing on purpose). A turn that ended short of `end_turn` always
/// reports the failure.
fn turn_alert(failed: bool, stop_reason: &str, quiet: bool) -> Option<AlertKind> {
    if failed || !matches!(stop_reason, "end_turn" | "cancelled") {
        return Some(AlertKind::Failed);
    }
    (stop_reason == "end_turn" && !quiet).then_some(AlertKind::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_finished_tasks_alert() {
        assert_eq!(turn_alert(false, "end_turn", false), Some(AlertKind::Done));
        assert_eq!(turn_alert(false, "end_turn", true), None, "next message queued, or a reaction held back");
        assert_eq!(turn_alert(false, "cancelled", false), None);
        assert_eq!(turn_alert(false, "max_turn_requests", false), Some(AlertKind::Failed));
        assert_eq!(turn_alert(true, "error", true), Some(AlertKind::Failed));
    }
}
