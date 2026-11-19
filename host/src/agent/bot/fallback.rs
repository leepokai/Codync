//! The bot's side of its fallback chain (`agent::fallback`). Each turn starts on the first agent, in order, that
//! isn't resting; when the agent fails for a reason that moves on, the same turn starts again on the next one, told
//! what the agents before it did. Each agent keeps its own session (`park`), so going back resumes it, with a
//! catch-up of what was said meanwhile.

mod park;

pub(super) use park::resume_step;

use super::sign_in::Held;
use super::{Actor, Done, NoticeStyle};
use crate::LockExt;
use crate::agent::fallback::{self, Reason, carry, rest};
use crate::agent::login_profiles::Login;
use crate::analytics::Carried;
use crate::store::{BotConfig, Entry, EntryKind, Lane, now_ms};
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// What a chat line carries of a handoff for the apps to unfold, in bytes (`carry::excerpt`).
const LINE_CARRIED: usize = 8_000;
/// Chat messages read for a recap or a catch-up.
const READ: i64 = 120;
/// Tool calls a takeover lists, the latest ones.
const TOOLS_LISTED: usize = 40;

/// The context window the agent on `login` last reported in `bot`'s own chat, in tokens (`note_agent_window`).
/// Deleted with the bot's other `fallback.` records.
fn agent_window_key(bot: &str, login: &str) -> String {
    format!("fallback.window.{bot}.{login}")
}

/// What a turn needs to start again on the next agent.
#[derive(Clone)]
struct Attempt {
    lane: Lane,
    entries: Vec<String>,
    text: String,
}

/// The running turn's place in the chain.
#[derive(Default)]
pub(super) struct Relay {
    /// The turn, to start again elsewhere; `None` for a hidden turn (a compaction, a resume).
    attempt: Option<Attempt>,
    /// Agents (their `login::of`) this turn already failed on.
    tried: Vec<String>,
    /// Agents (their `login::of`) that can't run their step as set up (no such model or mode): skipped, and the
    /// turn moves on, without resting them.
    skipped: Vec<String>,
    /// Agents this turn left signed out, in order, whose sign-in cards are in the chat. When no agent can answer, the
    /// first one's card holds the turn's messages.
    signed_out: Vec<Login>,
    /// The next `start_turn` is this turn again, on the next agent.
    retrying: bool,
    /// The agents that failed this turn, in order: why, and what each had done.
    failed: Vec<carry::Failed>,
    /// Why the agent just left stopped and when it comes back, for the chat line.
    stopped: Option<(Reason, Option<i64>)>,
    /// Every agent of the chain failed this turn: its failure notice says how each one is.
    exhausted: bool,
    /// The main session moved off this agent (and its session): its next turn there starts with a recap or a
    /// catch-up. Kept until the main session comes up on an agent.
    recap: Option<(String, Option<String>)>,
    /// This attempt's move in the chat, which shows what the agent was told once its session is up.
    line: Option<String>,
    /// What this attempt's agent hears first after the user's handoff (`carry_in`); this turn's only.
    pub(super) carried: Option<String>,
    /// The turn's messages, held for a sign-in or sent again: a task's item goes on past the turn (`item_goes_on`).
    pub(super) held: Option<Held>,
}

/// The bot as it runs on `current`'s agent with `base`'s settings, also when that agent is no longer in its chain.
pub(super) fn running(store: &crate::store::Store, base: &BotConfig, current: &BotConfig) -> BotConfig {
    match fallback::step_of(store, base, &fallback::login::of(current)) {
        Some(step) => fallback::effective(store, base, step),
        None => BotConfig {
            backend: current.backend.clone(),
            command: current.command.clone(),
            login_profile: current.login_profile.clone(),
            model: current.model.clone(),
            model_options: current.model_options.clone(),
            ..base.clone()
        },
    }
}

impl Actor {
    /// The step of the agent the bot is on (its own: 0).
    pub(super) fn step(&self) -> Option<usize> {
        fallback::step_of(&self.hub.store, &self.base, &fallback::login::of(&self.cfg))
    }

