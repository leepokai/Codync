//! Commands from clients and the queue of turns waiting for the bot.

use super::{Actor, CANCEL_GRACE, Cmd, Done, Queued, RESUME_PROMPT, RoutineCompletion, inflight_key};
use crate::agent::own_memory;
use crate::chat::memory;
use crate::hub::BotStatus;
use crate::market::agent_sign_in;
use crate::store::{Lane, now_ms};
use anyhow::{Result, anyhow};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

impl Actor {
    pub(super) async fn on_cmd(&mut self, cmd: Cmd, done_tx: &mpsc::UnboundedSender<Done>) -> bool {
        match cmd {
            Cmd::RefreshTools => {
                self.tools_changed = true;
            }
            Cmd::Routine(id) => self.enqueue(Queued::Routine(id), done_tx).await,
            Cmd::Send { lane, entry_id, text } => self.enqueue(Queued::User { lane, entry_id, text }, done_tx).await,
            Cmd::Group(turn) => {
                // The room asks again: this turn answers for any still waiting there.
                self.queue.retain(|q| !matches!(q, Queued::Group(g) if g.lane == turn.lane));
                self.enqueue(Queued::Group(turn), done_tx).await;
            }
            Cmd::CancelGroup { chat } => {
                // Dropping a queued turn's reply tells the room it won't come.
                self.queue.retain(|q| !matches!(q, Queued::Group(g) if g.lane.chat == chat));
                if self.active_group.as_ref().is_some_and(|g| g.lane.chat == chat) {
                    self.cancel_turn().await;
                }
            }
            Cmd::BotRequest(request) => self.enqueue(Queued::BotRequest(request), done_tx).await,
            Cmd::BotReply(reply) => self.enqueue(Queued::BotReply(reply), done_tx).await,
            Cmd::Prompt { text, call } => self.enqueue(Queued::Prompt { text, call }, done_tx).await,
            Cmd::SendToUser { text, reply } => {
                let _ = reply.send(self.send_to_user(text));
            }
            Cmd::React { emoji, to, reply } => {
                let _ = reply.send(self.react(&emoji, to.as_deref()));
            }
            Cmd::SendFile { path, name, cancelled, reply } => self.begin_file(path, name, cancelled, reply),
            Cmd::PostArtifact { data, reply } => {
                let _ = reply.send(self.post_artifact(data));
            }
            Cmd::Stop => self.stop().await,
            Cmd::Permission { entry_id, option_id } => self.answer_permission(&entry_id, option_id).await,
            Cmd::Approve { title, detail, reply } => self.ask_approval(&title, &detail, reply),
            Cmd::NewSession => {
                if self.turn.is_some() {
                    self.stop().await;
                }
                self.forget_session();
                self.forget_parked();
                // Retain and summarize completed exchanges even when the user starts afresh. The chat shows no line
                // for it: the user just confirmed it.
                let _ = self.dreamer.send(memory::DreamEvent::Flush);
            }
            Cmd::SwitchBack => self.switch_back().await,
            Cmd::CallEnded { call } => {
                if self.turn_call.as_ref() == Some(&call) {
                    self.turn_call = None;
                }
                let _ = self.dreamer.send(memory::DreamEvent::Flush);
            }
            Cmd::Reconfigure(base) => {
                let old = std::mem::replace(&mut self.base, *base);
                // On its own agent the bot takes the new one; on a fallback it keeps that agent (its step may have
                // moved) until `settle_chain` moves it off one it no longer uses.
                let cfg = if crate::agent::fallback::step_of(&old, &self.cfg.backend) == Some(0) {
                    crate::agent::fallback::effective(&self.base, 0)
                } else {
                    super::fallback::running(&self.base, &self.cfg)
                };
                // The harness reads its own memory's switch when it starts: an agent that can resume its session
                // restarts into it, one that can't starts afresh.
                let memory_changed = own_memory::kept(&cfg) != own_memory::kept(&self.cfg);
                let resumes = self.conn.as_ref().is_none_or(|c| c.sessions.load);
                let restart = cfg.backend != self.cfg.backend
                    || cfg.command != self.cfg.command
                    || cfg.cwd != self.cfg.cwd
                    || cfg.model != self.cfg.model
                    || cfg.model_options != self.cfg.model_options
                    || (memory_changed && !resumes);
                self.tools_changed |= cfg.connectors != self.cfg.connectors
                    || cfg.computer != self.cfg.computer
                    || cfg.manage_team != self.cfg.manage_team
                    || memory_changed;
                // Live sessions switch before their next turn (see `fast_mode`).
                let fast_changed = cfg.fast_mode != self.cfg.fast_mode;
                // Name, description and skill changes reach the agent as a profile update
                // on the next message (see `context`).
                self.cfg = cfg;
                if fast_changed {
                    self.fast_mode_changed();
                }
                if memory_changed {
                    // Claude Code's notes come or go on the memory screen.
                    memory::screen_changed(&self.hub, &self.cfg.id);
                }
                // The running turn finishes first: it may be the bot changing its own model (`update_bot`).
                if restart && self.turn.is_some() {
                    self.restart_after_turn = true;
                } else if restart {
                    self.restart_agent().await;
                }
                self.settle_chain(&old).await;
            }
            Cmd::Shutdown => return false,
        }
        true
    }

