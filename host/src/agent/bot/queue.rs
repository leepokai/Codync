//! Commands from clients and the queue of turns waiting for the bot.

use super::{Actor, Cmd, Done, NoticeStyle, Queued, RESUME_PROMPT, Retirement, RoutineCompletion, inflight_key};
use crate::chat::memory;
use crate::hub::BotStatus;
use crate::store::{Lane, now_ms};
use anyhow::{Result, anyhow};
use serde_json::json;
use std::time::Instant;
use tokio::sync::mpsc;

impl Actor {
    pub(super) async fn on_cmd(&mut self, cmd: Cmd, done_tx: &mpsc::UnboundedSender<Done>) -> bool {
        match cmd {
            Cmd::RefreshTools => {
                self.tools_changed = true;
            }
            Cmd::Routine(id) => {
                self.queue.push_back(Queued::Routine(id));
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::Send { lane, entry_id, text } => {
                self.queue.push_back(Queued::User { lane, entry_id, text });
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::Group(turn) => {
                self.queue.push_back(Queued::Group(turn));
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::CancelGroup { chat } => {
                // Dropping a queued turn's reply tells the room it won't come.
                self.queue.retain(|q| !matches!(q, Queued::Group(g) if g.lane.chat == chat));
                if self.active_group.as_ref().is_some_and(|g| g.lane.chat == chat) {
                    self.cancel_turn().await;
                }
            }
            Cmd::BotRequest(ask) => {
                self.queue.push_back(Queued::BotRequest(ask));
                if self.turn.is_none() {
                    self.next_in_queue(done_tx).await;
                }
            }
            Cmd::CancelAsk { id } => {
                self.queue.retain(|q| !matches!(q, Queued::BotRequest(a) if a.expects_reply() && a.id == id));
                if self.active_request.as_ref().is_some_and(|a| a.expects_reply() && a.id == id) {
                    self.hub.team.cancel_from(&self.cfg.id);
                    self.stop_requested = true;
                    // The request has expired. Kill only its process so a harness
                    // ignoring cooperative cancellation cannot block the queue.
                    self.retire_agent(Retirement::Timeout).await;
                }
            }
            Cmd::SendToUser { text, reply } => {
                let _ = reply.send(self.send_to_user(text));
            }
            Cmd::Stop => self.stop().await,
            Cmd::Permission { entry_id, option_id } => self.answer_permission(&entry_id, option_id).await,
            Cmd::NewSession => {
                if self.turn.is_some() {
                    self.stop().await;
                }
                if let Err(error) = self.release_main_session().await {
                    self.notice(&format!("{error:#}"), NoticeStyle::Error);
                    return true;
                }
                self.forget_session();
                // Retain and summarize completed exchanges even when the user starts afresh.
                let _ = self.keeper.send(memory::KeeperEvent::Flush);
                self.notice("New session — the agent starts with a fresh context.", NoticeStyle::Divider);
            }
            Cmd::Reconfigure(cfg) => {
                let cfg = *cfg;
                let restart = cfg.backend != self.cfg.backend
                    || cfg.command != self.cfg.command
                    || cfg.cwd != self.cfg.cwd
                    || cfg.model != self.cfg.model;
                self.tools_changed |= cfg.connectors != self.cfg.connectors || cfg.computer != self.cfg.computer;
                // Name, description and skill changes reach the agent as a profile update
                // on the next message (see `context`).
                self.cfg = cfg;
                if restart {
                    self.retire_agent(Retirement::Reconfigure).await;
                    for root in self.thread_roots() {
                        self.set_session(Some(&root), None);
                    }
                    if self.session_id.is_some() {
                        self.forget_session();
                        self.notice(
                            "The agent, model, command or folder changed — the agent starts with a fresh context.",
                            NoticeStyle::Divider,
                        );
                    }
                    // A running prompt now fails; finish_turn reports it.
                }
            }
            Cmd::Shutdown => return false,
        }
        true
    }

    pub(super) async fn stop(&mut self) {
        self.hub.team.cancel_from(&self.cfg.id);
        if let Err(error) = self.hub.routines.cancel_queued(&self.hub.store, &self.cfg.id) {
            tracing::error!(error = format!("{error:#}"), "couldn't cancel queued routines");
        }
        if self.active_routine.is_none() {
            self.hub.routines.release(&self.cfg.id);
        }
        // Queued messages are dropped too: Stop means "stop everything".
        for queued in self.queue.drain(..) {
            let Queued::User { entry_id, .. } = queued else { continue };
            if let Some(mut e) = self.hub.store.entry(&entry_id) {
                e.data["status"] = "cancelled".into();
                self.hub.set_entry(&entry_id, &e.data);
            }
        }
        self.cancel_turn().await;
    }

    /// Stops the running turn only (queued ones stay).
    pub(super) async fn cancel_turn(&mut self) {
        if self.turn.is_none() {
            return;
        }
        self.stop_requested = true;
        let ids: Vec<String> = self.perms.keys().cloned().collect();
        for id in ids {
            self.answer_permission(&id, None).await;
        }
        if let (Some(c), Some(sid)) = (&self.conn, &self.turn_session) {
            let _ = c.acp.notify("session/cancel", json!({"sessionId": sid})).await;
        }
    }

    pub(super) async fn next_in_queue(&mut self, done_tx: &mpsc::UnboundedSender<Done>) {
        let main = Lane::main(&self.cfg.id);
        while self.turn.is_none() && self.routine_completion.is_none() && !self.queue.is_empty() {
            match self.queue.front() {
                Some(Queued::Routine(_)) => {
                    let Some(Queued::Routine(id)) = self.queue.pop_front() else { unreachable!("front is a routine") };
                    self.active_routine = Some(id.clone());
                    self.routine_timed_out = false;
                    match self.hub.routines.begin(&self.hub, &self.cfg.id, &id) {
                        Ok(Some(prepared)) => {
                            self.routine_deadline = Some(Instant::now() + prepared.timeout);
                            let result = self.start_turn(prepared.lane, &[], &prepared.prompt, false, done_tx).await;
                            self.turn_text = None;
                            if let Err(error) = result {
                                self.start_failed(&error);
                            }
                        }
                        Ok(None) => {
                            self.active_routine = None;
                            self.hub.routines.release(&self.cfg.id);
                        }
                        Err(error) => {
                            self.finish_routine(
                                crate::routines::Status::Failed,
                                Some(format!("Could not start routine: {error:#}")),
                                None,
                            );
                        }
                    }
                    continue;
                }
                Some(Queued::BotRequest(_)) => {
                    let Some(Queued::BotRequest(mut ask)) = self.queue.pop_front() else {
                        unreachable!("front is an ask")
                    };
                    if ask.reply_closed() {
                        continue;
                    }
                    let ids = [ask.entry_id.clone()];
                    let prompt = ask.prompt.clone();
                    ask.mark_started();
                    self.active_request = Some(ask);
                    let result = self.start_turn(main.clone(), &ids, &prompt, false, done_tx).await;
                    // Another bot's words are not facts learned from the user.
                    self.turn_text = None;
                    if let Err(e) = result {
                        self.start_failed(&e);
                    }
                    continue;
                }
                Some(Queued::Group(_)) => {
                    let Some(Queued::Group(turn)) = self.queue.pop_front() else {
                        unreachable!("front is a group turn")
                    };
                    if turn.reply.is_closed() {
                        continue;
                    }
                    let (lane, prompt) = (turn.lane.clone(), turn.prompt.clone());
                    self.active_group = Some(turn);
                    let result = self.start_turn(lane, &[], &prompt, false, done_tx).await;
                    // The room is not the user's private chat: nothing here feeds memory.
                    self.turn_text = None;
                    if let Err(e) = result {
                        self.start_failed(&e);
                    }
                    continue;
                }
                _ => {}
            }
            // Messages sent while the agent worked go out together as its next turn
            // without crossing a bot request or another lane: each gets its own reply.
            let Some(Queued::User { lane, .. }) = self.queue.front() else { continue };
            let lane = lane.clone();
            let mut batch = Vec::new();
            while let Some(Queued::User { lane: l, .. }) = self.queue.front()
                && *l == lane
            {
                if let Some(Queued::User { entry_id, text, .. }) = self.queue.pop_front() {
                    batch.push((entry_id, text));
                }
            }
            let ids: Vec<String> = batch.iter().map(|(id, _)| id.clone()).collect();
            let text = batch.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n\n");
            if let Err(e) = self.start_turn(lane, &ids, &text, false, done_tx).await {
                self.start_failed(&e);
            }
        }
    }

    pub(super) fn start_failed(&mut self, e: &anyhow::Error) {
        self.finish_routine(crate::routines::Status::Failed, Some(format!("Could not start routine: {e:#}")), None);
        self.complete_request(Err(anyhow!("couldn't start recipient: {e:#}")), false);
        self.complete_group(Err(anyhow!("couldn't start: {e:#}")));
        let mut msg = format!("Couldn't start the agent: {e}");
        if e.to_string().to_lowercase().contains("auth")
            && let Some(h) = crate::agent::backends::harness(&self.cfg.backend)
        {
            msg = format!("{0} needs you to sign in. Open {0} in Marketplace to sign in.", h.name);
        }
        self.notice(&msg, NoticeStyle::Error);
        self.turn = None;
        if self.routine_completion.is_none() {
            self.set_inflight(false);
        }
        self.hub.set_runtime(&self.id(), |r| {
            r.status = BotStatus::Error;
            r.activity = "Couldn't start".into();
            r.started_at = None;
            r.lane = None;
        });
        self.hub.team.cancel_from(&self.cfg.id);
    }

    pub(super) fn complete_request(&mut self, result: Result<String>, cancelled: bool) {
        if let Some(ask) = self.active_request.take() {
            ask.complete(result, cancelled);
        }
    }

    pub(super) fn complete_group(&mut self, result: Result<Option<String>>) {
        if let Some(turn) = self.active_group.take() {
            let _ = turn.reply.send(result);
        }
    }

    pub(super) fn finish_routine(
        &mut self,
        status: crate::routines::Status,
        detail: Option<String>,
        text: Option<String>,
    ) {
        if let Some(id) = self.active_routine.take() {
            self.routine_completion = Some(RoutineCompletion { id, status, detail, text });
            self.routine_deadline = None;
            self.retry_routine_completion();
        }
    }

    /// If SQLite is temporarily unavailable, keep the result and block this
    /// actor's next turn until the durable completion has been saved.
    pub(super) fn retry_routine_completion(&mut self) -> bool {
        let Some(completion) = &self.routine_completion else {
            return true;
        };
        match self.hub.routines.complete(
            &self.hub,
            &completion.id,
            completion.status,
            completion.detail.clone(),
            completion.text.clone(),
        ) {
            Ok(()) => {
                self.routine_completion = None;
                true
            }
            Err(error) => {
                tracing::error!(
                    error = format!("{error:#}"),
                    run = completion.id,
                    "couldn't save routine result; will retry"
                );
                false
            }
        }
    }

    /// Grok Bot's upgrade resume: the previous host process stopped mid-turn, so
    /// the agent is told, in the same session, to finish without redoing steps.
    pub(super) async fn resume_interrupted(&mut self, lane: Lane, done_tx: &mpsc::UnboundedSender<Done>) {
        self.set_inflight(false);
        if self.session_for(lane.thread.as_deref()).is_none() {
            self.finish_routine(
                crate::routines::Status::Interrupted,
                Some("No saved agent session is available; inspect this run before retrying".into()),
                None,
            );
            return;
        }
        let prompt = if self.active_routine.is_some() {
            format!(
                "{RESUME_PROMPT} This is still a scheduled routine run. If its instruction permits silence and there is nothing to report, return exactly (pass)."
            )
        } else {
            RESUME_PROMPT.to_owned()
        };
        if let Err(e) = self.start_turn(lane, &[], &prompt, true, done_tx).await {
            self.finish_routine(
                crate::routines::Status::Interrupted,
                Some(format!("Could not resume this run's saved conversation: {e:#}; inspect before retrying")),
                None,
            );
            self.start_failed(&e);
        }
    }

    /// Marks a turn in the bot's own chat or a thread as running across host restarts
    /// (see [`Self::resume_interrupted`]); the thread root rides along.
    pub(super) fn set_inflight(&self, on: bool) {
        let value = match (&self.lane.thread, on) {
            (_, false) => String::new(),
            (Some(root), true) => format!("{}\t{root}", now_ms()),
            (None, true) => now_ms().to_string(),
        };
        if let Err(error) = self.hub.store.kv_set(&inflight_key(&self.cfg.id), &value) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the running turn");
        }
    }
}
