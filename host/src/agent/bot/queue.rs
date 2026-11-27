//! Commands from clients and the queue of turns waiting for the bot.

use super::sign_in::Held;
use super::{
    Actor, CANCEL_GRACE, Cmd, Done, HostTurn, Inflight, NoticeStyle, PAUSED_NOTICE, Queued, RESUME_PROMPT,
    RoutineCompletion,
};
use crate::agent::own_memory;
use crate::chat::memory;
use crate::chat::work::{self, Failure, Outcome};
use crate::hub::BotStatus;
use crate::store::{EntryKind, Lane, now_ms};
use anyhow::{Result, anyhow};
use codync_network::Access;
use serde_json::json;
use std::time::Instant;
use tokio::sync::mpsc;

impl Actor {
    pub(super) async fn on_cmd(&mut self, cmd: Cmd, done_tx: &mpsc::UnboundedSender<Done>) -> bool {
        match cmd {
            Cmd::RefreshTools => self.restart_stale(),
            Cmd::KeeperRestarted => self.keeper_restarted().await,
            Cmd::Routine(id) => self.enqueue(Queued::Routine(id), done_tx).await,
            Cmd::FollowUp(id) => self.enqueue(Queued::FollowUp(id), done_tx).await,
            Cmd::TaskReport(report) => self.enqueue(Queued::TaskReport(report), done_tx).await,
            Cmd::WorkflowStep { step_id } => self.enqueue(Queued::WorkflowStep(step_id), done_tx).await,
            Cmd::WorkflowStop { step_id } => self.stop_workflow(&step_id).await,
            Cmd::Watchdog(stuck) => {
                // A newer look at the thread stands for one still waiting.
                self.queue.retain(|q| !matches!(q, Queued::Watchdog(s) if s.thread() == stuck.thread()));
                self.enqueue(Queued::Watchdog(stuck), done_tx).await;
            }
            Cmd::TaskAnswer { prompt, hops } => self.enqueue(Queued::TaskAnswer { prompt, hops }, done_tx).await,
            Cmd::Send { lane, entry_id, mut text } => {
                // What the user writes to a thread waits behind the turn its card holds: only a press
                // answers a thread's card, and a message neither answers nor ends it.
                if self.cfg.is_thread() || !self.try_decision_message(&lane, &entry_id, &mut text) {
                    self.enqueue(Queued::User { lane, entry_id, text }, done_tx).await;
                }
            }
            Cmd::Steer { entry_id } => self.steer(entry_id).await,
            Cmd::Steered { entry_id, outcome } => self.steer_answered(&entry_id, &outcome, done_tx).await,
            Cmd::Unqueue { entry_id } => self.unqueue(&entry_id),
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
            // A task in its box, so the chat stays free; a bot that needs its chat's tools for it takes it here.
            Cmd::BotRequest(request) if work::boxed(&self.base) => {
                work::add_request(&self.hub, &self.cfg.id, request);
            }
            Cmd::BotRequest(request) => self.enqueue(Queued::BotRequest(request), done_tx).await,
            Cmd::BotReply(reply) => self.enqueue(Queued::BotReply(reply), done_tx).await,
            Cmd::Prompt { text, call } => self.enqueue(Queued::Prompt { text, call }, done_tx).await,
            Cmd::SendToUser { text, quote, reply } => {
                let _ = reply.send(self.send_to_user(text, quote));
            }
            Cmd::Ask { data, started_at, deadline, reply } => self.ask(data, started_at, deadline, reply),
            Cmd::Decision { entry_id, choice, reply } => {
                if !reply.is_closed() {
                    // A call that just ran out leaves its card open: a press on it is the user's next message.
                    self.expire_abandoned_decisions();
                    let answer = if self.open_card.as_deref() == Some(&entry_id) {
                        self.answer_open_card(&entry_id, &choice)
                    } else {
                        self.decided(&entry_id, &choice, None)
                    };
                    let _ = reply.send(answer);
                }
            }
            Cmd::React { emoji, to, reply } => {
                let _ = reply.send(self.react(&emoji, to.as_deref()));
            }
            Cmd::SendFile { path, name, text, quote, cancelled, reply } => {
                self.begin_file(path, name, text, quote, cancelled, reply);
            }
            Cmd::PostArtifact { data, reply } => {
                let _ = reply.send(self.post_artifact(data));
            }
            Cmd::Stop => self.stop().await,
            Cmd::Paused(paused) => self.pause_changed(paused, done_tx).await,
            Cmd::Permission { entry_id, option_id } => self.answer_permission(&entry_id, option_id).await,
            Cmd::Approve { title, detail, reply } => self.ask_approval(&title, &detail, reply),
            Cmd::NewSession => {
                // Only the running turn stops: queued messages and follow-ups run in the new session; queued
                // routines still run, each in its own.
                self.cancel_turn().await;
                self.forget_session();
                self.forget_parked();
                self.drop_handoff();
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
            Cmd::Reconfigure(mut base) => {
                // A thread's folder is its own (`enter_worktree`): its bot's changes (`work::fallbacks_changed`) keep it.
                if self.cfg.is_thread() {
                    base.cwd.clone_from(&self.base.cwd);
                }
                let old = std::mem::replace(&mut self.base, *base);
                self.versions.ready(&self.hub, &self.base);
                // On its own agent the bot takes the new one; on a fallback it keeps that agent (its step may have
                // moved) until `settle_chain` moves it off one it no longer uses.
                let cfg = if crate::agent::fallback::step_of(&old, &crate::agent::fallback::login::of(&self.cfg))
                    == Some(0)
                {
                    crate::agent::fallback::effective(&self.base, 0)
                } else {
                    super::fallback::running(&self.base, &self.cfg)
                };
                // The harness reads its own memory's switch when it starts: an agent that can resume its session
                // restarts into it, one that can't starts afresh.
                let memory_changed = own_memory::kept(&cfg) != own_memory::kept(&self.cfg);
                let resumes = self.conn.as_ref().is_none_or(|c| c.sessions.reopen().is_some());
                let restart = cfg.backend != self.cfg.backend
                    || cfg.command != self.cfg.command
                    || cfg.login_profile != self.cfg.login_profile
                    || cfg.cwd != self.cfg.cwd
                    || cfg.model != self.cfg.model
                    || cfg.model_options != self.cfg.model_options
                    || (memory_changed && !resumes);
                self.restart_before_turn |= cfg.connectors != self.cfg.connectors
                    || cfg.computer != self.cfg.computer
                    || cfg.manage_team != self.cfg.manage_team
                    || memory_changed;
                // Live sessions switch before their next turn (see `fast_mode`).
                let fast_changed = cfg.fast_mode != self.cfg.fast_mode;
                let mode_changed = cfg.mode != self.cfg.mode;
                let threshold_changed = cfg.compaction != self.cfg.compaction;
                // Name, description and skill changes reach the agent as a profile update
                // on the next message (see `context`).
                self.switched_by_user(&cfg);
                if cfg.cwd != self.cfg.cwd && !self.cfg.is_thread() {
                    self.moved_folder();
                }
                self.cfg = cfg;
                if threshold_changed {
                    self.threshold_changed();
                }
                if fast_changed {
                    self.fast_mode_changed();
                }
                if mode_changed {
                    self.mode_changed();
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
                // A pause stops the running turn (nothing more is spent) and closes what waits on it; resuming
                // lets what stayed queued go on and looks over the box again. The host's pause transaction
                // cancelled the bot's routine runs (`Hub::update_bot`), so a notification that arrives late
                // cannot cancel a run admitted after a resume.
                if old.paused != self.base.paused {
                    self.pause_changed(self.base.paused, done_tx).await;
                }
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
        if let Err(error) = self.hub.routines.cancel_for_stop(&self.hub.store, &self.cfg.id) {
            tracing::error!(error = format!("{error:#}"), "couldn't cancel queued routines");
        }
        if self.active_routine.is_none() {
            self.hub.routines.release(&self.cfg.id);
        }
        if let Err(error) = self.hub.routines.drop_queued_follow_ups(&self.hub, &self.cfg.id) {
            tracing::error!(error = format!("{error:#}"), "couldn't drop queued follow-ups");
        }
        // Queued messages are dropped too: Stop means "stop everything". A room's late turn stays: the room was
        // told this member answers there once free, and only the group's Stop drops it.
        for queued in std::mem::take(&mut self.queue) {
            match queued {
                Queued::WorkflowStep(step) => {
                    self.hub.workflows.finish(&self.hub, &step, None, None, true);
                }
                Queued::Group(turn) if turn.late => self.queue.push_back(Queued::Group(turn)),
                // A task's report is news the user still wants.
                Queued::TaskReport(report) => self.queue.push_back(Queued::TaskReport(report)),
                // Its notice keeps the outcome; a host restart doesn't bring back what the user stopped.
                Queued::BotReply(reply) => crate::chat::team::relayed(&self.hub, std::slice::from_ref(&reply), &[]),
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
        // A thread whose task waited for a sign-in has no turn to cancel: it no longer needs the user, and its box is
        // looked over again (its task stopped or gave way, so the thread ends).
        if self.turn.is_none() && self.cfg.is_thread() && self.hub.runtime(&self.id()).status == BotStatus::NeedsInput {
            self.hub.set_runtime(&self.id(), |r| {
                r.status = BotStatus::Idle;
                r.activity.clear();
            });
            self.turn_over(Outcome::Stopped);
        }
        // Nor does one whose task waits on what its agent runs in the background: that stops with its agent.
        if self.turn.is_none() && self.cfg.is_thread() && self.runs_in_background() {
            self.hub.set_runtime(&self.id(), |r| r.background.clear());
            self.kill_agent().await;
            self.turn_over(Outcome::Stopped);
        }
    }

    async fn stop_workflow(&mut self, step_id: &str) {
        self.queue.retain(|queued| !matches!(queued, Queued::WorkflowStep(id) if id == step_id));
        if self.active_workflow.as_deref() == Some(step_id) {
            self.cancel_turn().await;
        } else {
            self.hub.workflows.finish(&self.hub, step_id, None, None, true);
        }
    }

    /// Its bot's pause changed: a pause stops the running turn (nothing more is spent) and closes what waits on
    /// it; a resume lets what stayed queued go on and, for the bot's own chat, looks the box over again. The host's
    /// pause transaction cancelled the bot's routine runs, so this never does: a late notification would otherwise
    /// cancel a run admitted after a resume.
    pub(super) async fn pause_changed(&mut self, paused: bool, done_tx: &mpsc::UnboundedSender<Done>) {
        if paused {
            self.cancel_turn().await;
            self.refuse_paused();
        } else {
            self.next_in_queue(done_tx).await;
            if !self.cfg.is_thread() {
                work::dispatch(&self.hub, &self.cfg.id);
            }
        }
    }

    /// Stops the running turn only (queued ones stay).
    pub(super) async fn cancel_turn(&mut self) {
        if self.turn.is_none() {
            return;
        }
        self.stop_requested = true;
        self.expire_decisions(crate::chat::decision::Expiry::Stopped);
        self.cancel_deadline.get_or_insert_with(|| Instant::now() + CANCEL_GRACE);
        self.files.cancel();
        let ids: Vec<String> = self.perms.keys().cloned().collect();
        for id in ids {
            self.answer_permission(&id, None).await;
        }
        if let (Some(c), Some(sid)) = (&self.conn, &self.turn_session) {
            let _ = c.acp.session(sid).cancel().await;
        }
    }

    /// A new agent, model, command, folder or memory: the agent starts again with a fresh context.
    pub(super) async fn restart_agent(&mut self) {
        self.files.cancel();
        self.kill_agent().await;
        for slot in self.run_slots() {
            self.set_session(Some(&slot), None);
        }
        self.forget_session();
    }

    pub(super) async fn next_in_queue(&mut self, done_tx: &mpsc::UnboundedSender<Done>) {
        if self.turn.is_none() && std::mem::take(&mut self.restart_after_turn) {
            self.restart_agent().await;
        }
        // Paused: no turn of its starts, so nothing is spent. What waits on it hears so now (see `refuse_paused`).
        if self.paused() {
            self.refuse_paused();
            return;
        }
        let main = Lane::main(&self.cfg.id);
        while self.turn.is_none() && self.routine_completion.is_none() && !self.queue.is_empty() {
            // What the rest of the queue works on is the user's, or this computer's own: only a bot request and
            // bot replies carry another computer's text.
            self.hub.ceilings.set(&self.cfg.id, Access::Allow);
            match self.queue.front() {
                Some(Queued::WorkflowStep(_)) => {
                    let Some(Queued::WorkflowStep(id)) = self.queue.pop_front() else {
                        unreachable!("front is a workflow step")
                    };
                    self.active_workflow = Some(id.clone());
                    match crate::workflows::agent_begin(&self.hub, &id) {
                        Ok(Some(prepared)) => {
                            self.routine_deadline = Some(Instant::now() + prepared.timeout);
                            let result = self.start_turn(prepared.lane, &[], &prepared.prompt, false, done_tx).await;
                            if let Err(error) = result {
                                self.routine_deadline = None;
                                self.hub.workflows.finish(&self.hub, &id, None, Some(format!("{error:#}")), false);
                                self.active_workflow = None;
                            } else if let Err(error) = crate::workflows::agent_mark_started(&self.hub, &id) {
                                tracing::warn!(error = format!("{error:#}"), "couldn't mark a workflow step started");
                            }
                        }
                        Ok(None) => self.active_workflow = None,
                        Err(error) => {
                            self.hub.workflows.finish(&self.hub, &id, None, Some(format!("{error:#}")), false);
                            self.active_workflow = None;
                        }
                    }
                    continue;
                }
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
                            let why = crate::routines::could_not_start(format_args!("{error:#}"));
                            self.finish_routine(crate::routines::Status::Failed, Some(why), None);
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
                    self.hub.ceilings.set(&self.cfg.id, request.access());
                    request.mark_started();
                    self.active_request = Some(request);
                    let result = self.start_turn(main.clone(), &ids, &prompt, false, done_tx).await;
                    if let Err(e) = result {
                        self.start_error(e, done_tx).await;
                    }
                    continue;
                }
                Some(Queued::BotReply(_)) => {
                    // Replies that came in while the bot worked are one turn, as the user's messages are, those a
                    // network rule holds to the same level: a turn that can't start under a ceiling takes no other
                    // bot's reply down with it.
                    let mut replies: Vec<crate::chat::team::BotReply> = Vec::new();
                    while let Some(Queued::BotReply(next)) = self.queue.front()
                        && replies.first().is_none_or(|first| first.access() == next.access())
                    {
                        if let Some(Queued::BotReply(reply)) = self.queue.pop_front() {
                            replies.push(reply);
                        }
                    }
                    let access = replies.first().map_or(Access::Allow, crate::chat::team::BotReply::access);
                    self.hub.ceilings.set(&self.cfg.id, access);
                    self.reply_hops = replies.iter().map(crate::chat::team::BotReply::hops).max();
                    let prompt = crate::chat::team::reply_prompt(&replies, access);
                    let result = self.start_turn(main.clone(), &[], &prompt, false, done_tx).await;
                    let started = result.is_ok();
                    if let Err(e) = result {
                        self.start_error(e, done_tx).await;
                    }
                    // Only once the agent has the prompt (or the turn failed where the user sees it): a host restart
                    // before that relays them again from their notices. From then on a restart resumes the turn: the
                    // one write that takes them also marks it running.
                    let running = (started && self.resumes()).then(|| self.inflight_row(true));
                    crate::chat::team::relayed(&self.hub, &replies, running.as_slice());
                    continue;
                }
                Some(Queued::TaskReport(_)) => {
                    // Reports that came in while the bot worked are one turn, as replies are.
                    let mut reports = Vec::new();
                    while let Some(Queued::TaskReport(_)) = self.queue.front() {
                        if let Some(Queued::TaskReport(report)) = self.queue.pop_front() {
                            reports.push(report);
                        }
                    }
                    self.host_turn = Some(HostTurn::TaskReport);
                    let hops = reports.iter().map(|r| r.hops).max().unwrap_or(0);
                    let prompt = work::report_prompt(&reports);
                    self.reports = reports;
                    match self.start_turn(main.clone(), &[], &prompt, false, done_tx).await {
                        // A task this turn adds carries the chain on.
                        Ok(()) => self.hub.work.answering(&self.cfg.id, hops),
                        Err(e) => self.start_error(e, done_tx).await,
                    }
                    continue;
                }
                Some(Queued::Watchdog(_)) => {
                    let Some(Queued::Watchdog(stuck)) = self.queue.pop_front() else {
                        unreachable!("front is the watchdog's")
                    };
                    // The task ended or was set aside while this waited: nothing is left to look at.
                    let Some((prompt, hops)) = stuck.turn(&self.hub.store, &self.cfg.id) else { continue };
                    self.host_turn = Some(HostTurn::Watchdog);
                    match self.start_turn(main.clone(), &[], &prompt, false, done_tx).await {
                        // A task this turn adds carries the chain on, as a report's turn does.
                        Ok(()) => self.hub.work.answering(&self.cfg.id, hops),
                        Err(e) => self.start_error(e, done_tx).await,
                    }
                    continue;
                }
                Some(Queued::TaskAnswer { .. }) => {
                    let Some(Queued::TaskAnswer { prompt, hops }) = self.queue.pop_front() else {
                        unreachable!("front is a thread's answer")
                    };
                    self.host_turn = Some(HostTurn::TaskAnswer);
                    match self.start_turn(main.clone(), &[], &prompt, false, done_tx).await {
                        // A task this turn adds carries the chain on, as a report's turn does.
                        Ok(()) => self.hub.work.answering(&self.cfg.id, hops),
                        Err(e) => self.start_error(e, done_tx).await,
                    }
                    continue;
                }
                Some(Queued::Prompt { .. }) => {
                    let Some(Queued::Prompt { text, call }) = self.queue.pop_front() else {
                        unreachable!("front is a prompt")
                    };
                    self.host_turn = Some(HostTurn::Opening);
                    self.prompt_call = call;
                    if let Err(e) = self.start_turn(main.clone(), &[], &text, false, done_tx).await {
                        self.start_error(e, done_tx).await;
                    }
                    continue;
                }
                Some(Queued::FollowUp(_)) => {
                    let Some(Queued::FollowUp(id)) = self.queue.pop_front() else {
                        unreachable!("front is a follow-up")
                    };
                    match self.hub.routines.begin_follow_up(&self.hub, &self.cfg.id, &id) {
                        Ok(Some(prompt)) => {
                            self.host_turn = Some(HostTurn::FollowUp);
                            if let Err(e) = self.start_turn(main.clone(), &[], &prompt, false, done_tx).await {
                                self.start_error(e, done_tx).await;
                            }
                        }
                        // Cancelled while it waited for its turn.
                        Ok(None) => {}
                        Err(error) => {
                            tracing::error!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't begin a follow-up");
                        }
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
                    if let Err(error) = crate::chat::group::refresh(&self.hub, &mut turn, &self.cfg.id) {
                        tracing::warn!(bot = self.cfg.id, error = format!("{error:#}"), "room turn dropped");
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

    /// Paused: what waits on this bot hears so now, and nothing of theirs is spent. The user's messages go out
    /// cancelled with a notice (they don't wait to run on a later resume), a bot's request ends as not started,
    /// replies go to their notices, and a room's turn is dropped with why. Host turns (a task's report, the
    /// watchdog, a due follow-up) and compactions stay queued: they run when it is resumed. A due routine run is
    /// cancelled instead (see below): its schedule continues from the next trigger after the resume.
    fn refuse_paused(&mut self) {
        let (kept, refused): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queue).into_iter().partition(|q| {
            !matches!(q, Queued::User { .. } | Queued::BotRequest(_) | Queued::BotReply(_) | Queued::Group(_))
        });
        // A routine run that came due while it is paused is cancelled too (a pause stops its queued ones):
        // its schedule continues from the next trigger after the resume. Only the runs still in *this* queue: a
        // run admitted after the resume is not one of them, so a pause notification that arrives late cannot
        // cancel work the resume already started.
        let routine_ids: Vec<String> = kept
            .iter()
            .filter_map(|q| match q {
                Queued::Routine(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        if !routine_ids.is_empty()
            && let Err(error) = self.hub.routines.cancel_queued(&self.hub.store, &routine_ids)
        {
            tracing::error!(error = format!("{error:#}"), "couldn't cancel a paused bot's queued routines");
        }
        self.queue = kept.into_iter().collect();
        let mut told = false;
        for item in refused {
            match item {
                Queued::User { entry_id, .. } => {
                    if let Some(mut e) = self.hub.store.entry(&entry_id) {
                        e.data["status"] = "cancelled".into();
                        self.hub.set_entry(&entry_id, &e.data);
                    }
                    told = true;
                }
                Queued::BotRequest(request) => {
                    request.complete(Err(crate::chat::team::no_reply::not_started("this bot is paused")), false);
                }
                Queued::BotReply(reply) => crate::chat::team::relayed(&self.hub, std::slice::from_ref(&reply), &[]),
                Queued::Group(turn) => {
                    let _ = turn.reply.send(Err(anyhow!("this bot is paused")));
                }
                _ => unreachable!("only the queued kinds above are refused"),
            }
        }
        if told {
            self.add_between_turns(EntryKind::Notice, &json!({"text": PAUSED_NOTICE, "style": "info"}));
        }
    }

    pub(super) fn start_failed(&mut self, e: &anyhow::Error) {
        if self.turn.is_some() {
            self.report_turn(false, Some(e), "start_failed");
        }
        let why = crate::routines::could_not_start(format_args!("{e:#}"));
        self.finish_routine(crate::routines::Status::Failed, Some(why), None);
        self.complete_request(Err(crate::chat::team::no_reply::not_started(format_args!("{e:#}"))), false);
        self.complete_group(Err(anyhow!("couldn't start: {e:#}")));
        if !self.invite_sign_in(e) {
            // `{:#}` keeps the reason ("starting `npx …`: EACCES"), not just "agent process exited".
            self.failure_notice(e, &format!("Couldn't start the agent: {e:#}"), "start_failed");
        }
        let goes_on = self.end_task(|| Outcome::not_started(e));
        // What the chain carried in (a recap, a takeover) was this turn's; the user's handoff waits for the next.
        self.relay.carried = None;
        self.turn = None;
        self.reply_hops = None;
        self.end_host_turn(false);
        memory::turns::drop_turn(&self.cfg.id);
        if self.routine_completion.is_none() {
            self.set_inflight(false);
        }
        let needs = goes_on.as_ref().and_then(Held::needs);
        self.hub.set_runtime(&self.id(), |r| {
            r.status = if needs.is_some() { BotStatus::NeedsInput } else { BotStatus::Error };
            r.activity = needs.unwrap_or_else(|| "Couldn't start".into());
            r.started_at = None;
            r.lane = None;
            r.message = None;
            r.session = None;
        });
        self.wait_for_sign_in(goes_on.as_ref());
        self.hub.team.end_turn(&self.cfg.id);
    }

    /// The host-started turn is over; what it was, for the turn's end to act on. `answered`: the agent ended it
    /// with a reply of its own.
    pub(super) fn end_host_turn(&mut self, answered: bool) -> Option<HostTurn> {
        let ended = self.host_turn.take();
        match ended {
            Some(HostTurn::FollowUp) => self.hub.routines.end_follow_up(&self.cfg.id),
            Some(HostTurn::TaskReport) => self.reports_over(answered),
            Some(HostTurn::Opening | HostTurn::Watchdog | HostTurn::TaskAnswer) | None => {}
        }
        ended
    }

    /// The turn for the tasks' reports is over. Unanswered (stopped, failed, never started), they show in the chat
    /// as they came, so what a thread found isn't lost with the turn.
    fn reports_over(&mut self, answered: bool) {
        let reports = std::mem::take(&mut self.reports);
        if !answered {
            for report in &reports {
                self.notice(&work::report_notice(report), NoticeStyle::Info);
            }
        }
        let tasks: Vec<String> = reports.into_iter().map(|report| report.task).collect();
        work::reported(&self.hub, &self.cfg.id, &tasks);
    }

    /// The turn is over, however it ended (or never began): a thread's task says how and its bot gets the report;
    /// the tasks a bot added in the turn may start. Whether the thread's task goes on with what the turn left running
    /// instead, which then stays as it is ([`work::finished`]).
    pub(super) fn turn_over(&mut self, outcome: Outcome) -> bool {
        if self.cfg.is_thread() {
            let goes_on = work::finished(&self.hub, &self.cfg, outcome);
            if !goes_on {
                // Its task's next round begins from what runs then.
                self.detached_before = None;
            }
            goes_on
        } else {
            work::turn_over(&self.hub, &self.cfg.id, matches!(outcome, Outcome::Stopped));
            false
        }
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
        let ran = crate::routines::Ran { timed_out: completion.timed_out, thread: None };
        match self.hub.routines.complete(
            &self.hub,
            &completion.id,
            completion.status,
            completion.detail.clone(),
            completion.text.clone(),
            &ran,
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
        let follow_up = Inflight::load(&self.hub.store, &self.cfg.id)
            .and_then(|turn| turn.follow_up)
            .filter(|_| self.active_routine.is_none());
        self.set_inflight(false);
        let slot = self.slot();
        if self.session_for(slot.as_deref()).is_none() {
            self.finish_routine(
                crate::routines::Status::Interrupted,
                Some("No saved agent session is available".into()),
                None,
            );
            self.turn_over(Outcome::Failed(Failure::Restart, work::CUT_OFF.into()));
            return;
        }
        let prompt = if self.active_routine.is_some() {
            format!(
                "{RESUME_PROMPT} This is still a scheduled routine run. If its instruction permits silence and there is nothing to report, return exactly (pass)."
            )
        } else if let Some(chain) = follow_up {
            // Still the follow-up's turn: a `(pass)` stays silent and alerts no one, and a follow-up it sets
            // continues the chain it was in.
            self.host_turn = Some(HostTurn::FollowUp);
            self.hub.routines.resume_follow_up(&self.cfg.id, chain);
            format!(
                "{RESUME_PROMPT} This is still the follow-up you set for yourself. If there is nothing worth telling the user yet, return exactly (pass)."
            )
        } else {
            RESUME_PROMPT.to_owned()
        };
        if let Err(e) = self.start_turn(lane, &[], &prompt, true, done_tx).await {
            self.start_failed(&e);
        }
    }

    /// Whether a host restart resumes the turn starting now. A bot request or room turn isn't (the room that waited is
    /// gone): a request's persisted notices are marked interrupted instead of silently repeating work. Nor is a call's
    /// opening (the call is over). A reply turn is, and so is a follow-up, which is gone from the list by now. A turn
    /// under a network rule's ceiling isn't: the ceiling doesn't outlive the host, and the turn that resumed it would
    /// work on the sender's text as the bot is set. Nor is a workflow step: its run doesn't outlive the host.
    pub(super) fn resumes(&self) -> bool {
        self.active_request.is_none()
            && !self.hub.ceilings.holds(&self.cfg.id)
            && self.active_group.is_none()
            && self.active_routine.is_none()
            && self.host_turn.is_none_or(HostTurn::resumed)
            && self.active_workflow.is_none()
    }

    /// Marks a turn in the bot's own chat as running across host restarts (see [`Self::resume_interrupted`]); a
    /// follow-up's turn is marked as one, with its place in its chain.
    pub(super) fn set_inflight(&self, on: bool) {
        let (key, mark) = self.inflight_row(on);
        if let Err(error) = self.hub.store.kv_set(&key, &mark) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the running turn");
        }
    }

    /// The kv row [`Self::set_inflight`] saves, for a write that saves it with others.
    pub(super) fn inflight_row(&self, on: bool) -> (String, String) {
        let turn = on.then(|| {
            let follow_up = (self.host_turn == Some(HostTurn::FollowUp))
                .then(|| self.hub.routines.follow_up_chain(&self.cfg.id).unwrap_or(0));
            Inflight { started: now_ms(), follow_up }
        });
        Inflight::row(&self.cfg.id, turn)
    }
}