    /// Queues `item`, and starts it right away when the bot is free.
    async fn enqueue(&mut self, item: Queued, done_tx: &mpsc::UnboundedSender<Done>) {
        self.queue.push_back(item);
        if self.turn.is_none() {
            self.next_in_queue(done_tx).await;
        }
    }

    pub(super) async fn stop(&mut self) {
        self.hub.team.end_turn(&self.cfg.id);
        if let Err(error) = self.hub.routines.cancel_queued(&self.hub.store, &self.cfg.id) {
            tracing::error!(error = format!("{error:#}"), "couldn't cancel queued routines");
        }
        if self.active_routine.is_none() {
            self.hub.routines.release(&self.cfg.id);
        }
        // Queued messages are dropped too: Stop means "stop everything". A room's late turn stays: the room was
        // told this member answers there once free, and only the group's Stop drops it.
        for queued in std::mem::take(&mut self.queue) {
            match queued {
                Queued::Group(turn) if turn.late => self.queue.push_back(Queued::Group(turn)),
                Queued::User { entry_id, .. } => {
                    if let Some(mut e) = self.hub.store.entry(&entry_id) {
                        e.data["status"] = "cancelled".into();
                        self.hub.set_entry(&entry_id, &e.data);
                    }
                }
                _ => {}
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
        self.cancel_deadline.get_or_insert_with(|| Instant::now() + CANCEL_GRACE);
        self.files.cancel();
        let ids: Vec<String> = self.perms.keys().cloned().collect();
        for id in ids {
            self.answer_permission(&id, None).await;
        }
        if let (Some(c), Some(sid)) = (&self.conn, &self.turn_session) {
            let _ = c.acp.notify("session/cancel", json!({"sessionId": sid})).await;
        }
    }

    /// A new agent, model, command, folder or memory: the agent starts again with a fresh context.
    async fn restart_agent(&mut self) {
        self.files.cancel();
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
        for slot in self.run_slots() {
            self.set_session(Some(&slot), None);
        }
        self.forget_session();
    }

    pub(super) async fn next_in_queue(&mut self, done_tx: &mpsc::UnboundedSender<Done>) {
        if self.turn.is_none() && std::mem::take(&mut self.restart_after_turn) {
            self.restart_agent().await;
        }
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
                            if let Err(error) = result {
                                self.start_error(error, done_tx).await;
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
                    let Some(Queued::BotRequest(mut request)) = self.queue.pop_front() else {
                        unreachable!("front is a bot request")
                    };
                    let ids = [request.entry_id.clone()];
                    let prompt = request.prompt.clone();
                    request.mark_started();
                    self.active_request = Some(request);
                    let result = self.start_turn(main.clone(), &ids, &prompt, false, done_tx).await;
                    if let Err(e) = result {
                        self.start_error(e, done_tx).await;
                    }
                    continue;
                }
                Some(Queued::BotReply(_)) => {
                    // Replies that came in while the bot worked are one turn, as the user's messages are.
                    let mut replies = Vec::new();
                    while let Some(Queued::BotReply(_)) = self.queue.front() {
                        if let Some(Queued::BotReply(reply)) = self.queue.pop_front() {
                            replies.push(reply);
                        }
                    }
                    self.reply_hops = replies.iter().map(crate::chat::team::BotReply::hops).max();
                    let prompt = crate::chat::team::reply_prompt(&replies);
                    if let Err(e) = self.start_turn(main.clone(), &[], &prompt, false, done_tx).await {
                        self.start_error(e, done_tx).await;
                    }
                    continue;
                }
                Some(Queued::Prompt { .. }) => {
                    let Some(Queued::Prompt { text, call }) = self.queue.pop_front() else {
                        unreachable!("front is a prompt")
                    };
                    self.active_prompt = true;
                    self.prompt_call = call;
                    if let Err(e) = self.start_turn(main.clone(), &[], &text, false, done_tx).await {
                        self.start_error(e, done_tx).await;
                    }
                    continue;
                }
                Some(Queued::Group(_)) => {
                    let Some(Queued::Group(mut turn)) = self.queue.pop_front() else {
                        unreachable!("front is a group turn")
                    };
                    // A newer message in the room (or Stop) ended the room turn that queued this; a late turn
                    // answers anyway (see `GroupTurn::late`).
                    if turn.reply.is_closed() || !(turn.late || self.hub.groups.current(&turn.lane, turn.epoch)) {
                        continue;
                    }
                    if turn.late
                        && let Err(error) = crate::chat::group::refresh(&self.hub, &mut turn, &self.cfg)
                    {
                        tracing::warn!(bot = self.cfg.id, error = format!("{error:#}"), "late room turn dropped");
                        continue;
                    }
                    let (lane, prompt) = (turn.lane.clone(), turn.prompt.clone());
                    self.active_group = Some(turn);
                    let result = self.start_turn(lane, &[], &prompt, false, done_tx).await;
                    if let Err(e) = result {
                        self.start_error(e, done_tx).await;
                    }
                    continue;
                }
                Some(Queued::Compact { .. }) => {
                    let Some(Queued::Compact { idle }) = self.queue.pop_front() else {
                        unreachable!("front is a compaction")
                    };
                    self.start_compaction(idle, done_tx).await;
                    continue;
                }
                _ => {}
            }
            // Messages sent while the agent worked go out together as its next turn
            // without crossing a bot request or reply or another lane: each gets its own reply.
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
            // A handoff waiting in the chat starts a new conversation with this message, as does one already taken
            // for a turn that couldn't start.
            if lane == main {
                self.handoff = self.take_handoff().or_else(|| self.handoff.take());
            }
            if let Err(e) = self.start_turn(lane, &ids, &text, false, done_tx).await {
                self.start_error(e, done_tx).await;
            }
        }
    }