    /// The first step, in order, whose agent this turn hasn't failed on and that isn't resting. A signed-out agent
    /// rests too: the sign-in state an app last checked can be stale.
    fn next_step(&self, now: i64) -> Option<fallback::Picked> {
        let passed = |login: &str| self.relay.tried.iter().chain(&self.relay.skipped).any(|l| l == login);
        fallback::pick_step(&self.hub.store, &self.base, now, passed)
    }

    /// `start_turn`, once the turn has its lane and number: picks the agent. Not for hidden turns.
    pub(super) async fn relay_turn(&mut self, lane: &Lane, entries: &[String], text: &str) {
        if !std::mem::take(&mut self.relay.retrying) {
            self.relay.tried.clear();
            self.relay.failed.clear();
            self.relay.signed_out.clear();
            self.relay.stopped = None;
            self.relay.held = None;
        }
        self.relay.exhausted = false;
        self.relay.line = None;
        self.relay.carried = None;
        self.relay.attempt = Some(Attempt { lane: lane.clone(), entries: entries.to_vec(), text: text.to_owned() });
        let now = now_ms();
        let target = self.next_step(now).unwrap_or_else(|| fallback::Picked::own(&self.base));
        for login in &target.signed_out_before {
            self.left_signed_out(login);
        }
        let from = self.cfg.clone();
        if fallback::login::of(&target.cfg) != fallback::login::of(&from) {
            self.switch_to(target.cfg).await;
            self.relay.line = self.tell_switch(&from);
        }
    }

    /// A message that joined the running turn (`steer`) goes with it to the next agent.
    pub(super) fn relay_joined(&mut self, entry_id: &str, text: &str) {
        if let Some(attempt) = &mut self.relay.attempt {
            attempt.entries.push(entry_id.to_owned());
            attempt.text = format!("{}\n\n{text}", attempt.text);
        }
    }

    /// `start_turn`, once the session is up (a new one, or the agent's own resumed): sets what the agent should hear
    /// first, and puts it on the chat line of the move. The main session gets a recap of the chat in a new session or
    /// a catch-up in its own; a retried turn gets what the agents before did. Not for hidden turns.
    pub(super) fn carry_in(&mut self) {
        let mut carried = Vec::new();
        if self.slot().is_none()
            && let Some((agent, session)) = self.relay.recap.take()
        {
            carried.extend(self.chat_for_task());
            carried.extend(self.recap(&agent, session.as_deref()));
        }
        carried.extend(carry::takeover(&self.relay.failed));
        let text = (!carried.is_empty()).then(|| carried.join("\n\n"));
        if let Some(text) = &text
            && let Some(id) = self.relay.line.take()
            && let Some(mut line) = self.hub.store.entry(&id)
        {
            line.data["agentSwitch"]["carried"] = carry::excerpt(text, LINE_CARRIED).into();
            self.hub.set_entry(&id, &line.data);
        }
        self.relay.carried = text;
    }

    /// A task's new session on another agent: the chat it comes from, which its first session had (by a fork, or
    /// told), unless this session is its first and is told it already.
    fn chat_for_task(&self) -> Option<String> {
        let new = self.cfg.is_task() && self.session_fresh && self.handoff.is_none();
        new.then(|| self.told_chat()).flatten()
    }

    /// What a task from the chat is told of it in a new session: the chat so far. Not a routine's run.
    pub(super) fn told_chat(&self) -> Option<String> {
        let from_chat = crate::chat::tasks::from_chat(&self.task_source);
        from_chat.then(|| crate::chat::tasks::chat_so_far(&self.hub.store, self.cfg.owner())).flatten()
    }

    /// What the chat calls the agent `cfg` runs: its name, with the login profile it runs on ("Claude Code
    /// (Work)"), which is what tells two steps on one agent apart.
    fn called(&self, cfg: &BotConfig) -> String {
        self.named(&cfg.backend, cfg.login_profile.as_deref())
    }

    /// What the chat calls agent `backend` on `login_profile` (the computer's own login: none).
    pub(super) fn named(&self, backend: &str, login_profile: Option<&str>) -> String {
        let agent = fallback::name(backend);
        match crate::agent::login_profiles::name_of(&self.hub.store, backend, login_profile) {
            Some(profile) => format!("{agent} ({profile})"),
            None => agent,
        }
    }

