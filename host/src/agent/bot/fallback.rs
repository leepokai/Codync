//! The bot's side of its fallback chain (`agent::fallback`). Each turn starts on the first agent, in order, that
//! isn't resting; when the agent fails for a reason that moves on, the same turn starts again on the next one, told
//! what the agents before it did. Each agent keeps its own session (`park`), so going back resumes it, with a
//! catch-up of what was said meanwhile.

mod park;

pub(super) use park::resume_step;

use super::{Actor, Done, NoticeStyle};
use crate::LockExt;
use crate::agent::fallback::{self, Reason, carry, rest};
use crate::store::{BotConfig, Entry, EntryKind, Lane, now_ms};
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// Longest handoff a chat line carries for the apps to unfold.
const LINE_CARRIED: usize = 8_000;
/// Chat messages read for a recap or a catch-up.
const READ: i64 = 120;
/// Tool calls a takeover lists, the latest ones.
const TOOLS_LISTED: usize = 40;

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
    /// Agents (their `rest::account`) this turn already failed on.
    tried: Vec<String>,
    /// Agents (their `rest::account`) that lack their step's model: skipped until the bot's settings change.
    no_model: Vec<String>,
    /// Agents this turn left signed out, in order, whose sign-in cards are in the chat. When no agent can answer, the
    /// first one's card holds the turn's messages.
    signed_out: Vec<String>,
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
}

/// The bot as it runs on `current`'s agent with `base`'s settings, also when that agent is no longer in its chain.
pub(super) fn running(base: &BotConfig, current: &BotConfig) -> BotConfig {
    match fallback::step_of(base, &current.backend) {
        Some(step) => fallback::effective(base, step),
        None => BotConfig {
            backend: current.backend.clone(),
            command: current.command.clone(),
            model: current.model.clone(),
            model_options: current.model_options.clone(),
            ..base.clone()
        },
    }
}

impl Actor {
    /// The step of the agent the bot is on (its own: 0).
    fn step(&self) -> Option<usize> {
        fallback::step_of(&self.base, &self.cfg.backend)
    }

    /// The first step, in order, whose agent this turn hasn't failed on and that isn't resting. A signed-out agent
    /// rests too: the sign-in state an app last checked can be stale.
    fn next_step(&self, now: i64) -> Option<usize> {
        (0..fallback::steps(&self.base)).find(|&s| {
            let account = fallback::account(&self.base, s);
            !self.relay.tried.contains(&account)
                && !self.relay.no_model.contains(&account)
                && rest::get(&self.hub.store, &account, now).is_none()
        })
    }

    /// `start_turn`, once the turn has its lane and number: picks the agent. Not for hidden turns.
    pub(super) async fn relay_turn(&mut self, lane: &Lane, entries: &[String], text: &str) {
        if !std::mem::take(&mut self.relay.retrying) {
            self.relay.tried.clear();
            self.relay.failed.clear();
            self.relay.signed_out.clear();
            self.relay.stopped = None;
        }
        self.relay.exhausted = false;
        self.relay.line = None;
        self.relay.carried = None;
        self.relay.attempt = Some(Attempt { lane: lane.clone(), entries: entries.to_vec(), text: text.to_owned() });
        let now = now_ms();
        let target = self.next_step(now).unwrap_or(0);
        self.left_signed_out_before(target, now);
        let from = self.cfg.clone();
        if fallback::account(&self.base, target) != rest::account(&from) {
            self.switch_to(target).await;
            self.relay.line = self.tell_switch(&from, target);
        }
    }

    /// A turn (or its retry) starting on step `target`: the agents before it that rest because a bot found them
    /// signed out ask to be signed in here too.
    fn left_signed_out_before(&mut self, target: usize, now: i64) {
        let signed_out = |s: &usize| {
            rest::get(&self.hub.store, &fallback::account(&self.base, *s), now)
                .is_some_and(|r| r.reason == Reason::SignedOut)
        };
        let skipped: Vec<String> =
            (0..target).filter(signed_out).map(|s| fallback::backend(&self.base, s).to_owned()).collect();
        for agent in skipped {
            self.left_signed_out(&agent);
        }
    }