    pub(super) fn start_failed(&mut self, e: &anyhow::Error) {
        if self.turn.is_some() {
            self.report_turn(false, Some(e), "start_failed");
        }
        self.finish_routine(crate::routines::Status::Failed, Some(format!("Could not start routine: {e:#}")), None);
        self.complete_request(Err(anyhow!("couldn't start recipient: {e:#}")), false);
        self.complete_group(Err(anyhow!("couldn't start: {e:#}")));
        if !self.invite_sign_in(e) {
            // `{:#}` keeps the reason ("starting `npx …`: EACCES"), not just "agent process exited".
            self.failure_notice(e, &format!("Couldn't start the agent: {e:#}"), "start_failed");
        }
        // What the chain carried in (a recap, a takeover) was this turn's; the user's handoff waits for the next.
        self.relay.carried = None;
        self.turn = None;
        self.reply_hops = None;
        self.active_prompt = false;
        memory::turns::drop_turn(&self.cfg.id);
        if self.routine_completion.is_none() {
            self.set_inflight(false);
        }
        self.hub.set_runtime(&self.id(), |r| {
            r.status = BotStatus::Error;
            r.activity = "Couldn't start".into();
            r.started_at = None;
            r.lane = None;
            r.message = None;
            r.session = None;
        });
        self.hub.team.end_turn(&self.cfg.id);
    }