    /// The chat line for a move from the agent `from` ran on to `self.cfg`'s (`carry_in` adds what the new agent
    /// was told).
    fn tell_switch(&self, from: &BotConfig) -> Option<String> {
        let to = self.cfg.backend.clone();
        let from_login = fallback::login::of(from);
        let to_login = fallback::login::of(&self.cfg);
        let own_login = fallback::login(&self.hub.store, &self.base, 0);
        let (from_name, to_name) = (self.called(from), self.called(&self.cfg));
        let from = from.backend.as_str();
        // Toward the bot's own agent (or an earlier step when idle), never when falling back mid-turn.
        let back = self.relay.stopped.is_none()
            && (to_login == own_login
                || matches!(
                    (fallback::step_of(&self.hub.store, &self.base, &from_login), self.step()),
                    (Some(from_step), Some(to_step)) if to_step < from_step
                ));
        let (reason, until) = self
            .relay
            .stopped
            .or_else(|| rest::get(&self.hub.store, &from_login, now_ms()).map(|r| (r.reason, Some(r.until))))
            .unzip();
        let until = until.flatten();
        let text = if back {
            format!("Back on {to_name}.")
        } else {
            let why = reason.map_or("can't answer right now", Reason::phrase);
            let when = until
                .filter(|_| reason == Some(Reason::UsageLimit))
                .map(|t| format!(" (resets {})", carry::in_words(t - now_ms())))
                .unwrap_or_default();
            format!("{from_name} {why}{when}. {to_name} carried on.")
        };
        let data = json!({
            "text": text,
            "style": "info",
            "agentSwitch": {
                "from": from, "to": to, "back": back, "reason": reason, "resetsAt": until, "carried": null,
            },
        });
        self.line(data)
    }

    /// A notice with its own fields in the running turn's lane (or the bot's own chat between turns).
    fn line(&self, data: Value) -> Option<String> {
        let entry = if let Some(turn) = self.turn {
            self.add(EntryKind::Notice, turn, data)
        } else {
            let main = Lane::main(&self.base.id);
            self.hub.add_entry(&main, EntryKind::Notice, self.hub.store.max_turn(&self.base.id), &data)
        };
        entry.map(|e| e.id)
    }

    /// The main session is left (a move to another agent, or its agent starts again and can't reopen it): the next
    /// one hears the conversation so far with this agent. A recap still owed is of an agent before it, which this one
    /// never got to answer.
    pub(super) fn owe_recap(&mut self) {
        if self.relay.recap.is_none() {
            self.relay.recap = Some((self.cfg.backend.clone(), self.session_id.clone()));
        }
    }

    /// What the agent the main session just moved to should hear first: a catch-up when it resumed its own
    /// session, a recap of the conversation when it started a new one. Only the user's messages and the bot's
    /// replies count, never from before the user last started a new session.
    fn recap(&self, from: &str, from_session: Option<&str>) -> Option<String> {
        let (kind, room, text) = if self.session_fresh {
            let room = self.agent_window().map(carry::room);
            (Carried::Recap, room, self.recap_text(from, from_session, room)?)
        } else {
            // A catch-up isn't sized to the agent's window.
            (Carried::CatchUp, None, self.catch_up_text()?)
        };
        let on_profile = Login::of(&self.hub.store, &self.cfg).profile.is_some();
        crate::analytics::handoff_carried(&self.hub, &self.cfg.backend, on_profile, kind, text.len(), room.is_some());
        Some(text)
    }

