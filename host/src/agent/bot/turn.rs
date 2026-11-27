//! Starting a turn and wrapping it up: final reply, notices, the reply to a requesting bot or room, alerts and memory.

use super::background_work::Left;
use super::sign_in::Held;
use super::{Actor, Conn, Done, HostTurn, Instructions, NoticeStyle, Queued, Reacted, Seg};
use crate::analytics::{self, TurnEnd, TurnOutcome, TurnSource};
use crate::chat::context::{self, Snapshot};
use crate::chat::memory;
use crate::chat::team::no_reply;
use crate::chat::work::{self, Failure, Outcome, Source};
use crate::hub::BotStatus;
use crate::remote::push::{self, AlertKind};
use crate::store::{BotConfig, EntryKind, Lane, now_ms};
use anyhow::{Result, anyhow};
use codync_agent::acp::{self, Incoming, StopReason};
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
        // An agent on its own (`BotConfig::plain`) hears the user's words alone.
        if self.cfg.plain {
            self.session_fresh = false;
            return Ok(text.to_owned());
        }
        let instructions = self.conn.as_ref().map_or(Instructions::FirstMessage, Conn::instructions);
        let mut snapshot = self.snapshot(sid).await?;
        // Sessions whose frozen instructions predate send_file, artifacts or follow-ups (as they are now: not for the bot's
        // own tasks) hear about them each turn.
        let remind_files = !snapshot.system.contains("send_file");
        let remind_artifacts = !snapshot.system.contains("publish_artifact");
        let remind_follow_ups = !snapshot.system.contains("check on a task you started");
        let briefed_on_threads = snapshot.system.contains("add_task");
        let mut prompt = text.to_owned();
        // The user's handoff first, then what the fallback chain carries in.
        for first in [self.relay.carried.take(), self.handoff.take()].into_iter().flatten() {
            prompt = format!("{first}\n\n{prompt}");
        }
        // A compaction may have dropped the profile the session's first message carried.
        let resend = instructions == Instructions::FirstMessage && self.profile_resend.remove(sid);
        if snapshot.unprompted || resend {
            // The instructions reaching the agent now hold memory as it was when they were rendered.
            memory::opened(&self.hub.store, self.cfg.owner(), sid, &snapshot.memory)?;
        }
        // A thread on a handed-out task holds neither memory nor who the bot is in its instructions, so no change to
        // them is news.
        let as_bot = context::as_bot(&self.cfg, &self.task_source);
        if as_bot && let Some((notice, revision)) = memory::change_notice(&self.hub.store, &self.cfg.as_owner(), sid) {
            prompt = format!("{notice}\n\n{prompt}");
            self.turn_memory_revision = revision;
        }
        // A prompt that failed may never have reached the agent: the instructions stay owed until one succeeds.
        self.session_fresh = false;
        if snapshot.unprompted {
            self.first_prompt = Some(snapshot.clone());
        }
        if snapshot.unprompted && instructions == Instructions::FirstMessage {
            prompt = format!("<bot-profile>\n{}\n</bot-profile>\n\n{prompt}", snapshot.system);
        } else if resend {
            context::mark_announced(&self.hub.store, self.cfg.owner(), &snapshot, snapshot.system_identity.clone())?;
            snapshot.announced = snapshot.system_identity.clone();
            prompt = format!("<bot-profile>\n{PROFILE_AGAIN}\n\n{}\n</bot-profile>\n\n{prompt}", snapshot.system);
        }
        if as_bot && let Some((update, identity)) = context::profile_update(&self.hub.store, &snapshot, &self.cfg) {
            prompt = format!("{prompt}\n\n{update}");
            self.announce = Some((snapshot, identity));
        }
        // A thread's instructions are its own (`context::render`): the reminders below are for a bot's own sessions
        // opened before what they remind of.
        let own_chat = self.in_own_chat() && self.active_workflow.is_none() && !self.cfg.is_thread();
        if remind_files && own_chat {
            prompt.push_str("\n\n[Codync: send_file(path, name?) shares any regular file up to 100 MiB as a downloadable card. Use it instead of sending a local path.]");
        }
        if remind_artifacts && own_chat {
            prompt.push_str("\n\n[Codync: for a report, plan, tracker or dashboard the user will come back to, write one self-contained HTML file and publish_artifact(path, title) instead of a long message. To change it, edit a copy of its HTML (list_artifacts) and publish_artifact(path, artifact: its id); notify: false skips a new card in the chat.]");
        }
        if remind_follow_ups && own_chat {
            prompt = format!("{prompt}\n\n[Codync: {}]", crate::routines::BRIEF);
        }
        let known = codync_agent::backends::harness(&self.cfg.backend).is_some();
        if let Some(about) = work::reminder(briefed_on_threads, known).filter(|_| own_chat) {
            prompt = format!("{prompt}\n\n[Codync: {about}]");
        }
        // Its box, so it can say what it is doing and place more work without asking. Where `EACH_TURN` goes: a
        // custom command gets the message as written.
        if own_chat
            && known
            && let Some(lines) = work::lines(&self.hub, &self.cfg, "Your")
        {
            prompt = format!("{prompt}\n\n[Codync: {lines}]");
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
        self.task_source = work::source_of(&self.hub.store, &self.cfg);
        if self.cfg.is_thread() {
            // Another computer's request holds the thread's turn to its ceiling, as it holds a turn in the chat.
            self.hub.ceilings.set(&self.cfg.id, work::access_of(&self.hub, &self.cfg));
            // A run worked as a thread: its session's setup ends by the routine's time limit too, where the box's
            // timer (`work::sources::watch`) only reaches a turn that began. Any other task has none.
            self.routine_deadline = match &self.task_source {
                Source::Routine { run, .. } => Some(Instant::now() + self.hub.routines.timeout(run)),
                _ => None,
            };
        }
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
                    // A resumed turn (hidden) carries on in the one that was cut off.
                    self.hub.store.max_turn(&lane.chat) + i64::from(!hidden && self.source() != TurnSource::User)
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
        // A room turn answers the user's message in the room; another turn, the messages it was given.
        let answered = self.active_group.as_ref().map_or(self.turn_entries.as_slice(), |g| g.answers.as_slice());
        let opens = self.prompt_call.take().filter(|call| crate::chat::calls::is_live(&self.hub.store, call));
        self.turn_call = crate::chat::calls::of_turn(&self.hub.store, answered).or(opens);
        self.reset_turn_state();
        let team_turn = if self.in_request() {
            crate::chat::team::Turn::Request
        } else {
            crate::chat::team::Turn::Chat(self.reply_hops.unwrap_or(0))
        };
        self.hub.team.start_turn(&self.cfg.id, team_turn);
        if hidden {
            self.hidden_turn();
        } else {
            self.relay_turn(&lane, entry_ids, text).await;
        }
        // A reply turn is marked only once its prompt is out and its replies are relayed (`queue`): a restart before
        // that sends them again from their notices, and resuming as well would hand them over twice.
        self.set_inflight(self.resumes() && self.reply_hops.is_none());
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

        // A new thread that works beside another in one repository does so in a worktree of its own.
        let text = &self.enter_worktree(text).await;
        // Memory files changed since (in an editor, say) reach the opening or the next notice.
        let (hub, cfg) = (self.hub.clone(), self.cfg.as_owner());
        let _ = tokio::task::spawn_blocking(move || memory::files::scan(&hub, &cfg)).await;
        let slot = self.slot();
        self.ensure_turn_session(slot.as_deref()).await?;
        self.note_detached().await;
        let written = self.written_to_thread(entry_ids);
        let picked_up = self.hear_thread(&written);
        // A question of its bot's among them: the thread's next message answers it.
        work::ask::heard(&self.hub, &self.cfg, entry_ids);
        let session = self.turn_session.clone();
        self.hub.set_runtime(&self.id(), |r| r.session.clone_from(&session));
        if let Some(sid) = session {
            self.sync_mode(&sid).await?;
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
            self.turn_over(Outcome::Failed(Failure::Restart, work::CUT_OFF.into()));
            self.turn = None;
            self.end_host_turn(false);
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
        // Requests that came in before this turn: a permission gets the answer it would have had a moment ago. Of the
        // chatter, what is about the agent (its commands, options) counts.
        while let Some(inc) = self.conn.as_mut().and_then(|c| c.rx.try_recv().ok()) {
            match inc {
                Incoming::Request(request) => self.answer_outside_turn(request).await,
                Incoming::Update(update) => {
                    self.on_session_wide(&update);
                }
                _ => {}
            }
        }
        let sid = self.turn_session.clone().ok_or_else(|| anyhow!("no session"))?;
        // The user's own `/command` for the agent goes as typed, so the agent runs it; what Codync would add waits
        // for the next message.
        let agent_command = from_user && self.active_group.is_none() && self.is_agent_command(text);
        if agent_command {
            // The profile stays owed: the first message the agent hears that isn't a command carries it.
            self.session_fresh = false;
            analytics::agent_command_sent(&self.hub, &self.cfg.backend);
        } else if !hidden {
            // So does a recap: a command goes without it, and it waits in `relay.recap` for the next message.
            self.carry_in();
        }
        // What the user wrote to a thread says so, after the work itself where the session doesn't hold it.
        let text = &if written.is_empty() || agent_command || self.cfg.plain {
            text.to_owned()
        } else {
            work::in_thread(text, picked_up)
        };
        let prompt = if std::mem::take(&mut self.raw_prompt) || agent_command {
            text.to_owned()
        } else {
            self.compose(&sid, text).await?
        };
        self.hub.set_runtime(&self.id(), |r| r.activity = "Thinking…".into());
        let acp = self.conn.as_ref().ok_or_else(|| anyhow!("agent not running"))?.acp.clone();
        let prompt = json!([{"type": "text", "text": prompt}]);
        if let Some(id) = &self.active_routine {
            self.hub.routines.mark_started(&self.hub.store, id)?;
            self.set_inflight(true);
        }
        // The bot's own folder is saved as it is, so what this turn changes there is a version of its own.
        self.versions.before(&self.hub, &self.cfg, text).await;
        // From here what changes in its memory folders is the bot's, however it changed them: a thread's too, as
        // its own turn's, in its own session. A thread only adds to memory, also in a turn the user's message to its
        // thread started: what the user wants forgotten, the bot does in the chat.
        let asked = self.hub.runtime(&self.cfg.id).message.filter(|_| !self.cfg.is_thread());
        memory::turns::begin_turn(&self.cfg, &sid, asked);
        // Written before this returns, so a Stop right after is sent after the prompt.
        let answer = acp.session(&sid).prompt(prompt).await?;
        if std::mem::take(&mut self.relay.handing_off) {
            self.handed_off(true);
        }
        if !self.turn_memory_revision.is_empty()
            && let Err(error) =
                memory::mark_announced(&self.hub.store, self.cfg.owner(), &sid, &self.turn_memory_revision)
        {
            tracing::warn!(%error, "could not acknowledge memory update notice");
        }
        let done_tx = done_tx.clone();
        tokio::spawn(async move {
            let _ = done_tx.send(answer.response().await.and_then(acp::codebuddy::prompt_result));
        });
        Ok(())
    }

    /// The user's own messages among `entry_ids`, when this is a thread: what they wrote to its thread. Not what
    /// the host wrote there (a task, as the thread is told it).
    fn written_to_thread(&self, entry_ids: &[String]) -> Vec<String> {
        if !self.cfg.is_thread() {
            return Vec::new();
        }
        let entries = entry_ids.iter().filter_map(|id| self.hub.store.entry(id)).filter(|e| e.data["from"] != "host");
        entries.map(|e| work::written(&e.data)).collect()
    }

    /// A thread's turn for what the user wrote to its thread (`written`) begins: its task keeps their words for its
    /// report. What goes before the message: the work itself, when the session that did it couldn't be picked up
    /// and this one is new.
    fn hear_thread(&mut self, written: &[String]) -> Option<String> {
        if written.is_empty() {
            return None;
        }
        let picked_up = self.session_fresh.then(|| work::picked_up(&self.hub.store, &self.cfg)).flatten();
        work::heard(&self.hub, &self.cfg, written);
        picked_up
    }

    /// A thread's first turn: when its task gets a git worktree (`work::worktree`), the thread works there from here,
    /// and `text` says so. Otherwise `text` as it is.
    async fn enter_worktree(&mut self, text: &str) -> String {
        if !self.cfg.is_thread() {
            return text.to_owned();
        }
        let Some(worktree) = work::enter_worktree(&self.hub, &self.cfg).await else { return text.to_owned() };
        self.base.cwd.clone_from(&worktree.cwd);
        self.cfg.cwd.clone_from(&worktree.cwd);
        // Its row too: a host restart resumes it there, and its dev servers and artifacts start from it. Only its
        // folder: its bot's fallbacks may have reached the row meanwhile (`work::fallbacks_changed`).
        let saved = {
            let _g = crate::LockExt::locked(&self.hub.emit_lock);
            self.hub.store.bot(&self.cfg.id).and_then(|row| {
                let row = row.map_or_else(|| self.base.clone(), |row| row.config);
                self.hub.store.save_bot(&BotConfig { cwd: worktree.cwd.clone(), ..row })
            })
        };
        if let Err(error) = saved {
            tracing::warn!(thread = %self.cfg.id, error = format!("{error:#}"), "couldn't save a thread's worktree folder");
        }
        format!("{text}\n\n{}", worktree.brief())
    }

    /// Clears what the last turn left for the one beginning: everything its end reads that no prompt sets.
    pub(super) fn reset_turn_state(&mut self) {
        self.turn_memory_revision.clear();
        self.announce = None;
        self.first_prompt = None;
        self.stop_requested = false;
        self.stopped_for_message = false;
        self.cancel_deadline = None;
        self.exit_tail = None;
        self.agent_error = None;
        self.seg = Seg::None;
        self.tools.clear();
        self.plan_entry = None;
        self.last_text = None;
        self.sent.clear();
        self.reacted = Reacted::No;
        self.joined = false;
        self.unheard.clear();
        self.files.summary = None;
        self.files.cancel();
    }

    /// The running turn ended with `done`; `left`: what a thread's turn left running in the background
    /// ([`Self::left_running`]). Whether the thread's task goes on with that in this session ([`Held::Background`]).
    pub(super) fn finish_turn(&mut self, done: &Done, left: Option<Left>) -> bool {
        if self.turn.is_none() {
            return false;
        }
        self.after_turn(done);
        self.versions.after(&self.hub);
        // The bot's own edits to its memory files, checked and shown in the chat it made them from.
        memory::turns::scan_after_turn(self.hub.clone(), &self.cfg);
        if let (Some(conn), Some(sid)) = (self.conn.as_mut(), &self.turn_session)
            && let Some(used) = conn.loaded.get_mut(sid)
        {
            *used = Instant::now();
        }
        let cancelled = matches!(&done, Ok(v) if StopReason::of(v) == StopReason::Cancelled);
        if cancelled && !self.stop_requested {
            self.requeue_unheard();
        }
        let stopped = self.stop_requested || cancelled;
        self.close_attempt(stopped || done.is_err());
        let mut final_text = None;
        let routine = self.active_routine.is_some();
        let grouped = self.active_group.is_some() || routine || self.active_workflow.is_some();
        let delegated = self.active_request.is_some();
        let sent = std::mem::take(&mut self.sent);
        let reacted = std::mem::take(&mut self.reacted);
        if !sent.is_empty() {
            final_text = Some(sent.join("\n\n"));
        }
        // The bot talked through send_message, or a reaction (or one it held back) was its whole reply: what it wrote
        // stays in the trace. With a message that joined the turn, either may have answered only what came before it.
        if !self.joined && (!sent.is_empty() || !matches!(reacted, Reacted::No)) {
            self.last_text = None;
        }
        if let Some(id) = self.last_text.take().filter(|_| !stopped)
            && let Some(mut e) = self.hub.store.entry(&id)
        {
            let text = e.data["text"].as_str().map(str::to_owned);
            // A pass stays in the trace: neither a room nor the user sees it (a bot request's reply goes back as it
            // is, from a thread too). A routine's pass may follow a line of text — in its chat or in the box — and the
            // run still says nothing.
            let pass: fn(&str) -> bool =
                if self.in_routine() { crate::chat::group::ends_in_pass } else { crate::chat::group::is_pass };
            if self.in_request() || !text.as_deref().is_none_or(pass) {
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
            Ok(v) => StopReason::of(v).as_str().to_owned(),
            Err(_) => "error".into(),
        };
        match (&done, stop_reason.as_str()) {
            (Err(e), _) if !self.stop_requested => {
                let tail = self.exit_tail.take().filter(|t| !t.is_empty());
                if !self.invite_sign_in(e) {
                    let detail = tail.map(|t| format!("\n\n{}", acp::truncate(&t, 1200))).unwrap_or_default();
                    self.failure_notice(e, &format!("The agent failed: {e}{detail}"), "error");
                }
            }
            // Stopped by Send now: the message going out next is what the user sees, not a line drawn under it (a
            // Stop or Remove since took it back).
            _ if self.stop_requested
                && self.stopped_for_message
                && self.queue.iter().any(|q| matches!(q, Queued::User { .. })) => {}
            // A stopped turn says so even when its agent still ended it normally: its reply is withheld above.
            (_, "cancelled") | (Err(_), _) => self.stop_notice("Stopped.", "cancelled"),
            (Ok(_), _) if self.stop_requested => self.stop_notice("Stopped.", "cancelled"),
            (_, "max_tokens") => self.stop_notice("The agent hit its output limit.", "max_tokens"),
            (_, "max_turn_requests") => {
                self.stop_notice("The agent hit its step limit for this turn.", "max_turn_requests");
            }
            (_, "refusal") => self.stop_notice("The agent declined to continue.", "refusal"),
            (Ok(_), "end_turn") if !stopped => crate::market::agent_sign_in::answered(&self.hub, &self.cfg),
            _ => {}
        }
        let failure = done.as_ref().err().filter(|_| !self.stop_requested);
        let failed = failure.is_some();
        self.report_turn(stopped, failure, &stop_reason);
        if done.is_ok() && stop_reason == "end_turn" {
            self.agent_answered();
        }
        let reply = if stopped {
            Err(no_reply::stopped())
        } else if stop_reason != "end_turn" {
            Err(no_reply::did_not_finish(&stop_reason))
        } else {
            final_text.clone().filter(|s| !s.trim().is_empty()).ok_or_else(no_reply::no_text)
        };
        if let Some(step) = self.active_workflow.clone() {
            let timed_out = self.routine_timed_out;
            let stopped_step = !timed_out && (stopped || self.stop_requested);
            let failed_step = timed_out || (!stopped_step && (failed || stop_reason != "end_turn"));
            let output = (!stopped_step && !failed_step).then(|| final_text.clone()).flatten();
            let error = if timed_out {
                Some("This step took too long".into())
            } else {
                failed_step.then(|| format!("The agent didn't finish ({stop_reason})"))
            };
            self.routine_deadline = None;
            self.routine_timed_out = false;
            self.active_workflow = None;
            let keep = self.hub.workflows.finish(&self.hub, &step, output, error, stopped_step);
            if !keep {
                let slot = crate::workflows::session_slot(&step);
                self.set_session(Some(&slot), None);
            }
        }
        if routine {
            let (status, detail) = if self.routine_timed_out {
                (crate::routines::Status::Failed, Some(crate::routines::TIMED_OUT.into()))
            } else if stopped {
                (crate::routines::Status::Cancelled, Some(crate::routines::STOPPED_BY_USER.into()))
            } else if failed || stop_reason != "end_turn" {
                (crate::routines::Status::Failed, Some(crate::routines::did_not_finish(&stop_reason)))
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
        let goes_on = self.end_task(|| {
            if stopped {
                Outcome::Stopped
            } else if let Some(error) = done.as_ref().err() {
                Outcome::Failed(Failure::Agent, format!("its agent failed: {error}"))
            } else if stop_reason != "end_turn" {
                Outcome::Failed(Failure::EndedEarly, format!("it ended early ({stop_reason})"))
            } else {
                match left {
                    Some(Left::Reported) => Outcome::Waiting { text: final_text.clone() },
                    Some(Left::Detached(running)) => Outcome::LeftRunning { running, text: final_text.clone() },
                    None => Outcome::Done(final_text.clone()),
                }
            }
        });
        // Its task goes on with what it left running: nothing is done yet.
        let waits = goes_on == Some(Held::Background);
        self.turn = None;
        self.reply_hops = None;
        let host_turn = self.end_host_turn(done.is_ok() && !stopped && stop_reason == "end_turn");
        if self.routine_completion.is_none() {
            self.set_inflight(false);
        }
        // The message reached the agent: the profile update it carried is now known to it.
        if let (Ok(_), Some((snapshot, identity))) = (&done, self.announce.take())
            && let Err(error) = context::mark_announced(&self.hub.store, self.cfg.owner(), &snapshot, identity)
        {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the profile update");
        }
        if let (Ok(_), Some(sent)) = (&done, self.first_prompt.take())
            && let Err(error) = context::mark_prompted(&self.hub.store, self.cfg.owner(), &sent)
        {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't record the delivered instructions");
        }
        if done.is_ok() {
            self.file_codex_session();
        }
        if let (Some(user), Some(agent), "end_turn") = (self.turn_text.take(), &final_text, stop_reason.as_str()) {
            let turn = crate::chat::naming::Turn { user, agent: agent.clone() };
            let _ = self.dreamer.send(memory::DreamEvent::Turn(turn));
        }
        let needs = goes_on.as_ref().and_then(Held::needs);
        self.hub.set_runtime(&self.id(), |r| {
            r.status = if needs.is_some() {
                BotStatus::NeedsInput
            } else if failed {
                BotStatus::Error
            } else {
                BotStatus::Idle
            };
            r.activity = needs.unwrap_or_default();
            r.started_at = None;
            r.lane = None;
            r.message = None;
            r.session = None;
        });
        self.wait_for_sign_in(goes_on.as_ref());
        self.hub.team.end_turn(&self.cfg.id);
        // A room turn's news comes from the group once the room is done (see `group`).
        let more = self.queue.iter().any(|q| matches!(q, Queued::User { .. }));
        // It held back its only answer, a reaction: it said nothing, so there is nothing to tell either.
        let held_back = final_text.is_none() && matches!(reacted, Reacted::Refused);
        // A turn the host started that found nothing worth saying (a follow-up's `(pass)`): the same.
        let passed = final_text.is_none() && host_turn.is_some();
        if !self.stop_requested
            && !delegated
            && !grouped
            && host_turn.is_none_or(HostTurn::alerts)
            && let Some(kind) = turn_alert(failed, &stop_reason, more || held_back || passed)
        {
            let body = final_text.unwrap_or_else(|| match (kind, reacted) {
                (AlertKind::Done, Reacted::With(emoji)) => format!("Reacted {emoji} to your message"),
                (AlertKind::Done, _) => "Finished.".into(),
                _ => "The agent didn't finish.".into(),
            });
            push::notify(&self.hub, &self.cfg, None, &self.cfg.name, &body, kind, None);
        }
        waits
    }
}

impl Actor {
    /// Says why the running turn ended early; `stopReason` tells scripts (`codync-host send`) without the words.
    fn stop_notice(&self, text: &str, reason: &str) {
        self.notice_data(json!({"text": text, "style": NoticeStyle::Info, "stopReason": reason}));
    }

    /// Ends what an attempt left open: file shares, the last text, unanswered permission cards (they can't be
    /// answered after the turn) and tool calls the agent never closed out (they'd spin forever in the trace).
    pub(super) fn close_attempt(&mut self, failed: bool) {
        self.files.cancel();
        self.end_seg(false);
        self.expire_decisions(crate::chat::decision::Expiry::TurnEnded);
        self.expire_perms();
        for entry_id in self.tools.values() {
            if let Some(mut e) = self.hub.store.entry(entry_id)
                && matches!(e.data["status"].as_str(), Some("pending" | "in_progress"))
            {
                e.data["status"] = if failed { "failed" } else { "completed" }.into();
                self.hub.set_entry(entry_id, &e.data);
            }
        }
    }

    /// Expires the permission cards still unanswered: no one can answer them once their turn, or the actor, is
    /// gone, and a pending card would stay in its chat's badge (`Store::badge`).
    pub(super) fn expire_perms(&mut self) {
        // The agent would wait on a card no one can answer any more: its request, dropped, answers `cancelled`.
        for (id, _answer) in self.perms.drain() {
            if let Some(mut e) = self.hub.store.entry(&id) {
                e.data["status"] = "expired".into();
                self.hub.set_entry(&id, &e.data);
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
        let (source, work) = (self.source(), self.task_source.kind());
        analytics::turn_finished(&self.hub, &self.cfg, TurnEnd { outcome, stop_reason, failure, source, work, ms });
    }

    /// The turn is a routine's run: in the chat, or a thread's whose task is one.
    pub(super) fn in_routine(&self) -> bool {
        self.active_routine.is_some() || matches!(self.task_source, Source::Routine { .. })
    }

    /// The turn answers another bot's request: in the chat, or a thread's whose task is one.
    pub(super) fn in_request(&self) -> bool {
        self.active_request.is_some() || matches!(self.task_source, Source::Request { .. })
    }

    /// The turn is the bot's own (a thread on the bot's own work counts): not a room's, a routine's run or another
    /// bot's request.
    pub(super) fn in_own_chat(&self) -> bool {
        self.active_group.is_none() && !self.in_routine() && !self.in_request()
    }

    /// What started the turn that is starting or running. A thread's turn is what its task came from.
    pub(super) fn source(&self) -> TurnSource {
        if self.in_routine() || self.active_workflow.is_some() {
            TurnSource::Routine
        } else if self.active_group.is_some() {
            TurnSource::Group
        } else if self.in_request() || self.reply_hops.is_some() {
            TurnSource::Bot
        } else {
            self.host_turn.map_or(TurnSource::User, HostTurn::source)
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
    fn only_finished_threads_alert() {
        assert_eq!(turn_alert(false, "end_turn", false), Some(AlertKind::Done));
        assert_eq!(turn_alert(false, "end_turn", true), None, "next message queued, or a reaction held back");
        assert_eq!(turn_alert(false, "cancelled", false), None);
        assert_eq!(turn_alert(false, "max_turn_requests", false), Some(AlertKind::Failed));
        assert_eq!(turn_alert(true, "error", true), Some(AlertKind::Failed));
    }
}