    /// When `e` asks for signing in: marks the harness signed out and puts a sign-in card in the
    /// chat, which sends this turn's messages again once the user has signed in. Whether it did.
    /// A process older than the turn may hold a sign-in replaced since: a new one tries its messages first.
    pub(super) fn invite_sign_in(&mut self, e: &anyhow::Error) -> bool {
        let backend = self.cfg.backend.clone();
        if crate::agent::backends::harness(&backend).is_none() || !crate::agent::acp::needs_sign_in(e) {
            return false;
        }
        // The process keeps the credentials it started with (Claude's revoked token): the next turn starts a new one.
        self.restart_after_turn = true;
        let held = self.held_for_sign_in().to_vec();
        if self.process_predates_turn() && !held.is_empty() {
            match agent_sign_in::send_again(&self.hub, &self.cfg.id, held.iter().map(String::as_str)) {
                Ok(()) => return true,
                Err(error) => tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't retry"),
            }
        }
        crate::agent::backends::set_signed_in(&backend, Some(false));
        // An account check can still say signed in (`claude auth status` with a revoked token): this refusal wins.
        crate::agent::backends::note_prompt_refused(&backend);
        self.sign_in_card(&backend, &held)
    }

    /// The agent's process was already running when this turn started.
    fn process_predates_turn(&self) -> bool {
        let (Some(conn), Some(started_at)) = (&self.conn, self.hub.runtime(&self.id()).started_at) else {
            return false;
        };
        let turn_age = Duration::from_millis(u64::try_from(now_ms() - started_at).unwrap_or(0));
        conn.started.elapsed() > turn_age
    }

    /// Agent `backend`'s sign-in card in the chat, holding `entries`. Whether it's there.
    pub(super) fn sign_in_card(&self, backend: &str, entries: &[String]) -> bool {
        let Some(h) = crate::agent::backends::harness(backend) else { return false };
        let invited = agent_sign_in::invite(&self.hub, &self.cfg.id, &self.lane, backend, h.name, entries);
        invited
            .inspect_err(
                |error| tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't ask to sign in"),
            )
            .is_ok()
    }

    pub(super) fn complete_request(&mut self, result: Result<String>, cancelled: bool) {
        if let Some(request) = self.active_request.take() {
            request.complete(result, cancelled);
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
            // The deadline also covers a session setup that ran out the clock before any turn began.
            let timed_out = self.routine_timed_out || self.routine_deadline.is_some_and(|d| Instant::now() >= d);
            self.routine_completion = Some(RoutineCompletion { id, status, detail, text, timed_out });
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
            completion.timed_out,
        ) {
            Ok(()) => {
                // A finished run's session is never resumed.
                let slot = crate::routines::session_slot(&completion.id);
                self.routine_completion = None;
                self.set_session(Some(&slot), None);
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
        let slot = self.slot();
        if self.session_for(slot.as_deref()).is_none() {
            self.finish_routine(
                crate::routines::Status::Interrupted,
                Some("No saved agent session is available".into()),
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
            self.start_failed(&e);
        }
    }

    /// Marks a turn in the bot's own chat as running across host restarts (see [`Self::resume_interrupted`]).
    pub(super) fn set_inflight(&self, on: bool) {
        let value = if on { now_ms().to_string() } else { String::new() };
        if let Err(error) = self.hub.store.kv_set(&inflight_key(&self.cfg.id), &value) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the running turn");
        }
    }
}