    /// The main chat's last entry when the user last started a new session: no agent hears what came before it.
    fn fresh(&self) -> i64 {
        self.hub.store.kv_get(&park::fresh_key(&self.base.id)).and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// The main chat's messages after `after`, without the running turn's own.
    fn main_messages(&self, after: i64) -> Option<Vec<Entry>> {
        let current = self.relay.attempt.as_ref().map(|a| a.entries.as_slice()).unwrap_or_default();
        let mut entries = self.hub.store.messages_after(&Lane::main(&self.base.id), after, READ).ok()?;
        entries.retain(|e| !current.contains(&e.id));
        Some(entries)
    }

    /// For an agent back in the session it had: what was said, and what the other agents did, since it left.
    fn catch_up_text(&self) -> Option<String> {
        let left = self
            .hub
            .store
            .kv_get(&park::left_key(&self.base.id, &fallback::login::of(&self.cfg)))
            .and_then(|v| v.parse().ok());
        let since = left.unwrap_or(i64::MAX).max(self.fresh());
        let ledger = self.ledger(since);
        carry::catch_up(&carry::said(&self.main_messages(since)?), ledger.as_deref())
    }

    /// For an agent in a new session: the conversation so far with agent `from` (in its session `from_session`),
    /// within `room` bytes.
    fn recap_text(&self, from: &str, from_session: Option<&str>, room: Option<usize>) -> Option<String> {
        let bot = &self.base.id;
        let fresh = self.fresh();
        let entries = self.main_messages(fresh)?;
        let said = carry::said(&entries);
        if said.is_empty() {
            return None;
        }
        // The open plan and the ledger, as far back as the messages read.
        let oldest = entries.first().map_or(0, |e| e.seq).max(fresh);
        let from = fallback::name(from);
        let summary = from_session.and_then(|s| self.hub.store.kv_get(&crate::chat::handoff::summary_key(bot, s)));
        let plan = self
            .hub
            .store
            .last_of_kind(&Lane::main(bot), EntryKind::Plan, oldest)
            .map(|e| crate::chat::handoff::plan_text(&e.data));
        let ledger = self.ledger(oldest);
        let recap = carry::Recap {
            from: &from,
            summary: summary.as_deref(),
            said: &said,
            plan: plan.as_deref(),
            bookmark: entries.last().map(|e| e.id.as_str()),
            ledger: ledger.as_deref(),
            room,
        };
        Some(recap.text())
    }

    /// The ledger of what agents did in the main chat after `after`, before this turn (`carry::ledger`).
    fn ledger(&self, after: i64) -> Option<String> {
        let bot = &self.base.id;
        // This turn's own calls go with the takeover (`so_far`). A room's turns are numbered apart from the main
        // chat's, and their calls aren't in it.
        let running = self.turn.filter(|_| self.lane.chat == *bot);
        let tools = self.hub.store.tools_after(&Lane::main(bot), after, running, carry::ledger::READS);
        let tools = tools.unwrap_or_else(|error| {
            tracing::warn!(bot = %bot, error = format!("{error:#}"), "couldn't read the tool calls for a handoff");
            Vec::new()
        });
        carry::ledger::Ledger::of(&tools, &self.hub.dev_servers.active(bot)).render()
    }

    /// The window this agent last reported in the bot's own chat, in tokens.
    fn agent_window(&self) -> Option<u64> {
        let key = agent_window_key(&self.base.id, &fallback::login::of(&self.cfg));
        self.hub.store.kv_get(&key).and_then(|v| v.parse().ok())
    }

    /// Keeps the window this agent reports, in tokens: what a recap for it may fill a share of (`carry::room`). The
    /// agent's last, so a model changed since counts from its next turn.
    pub(super) fn note_agent_window(&self, size: u64) {
        if size == 0 || self.agent_window() == Some(size) {
            return;
        }
        let key = agent_window_key(&self.base.id, &fallback::login::of(&self.cfg));
        if let Err(error) = self.hub.store.kv_set(&key, &size.to_string()) {
            tracing::warn!(bot = %self.base.id, error = format!("{error:#}"), "couldn't record the agent's window");
        }
    }

    /// What the failing attempt did: its tool calls, the messages it sent, its last words.
    fn so_far(&self) -> Option<String> {
        let mut tools: Vec<_> = self.tools.values().filter_map(|id| self.hub.store.entry(id)).collect();
        tools.sort_by_key(|e| e.seq);
        let skip = tools.len().saturating_sub(TOOLS_LISTED);
        let tools: Vec<(String, String)> = tools[skip..]
            .iter()
            .map(|e| {
                let title = e.data["title"].as_str().unwrap_or("A tool call").to_owned();
                (title, e.data["status"].as_str().unwrap_or("pending").to_owned())
            })
            .collect();
        let narration = self.last_text.as_ref().and_then(|id| self.hub.store.entry(id));
        carry::so_far(&tools, &self.sent, narration.as_ref().and_then(|e| e.data["text"].as_str()))
    }

    /// The turn failed with `error`: when it may move on and another agent can take it, starts it again there.
    /// Whether the turn is handled (running again, or its failure reported).
    pub(super) async fn fall_back(&mut self, error: &anyhow::Error, done_tx: &mpsc::UnboundedSender<Done>) -> bool {
        if self.stop_requested || self.routine_timed_out || self.compacting.is_some() || self.turn.is_none() {
            return false;
        }
        let (Some(mut reason), Some(attempt)) = (fallback::reason::of(error), self.relay.attempt.clone()) else {
            return false;
        };
        let mut start_error: Option<anyhow::Error> = None;
        loop {
            let now = now_ms();
            let (agent, key) = (self.cfg.backend.clone(), fallback::login::of(&self.cfg));
            let login = Login::of(&self.hub.store, &self.cfg);
            if reason == Reason::SignedOut {
                self.note_signed_out(&login);
            }
            let failed = start_error.as_ref().unwrap_or(error);
            let resets =
                rest::reset_until(&self.hub.usage.locked(), &agent, login.profile.as_deref(), &failed.to_string(), now);
            let rest = rest::record(&self.hub.store, &key, reason, now, resets);
            if rest.is_some() {
                fallback::tell_bots(&self.hub, &key);
            }
            if matches!(reason, Reason::ModelMissing | Reason::ModeMissing) {
                self.relay.skipped.push(key.clone());
            }
            self.relay.tried.push(key);
            let Some(next) = self.next_step(now) else {
                self.relay.exhausted = true;
                // No agent answers the turn's messages: they go out again once the first one left signed out is
                // signed in (the last agent's own card holds them only when there is none, `held_for_sign_in`).
                if let Some(first) = self.relay.signed_out.first().filter(|_| !self.turn_entries.is_empty()).cloned() {
                    let entries = self.turn_entries.clone();
                    self.sign_in_card(&first, &entries);
                }
                return match start_error {
                    Some(e) => {
                        self.start_failed(&e);
                        true
                    }
                    None => false,
                };
            };
            if reason == Reason::SignedOut {
                self.left_signed_out(&login);
            }
            self.report_turn(false, Some(failed), "error");
            let to = Login::of(&self.hub.store, &next.cfg);
            let on_profile = login.profile.is_some() || to.profile.is_some();
            crate::analytics::agent_fell_back(&self.hub, &agent, on_profile, reason);
            let done = self.so_far();
            self.relay.failed.push(carry::Failed { agent: self.called(&self.cfg), reason, done });
            self.relay.stopped = Some((reason, rest.map(|r| r.until)));
            self.close_attempt(true);
            self.relay.retrying = true;
            match self.start_turn(attempt.lane.clone(), &attempt.entries, &attempt.text, false, done_tx).await {
                Ok(()) => return true,
                Err(e) => {
                    let Some(r) = fallback::reason::of(&e) else {
                        self.start_failed(&e);
                        return true;
                    };
                    reason = r;
                    start_error = Some(e);
                }
            }
        }
    }

    /// The turn moves past agent `agent`, signed out: its sign-in card goes in the chat, holding no message (the next
    /// agent answers them, unless none can: `fall_back`). Signing in there brings the bot back (`fallback::signed_in`).
    /// Once per turn.
    fn left_signed_out(&mut self, login: &Login) {
        if crate::agent::backends::harness(&login.backend).is_some() && !self.relay.signed_out.contains(login) {
            self.relay.signed_out.push(login.clone());
            self.sign_in_card(login, &[]);
        }
    }

    /// The messages the sign-in card of the agent this turn ended on holds: none when the card of an agent the turn
    /// left signed out holds them already (`fall_back` ran out of agents), so they go out again once.
    pub(super) fn held_for_sign_in(&self) -> &[String] {
        if self.relay.exhausted && !self.relay.signed_out.is_empty() { &[] } else { &self.turn_entries }
    }

    /// A hidden turn (a compaction, a resume) stays on its agent and isn't started again elsewhere.
    pub(super) fn hidden_turn(&mut self) {
        self.relay.attempt = None;
        self.relay.retrying = false;
        self.relay.exhausted = false;
        self.relay.carried = None;
        self.relay.held = None;
    }

    /// The failure notice: with a failure that moves on, a bot without fallbacks offers to add one, and a bot whose
    /// agents all failed this turn (`fall_back` ran out of them) says how each one is. It carries the turn's stop
    /// `reason` (see `stop_notice`).
    pub(super) fn failure_notice(&self, error: &anyhow::Error, text: &str, reason: &str) {
        let mut data = json!({"text": text, "style": NoticeStyle::Error, "stopReason": reason});
        let steps = fallback::steps(&self.hub.store, &self.base);
        match fallback::reason::of(error) {
            Some(Reason::UsageLimit | Reason::OutOfCredits | Reason::RateLimited | Reason::Overloaded)
                if steps <= 1 =>
            {
                data["addFallback"] = true.into();
            }
            Some(_) if self.relay.exhausted && steps > 1 => {
                data["text"] = format!("{text}\n\n{}", self.chain_note()).into();
            }
            _ => {}
        }
        // A task's offer to add a fallback is the user's to take, in its bot's chat; its own transcript keeps the
        // failure.
        // There it ends no turn of the bot's own: no `stopReason` (`codync-host send --wait` reads one as its end).
        if self.cfg.is_task()
            && let Some(offer) = data.as_object_mut().and_then(|d| d.remove("addFallback"))
        {
            self.add_for_user(
                EntryKind::Notice,
                json!({"text": data["text"], "style": data["style"], "addFallback": offer}),
            );
        }
        self.line(data);
    }

    /// How each agent of the chain is: "No agent in this bot's list could answer: Claude Code reached its usage
    /// limit (resets in 2 h); Codex is rate limited (back in 4 min)."
    fn chain_note(&self) -> String {
        let now = now_ms();
        let store = &self.hub.store;
        let parts: Vec<String> = (0..fallback::steps(store, &self.base))
            .map(|s| {
                let agent = self.called(&fallback::effective(store, &self.base, s));
                match rest::get(store, &fallback::login(store, &self.base, s), now) {
                    Some(r) => {
                        let when = if r.reason == Reason::UsageLimit { "resets" } else { "back" };
                        let wait = carry::in_words(r.until - now);
                        format!("{agent} {} ({when} {wait})", r.reason.phrase())
                    }
                    None => format!("{agent} couldn't answer"),
                }
            })
            .collect();
        format!("No agent in this bot's list could answer: {}.", parts.join("; "))
    }

    /// A turn that couldn't start: on the next agent if one can take it, otherwise the failure as usual.
    pub(super) async fn start_error(&mut self, error: anyhow::Error, done_tx: &mpsc::UnboundedSender<Done>) {
        if !self.fall_back(&error, done_tx).await {
            self.start_failed(&error);
        }
    }

    /// The agent answered a turn: whatever rest it had is over.
    pub(super) fn agent_answered(&mut self) {
        fallback::end_rest(&self.hub, &fallback::login::of(&self.cfg));
    }

    /// `switchBack`: the bot's own agent again, now when idle, or for its next turn.
    pub(super) async fn switch_back(&mut self) {
        let own = fallback::login(&self.hub.store, &self.base, 0);
        fallback::end_rest(&self.hub, &own);
        if self.turn.is_none() && fallback::login::of(&self.cfg) != own {
            let from = self.cfg.clone();
            self.idle_relay();
            self.switch_to(fallback::effective(&self.hub.store, &self.base, 0)).await;
            self.tell_switch(&from);
        }
    }

    /// After a settings change: forgets parked sessions of agents whose step changed or left the chain, and moves an
    /// idle bot off an agent it no longer uses.
    pub(super) async fn settle_chain(&mut self, old: &BotConfig) {
        self.forget_changed(old);
        // A model or mode the agent lacked may be one the bot no longer asks for.
        self.relay.skipped.clear();
        if self.step().is_none() && self.turn.is_none() {
            let from = self.cfg.clone();
            self.idle_relay();
            let target = self.next_step(now_ms()).unwrap_or_else(|| fallback::Picked::own(&self.base));
            self.switch_to(target.cfg).await;
            self.tell_switch(&from);
        } else {
            self.mark_step();
        }
    }

    /// Between turns: the last turn's place in the chain is over, not the recap the main session still owes.
    fn idle_relay(&mut self) {
        self.relay = Relay { recap: self.relay.recap.take(), ..Relay::default() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::path::PathBuf;
    use std::time::Instant;

    fn test_actor(store: crate::store::Store, base: BotConfig) -> (Actor, PathBuf) {
        let dir = std::env::temp_dir().join(format!("codync-bot-fallback-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        store.save_bot(&base).expect("save bot");
        let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("identity");
        let hub = crate::hub::Hub::new(store, "bot-fallback-test".into(), identity, "test".into(), 0);
        hub.start().expect("start hub");
        let lane = Lane::main(&base.id);
        let cfg = base.clone();
        let actor = Actor {
            hub,
            base,
            cfg,
            conn: None,
            session_id: None,
            run_sessions: HashMap::new(),
            lane,
            turn_session: None,
            active_group: None,
            active_routine: None,
            routine_deadline: None,
            routine_timed_out: false,
            routine_completion: None,
            session_fresh: false,
            profile_resend: HashSet::new(),
            compacting: None,
            raw_prompt: false,
            handoff: None,
            compaction_notice: None,
            applied_system: HashMap::new(),
            dreamer: mpsc::unbounded_channel().0,
            turn_text: None,
            versions: crate::agent::versions::Saves::default(),
            turn_entries: Vec::new(),
            turn_call: None,
            turn_memory_revision: String::new(),
            announce: None,
            first_prompt: None,
            turn: None,
            queue: VecDeque::new(),
            steers_in_flight: HashMap::new(),
            held_done: None,
            active_request: None,
            task_source: crate::chat::tasks::Source::Bot,
            reply_hops: None,
            host_turn: None,
            reports: Vec::new(),
            prompt_call: None,
            seg: super::super::Seg::None,
            tools: HashMap::new(),
            plan_entry: None,
            last_text: None,
            sent: Vec::new(),
            reacted: super::super::Reacted::No,
            joined: false,
            files: super::super::files::FileShares::default(),
            perms: HashMap::new(),
            asked: HashMap::new(),
            open_card: None,
            stop_requested: false,
            stopped_for_message: false,
            cancel_deadline: None,
            exit_tail: None,
            agent_error: None,
            restart_before_turn: false,
            restart_after_turn: false,
            fast_support: crate::agent::config::FastModeSupport::from_stored(None),
            idle_checked: Instant::now(),
            relay: Relay::default(),
        };
        (actor, dir)
    }

    #[test]
    fn a_bot_keeps_running_an_agent_that_left_its_chain_until_it_moves_off() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("a store");
        let mut base: BotConfig =
            serde_json::from_value(json!({"id": "b", "name": "B", "backend": "claude", "model": "opus"})).expect("bot");
        let mut on_codex = base.clone();
        on_codex.backend = "codex".into();
        on_codex.model = Some("gpt-5.5".into());
        base.description = "new instructions".into();
        let cfg = running(&store, &base, &on_codex);
        assert_eq!((cfg.backend.as_str(), cfg.model.as_deref()), ("codex", Some("gpt-5.5")));
        assert_eq!(cfg.description, "new instructions");
        assert_eq!(running(&store, &base, &base).backend, "claude");
    }

    #[tokio::test]
    async fn forget_parked_clears_signed_out_and_removed_profile_sessions() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("a store");
        crate::agent::login_profiles::tests::insert(&store, &[("fp-1", "claude"), ("fp-2", "claude")]);
        crate::agent::login_profiles::note_signed_in(&store, "fp-1", true);
        let base: BotConfig = serde_json::from_value(json!({
            "id": "b",
            "name": "B",
            "backend": "claude",
            "fallbacks": [{"backend": "codex"}],
        }))
        .expect("bot");
        store.kv_set("fallback.session.b.claude#profile-fp-1", "sid-fp1").expect("park fp1");
        store.kv_set("fallback.session.b.claude#profile-fp-2", "sid-fp2").expect("park fp2");
        store.kv_set("fallback.session.b.codex", "sid-codex").expect("park codex");
        store.kv_set("fallback.session.other.claude#profile-fp-1", "sid-other").expect("park other");
        crate::agent::login_profiles::note_signed_in(&store, "fp-1", false);

        let (mut actor, dir) = test_actor(store, base);
        actor.forget_parked();
        assert!(actor.hub.store.kv_prefix("fallback.session.b.").is_empty(), "every parked session of bot b cleared");
        assert_eq!(
            actor.hub.store.kv_get("fallback.session.other.claude#profile-fp-1").as_deref(),
            Some("sid-other"),
            "other bots stay untouched"
        );
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[tokio::test]
    async fn switch_to_keeps_selected_target_when_profiles_sign_out_mid_switch() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("a store");
        crate::agent::login_profiles::tests::insert(&store, &[("sw-1", "claude")]);
        crate::agent::login_profiles::note_signed_in(&store, "sw-1", true);
        let base: BotConfig = serde_json::from_value(json!({
            "id": "b",
            "name": "B",
            "backend": "claude",
            "fallbacks": [{"backend": "codex"}],
        }))
        .expect("bot");
        let now = now_ms();
        rest::record(&store, "claude", Reason::UsageLimit, now, None);
        rest::record(&store, "claude#profile-sw-1", Reason::UsageLimit, now, None);

        let (mut actor, dir) = test_actor(store, base);
        let picked = actor.next_step(now).expect("codex fallback step");
        assert_eq!(picked.cfg.backend, "codex");

        // Concurrent sign-out shrinks chain indices before `switch_to` runs.
        crate::agent::login_profiles::note_signed_in(&actor.hub.store, "sw-1", false);
        actor.switch_to(picked.cfg).await;

        assert_eq!(actor.cfg.backend, "codex");
        assert_eq!(actor.cfg.login_profile, None);
        let step = actor.hub.runtime("b").agent_step.expect("agent_step recorded");
        assert_eq!(step.agent, "codex");
        assert_eq!(step.step, 1);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[tokio::test]
    async fn resume_step_keeps_signed_out_auto_profile_and_protects_primary_parked_session() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("a store");
        crate::agent::login_profiles::tests::insert(&store, &[("rs-1", "claude")]);
        crate::agent::login_profiles::note_signed_in(&store, "rs-1", true);
        let base: BotConfig = serde_json::from_value(json!({
            "id": "b",
            "name": "B",
            "backend": "claude",
            "model": "opus",
            "fallbacks": [{"backend": "codex"}],
        }))
        .expect("bot");
        store.save_bot(&base).expect("save bot");
        store.set_session("b", Some("sid-rs1")).expect("save rs1 session");
        store.kv_set("fallback.on.b", "claude#profile-rs-1").expect("mark on rs1");
        store.kv_set("fallback.session.b.claude", "sid-primary").expect("park primary");

        // Another bot marks rs-1 signed out while host is down/restarting.
        crate::agent::login_profiles::note_signed_in(&store, "rs-1", false);
        let (mut actor, dir) = test_actor(store, base.clone());
        let saved = actor.hub.store.bot("b").ok().flatten().and_then(|b| b.session_id);
        let (cfg, session_id) = resume_step(&actor.hub, &base, saved);
        assert_eq!(fallback::login::of(&cfg), "claude#profile-rs-1");
        assert_eq!(cfg.model.as_deref(), Some("opus"));
        assert_eq!(session_id.as_deref(), Some("sid-rs1"));

        actor.cfg = cfg;
        actor.session_id = session_id;
        // Switching to codex parks sid-rs1 under rs-1, leaving primary's parked session intact.
        rest::record(&actor.hub.store, "claude", Reason::UsageLimit, now_ms(), None);
        let codex = actor.next_step(now_ms()).expect("codex step");
        assert_eq!(codex.cfg.backend, "codex");
        actor.switch_to(codex.cfg).await;
        assert_eq!(actor.hub.store.kv_get("fallback.session.b.claude").as_deref(), Some("sid-primary"));
        assert_eq!(actor.hub.store.kv_get("fallback.session.b.claude#profile-rs-1").as_deref(), Some("sid-rs1"));

        // If a saved profile was deleted altogether, resume_step restores the primary's parked session onto base.
        actor.hub.store.kv_set("fallback.on.b", "claude#profile-deleted").expect("mark deleted");
        actor.hub.store.set_session("b", Some("sid-deleted")).expect("save deleted session");
        let (cfg, session_id) = resume_step(&actor.hub, &base, Some("sid-deleted".into()));
        assert_eq!(fallback::login::of(&cfg), "claude");
        assert_eq!(session_id.as_deref(), Some("sid-primary"));
        assert_eq!(actor.hub.store.bot("b").ok().flatten().and_then(|b| b.session_id).as_deref(), Some("sid-primary"));
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