    /// `start_turn`, once the session is up (a new one, or the agent's own resumed): sets what the agent should hear
    /// first, and puts it on the chat line of the move. The main session gets a recap of the chat in a new session or
    /// a catch-up in its own; a retried turn gets what the agents before did. Not for hidden turns.
    pub(super) fn carry_in(&mut self) {
        let mut carried = Vec::new();
        if self.slot().is_none()
            && let Some((agent, session)) = self.relay.recap.take()
            && let Some(text) = self.recap(&agent, session.as_deref())
        {
            carried.push(text);
        }
        carried.extend(carry::takeover(&self.relay.failed));
        let text = (!carried.is_empty()).then(|| carried.join("\n\n"));
        if let Some(text) = &text
            && let Some(id) = self.relay.line.take()
            && let Some(mut line) = self.hub.store.entry(&id)
        {
            line.data["agentSwitch"]["carried"] = crate::agent::acp::truncate(text, LINE_CARRIED).into();
            self.hub.set_entry(&id, &line.data);
        }
        self.relay.carried = text;
    }

    /// The chat line for a move from the agent `from` ran on to step `target`'s (`carry_in` adds what the new agent
    /// was told).
    fn tell_switch(&self, from: &BotConfig, target: usize) -> Option<String> {
        let to = self.cfg.backend.clone();
        let from_account = rest::account(from);
        let from = from.backend.as_str();
        // Toward the bot's own agent, or away from one it no longer uses.
        let back = fallback::step_of(&self.base, from).is_none_or(|s| target < s);
        let (reason, until) = self
            .relay
            .stopped
            .or_else(|| rest::get(&self.hub.store, &from_account, now_ms()).map(|r| (r.reason, Some(r.until))))
            .unzip();
        let until = until.flatten();
        let text = if back {
            format!("Back on {}.", fallback::name(&to))
        } else {
            let why = reason.map_or("can't answer right now", Reason::phrase);
            let when = until
                .filter(|_| reason == Some(Reason::UsageLimit))
                .map(|t| format!(" (resets {})", carry::in_words(t - now_ms())))
                .unwrap_or_default();
            format!("{} {why}{when}. {} carried on.", fallback::name(from), fallback::name(&to))
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

    /// What the agent the main session just moved to should hear first: a catch-up when it resumed its own
    /// session, a recap of the conversation when it started a new one. Only the user's messages and the bot's
    /// replies count, never from before the user last started a new session.
    fn recap(&self, from: &str, from_session: Option<&str>) -> Option<String> {
        let bot = &self.base.id;
        let main = Lane::main(bot);
        let fresh: i64 = self.hub.store.kv_get(&park::fresh_key(bot)).and_then(|v| v.parse().ok()).unwrap_or(0);
        let current = self.relay.attempt.as_ref().map(|a| a.entries.as_slice()).unwrap_or_default();
        let messages = |after: i64| -> Option<Vec<Entry>> {
            let mut entries = self.hub.store.messages_after(&main, after.max(fresh), READ).ok()?;
            entries.retain(|e| !current.contains(&e.id));
            Some(entries)
        };
        if !self.session_fresh {
            let left = self.hub.store.kv_get(&park::left_key(bot, &self.cfg.backend)).and_then(|v| v.parse().ok());
            return carry::catch_up(&carry::said(&messages(left.unwrap_or(i64::MAX))?));
        }
        let entries = messages(0)?;
        let said = carry::said(&entries);
        if said.is_empty() {
            return None;
        }
        // The open plan, if it's as recent as the messages repeated.
        let oldest = entries.first().map_or(0, |e| e.seq);
        let summary = from_session.and_then(|s| self.hub.store.kv_get(&crate::chat::handoff::summary_key(bot, s)));
        let plan = self
            .hub
            .store
            .last_of_kind(&main, EntryKind::Plan, oldest.max(fresh))
            .map(|e| crate::chat::handoff::plan_text(&e.data));
        let bookmark = entries.last().map(|e| e.id.as_str());
        Some(carry::recap(&fallback::name(from), summary.as_deref(), &said, plan.as_deref(), bookmark))
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
            let (agent, account) = (self.cfg.backend.clone(), rest::account(&self.cfg));
            if reason == Reason::SignedOut {
                crate::agent::backends::set_signed_in(&agent, Some(false));
            }
            let resets = rest::reset_time(&self.hub.usage.locked(), &agent, now);
            let rest = rest::record(&self.hub.store, &account, reason, now, resets);
            if rest.is_some() {
                fallback::tell_bots(&self.hub, &account);
            }
            if reason == Reason::ModelMissing {
                self.relay.no_model.push(account.clone());
            }
            self.relay.tried.push(account);
            let failed = start_error.as_ref().unwrap_or(error);
            if self.next_step(now).is_none() {
                self.relay.exhausted = true;
                // No agent answers the turn's messages: they go out again once the first one left signed out is
                // signed in (the last agent's own card holds them only when there is none, `held_for_sign_in`).
                if let Some(first) = self.relay.signed_out.first().filter(|_| !self.turn_entries.is_empty()) {
                    self.sign_in_card(first, &self.turn_entries);
                }
                return match start_error {
                    Some(e) => {
                        self.start_failed(&e);
                        true
                    }
                    None => false,
                };
            }
            if reason == Reason::SignedOut {
                self.left_signed_out(&agent);
            }
            self.report_turn(false, Some(failed), "error");
            crate::analytics::agent_fell_back(&self.hub, &agent, reason);
            let done = self.so_far();
            self.relay.failed.push(carry::Failed { agent: fallback::name(&agent), reason, done });
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
    fn left_signed_out(&mut self, agent: &str) {
        if crate::agent::backends::harness(agent).is_some() && !self.relay.signed_out.iter().any(|a| a == agent) {
            self.relay.signed_out.push(agent.to_owned());
            self.sign_in_card(agent, &[]);
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
    }

    /// The failure notice: with a failure that moves on, a bot without fallbacks offers to add one, and a bot whose
    /// agents all failed this turn (`fall_back` ran out of them) says how each one is. It carries the turn's stop
    /// `reason` (see `stop_notice`).
    pub(super) fn failure_notice(&self, error: &anyhow::Error, text: &str, reason: &str) {
        let mut data = json!({"text": text, "style": NoticeStyle::Error, "stopReason": reason});
        match fallback::reason::of(error) {
            Some(Reason::UsageLimit | Reason::OutOfCredits | Reason::RateLimited | Reason::Overloaded)
                if self.base.fallbacks.is_empty() =>
            {
                data["addFallback"] = true.into();
            }
            Some(_) if self.relay.exhausted && !self.base.fallbacks.is_empty() => {
                data["text"] = format!("{text}\n\n{}", self.chain_note()).into();
            }
            _ => {}
        }
        self.line(data);
    }

    /// How each agent of the chain is: "No agent in this bot's list could answer: Claude Code reached its usage
    /// limit (resets in 2 h); Codex is rate limited (back in 4 min)."
    fn chain_note(&self) -> String {
        let now = now_ms();
        let parts: Vec<String> = (0..fallback::steps(&self.base))
            .map(|s| {
                let agent = fallback::backend(&self.base, s);
                match rest::get(&self.hub.store, &fallback::account(&self.base, s), now) {
                    Some(r) => {
                        let when = if r.reason == Reason::UsageLimit { "resets" } else { "back" };
                        let wait = carry::in_words(r.until - now);
                        format!("{} {} ({when} {wait})", fallback::name(agent), r.reason.phrase())
                    }
                    None => format!("{} couldn't answer", fallback::name(agent)),
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
        fallback::end_rest(&self.hub, &rest::account(&self.cfg));
    }

    /// `switchBack`: the bot's own agent again, now when idle, or for its next turn.
    pub(super) async fn switch_back(&mut self) {
        let own = fallback::account(&self.base, 0);
        fallback::end_rest(&self.hub, &own);
        if self.turn.is_none() && rest::account(&self.cfg) != own {
            let from = self.cfg.clone();
            self.idle_relay();
            self.switch_to(0).await;
            self.tell_switch(&from, 0);
        }
    }

    /// After a settings change: forgets parked sessions of agents whose step changed or left the chain, and moves an
    /// idle bot off an agent it no longer uses.
    pub(super) async fn settle_chain(&mut self, old: &BotConfig) {
        self.forget_changed(old);
        // A model the agent lacked may be one the bot no longer asks for.
        self.relay.no_model.clear();
        if self.step().is_none() && self.turn.is_none() {
            let from = self.cfg.clone();
            self.idle_relay();
            let target = self.next_step(now_ms()).unwrap_or(0);
            self.switch_to(target).await;
            self.tell_switch(&from, target);
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

    #[test]
    fn a_bot_keeps_running_an_agent_that_left_its_chain_until_it_moves_off() {
        let mut base: BotConfig =
            serde_json::from_value(json!({"id": "b", "name": "B", "backend": "claude", "model": "opus"})).expect("bot");
        let mut on_codex = base.clone();
        on_codex.backend = "codex".into();
        on_codex.model = Some("gpt-5.5".into());
        base.description = "new instructions".into();
        let cfg = running(&base, &on_codex);
        assert_eq!((cfg.backend.as_str(), cfg.model.as_deref()), ("codex", Some("gpt-5.5")));
        assert_eq!(cfg.description, "new instructions");
        assert_eq!(running(&base, &base).backend, "claude");
    }
}
