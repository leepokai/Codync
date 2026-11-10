//! Where each agent's conversation waits while another one answers. Moving to another agent parks the main session
//! under the old agent (`kv` `fallback.session.<bot>.<agent>`) and resumes the new agent's own, so going back picks
//! up where it was. `bots.session_id` is always the session of the agent the bot is on; `fallback.on.<bot>` names
//! that agent for a host restart.

use super::super::Actor;
use crate::agent::fallback;
use crate::hub::{AgentStep, Hub};
use crate::store::BotConfig;

fn on_key(bot: &str) -> String {
    format!("fallback.on.{bot}")
}

fn parked_key(bot: &str, backend: &str) -> String {
    format!("fallback.session.{bot}.{backend}")
}

/// The chat's last entry when the main session left agent `backend`: what it saw.
pub(super) fn left_key(bot: &str, backend: &str) -> String {
    format!("fallback.left.{bot}.{backend}")
}

/// The chat's last entry when the user started a new session: no agent hears what came before it.
pub(super) fn fresh_key(bot: &str) -> String {
    format!("fallback.fresh.{bot}")
}

/// This bookkeeping failing costs a resume or a catch-up, not the turn: logged.
fn logged(bot: &str, what: &str, result: anyhow::Result<()>) {
    if let Err(error) = result {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't {what}");
    }
}

/// The bot as the agent its main session was on when the host stopped runs it.
pub(in crate::agent::bot) fn resume_step(hub: &Hub, base: &BotConfig) -> BotConfig {
    let step = hub.store.kv_get(&on_key(&base.id)).and_then(|b| fallback::step_of(base, &b)).unwrap_or(0);
    let cfg = fallback::effective(base, step);
    if step > 0 {
        hub.set_runtime(&base.id, |r| r.agent_step = Some(AgentStep::new(&cfg, step, None)));
    }
    cfg
}

impl Actor {
    /// Moves the main session to step `target`'s agent: parks this agent's session and resumes the other's.
    pub(super) async fn switch_to(&mut self, target: usize) {
        let bot = self.base.id.clone();
        let from = self.cfg.backend.clone();
        // What this agent has seen of the chat: not a message that starts a turn it won't answer.
        let unseen = self
            .relay
            .failed
            .is_empty()
            .then_some(self.relay.attempt.as_ref())
            .flatten()
            .and_then(|a| a.entries.iter().filter_map(|id| self.hub.store.entry(id)).map(|e| e.seq).min());
        let left = unseen.map_or_else(|| self.hub.store.last_seq(&bot), |seq| seq - 1);
        // An agent whose main session never came up since the last move saw nothing new, and the recap still owed
        // is of the agent before it.
        let ran = self.relay.recap.is_none();
        let store = &self.hub.store;
        let marked = if ran { store.kv_set(&left_key(&bot, &from), &left.to_string()) } else { Ok(()) };
        let saved = marked.and_then(|()| match &self.session_id {
            Some(sid) => store.kv_set(&parked_key(&bot, &from), sid),
            None => store.kv_delete(&parked_key(&bot, &from)),
        });
        logged(&bot, "park the agent's session", saved);
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
        // A routine run's session belongs to the agent it ran on.
        for slot in self.run_slots() {
            self.set_session(Some(&slot), None);
        }
        let left_session = self.session_id.clone();
        self.cfg = fallback::effective(&self.base, target);
        let key = parked_key(&bot, &self.cfg.backend);
        let parked = self.hub.store.kv_get(&key).filter(|s| !s.is_empty());
        logged(&bot, "take the agent's parked session", self.hub.store.kv_delete(&key));
        self.set_session(None, parked.as_deref());
        if ran {
            self.relay.recap = Some((from, left_session));
        }
        self.mark_step();
    }

    /// Records which agent the bot is on, for a host restart and the apps.
    pub(super) fn mark_step(&self) {
        let (bot, step) = (self.base.id.clone(), self.step().unwrap_or(0));
        let stored = if step == 0 {
            self.hub.store.kv_delete(&on_key(&bot))
        } else {
            self.hub.store.kv_set(&on_key(&bot), &self.cfg.backend)
        };
        logged(&bot, "record the bot's agent", stored);
        // Still on the same agent and model (a settings change elsewhere): it's been on it since it moved.
        let same =
            self.hub.runtime(&bot).agent_step.filter(|s| s.agent == self.cfg.backend && s.model == self.cfg.model);
        let reason = self.relay.stopped.map(|(r, _)| r);
        let on = (step > 0).then(|| same.unwrap_or_else(|| AgentStep::new(&self.cfg, step, reason)));
        self.hub.set_runtime(&bot, |r| r.agent_step = on);
    }

    /// After a settings change (`old` before it): forgets the parked sessions of agents whose step changed or left the
    /// chain.
    pub(super) fn forget_changed(&self, old: &BotConfig) {
        for step in 0..fallback::steps(old) {
            let agent = fallback::backend(old, step);
            let same = fallback::step_of(&self.base, agent).is_some_and(|s| {
                let (a, b) = (fallback::effective(old, step), fallback::effective(&self.base, s));
                (a.command, a.model, a.model_options) == (b.command, b.model, b.model_options)
            });
            if !same && agent != self.cfg.backend {
                let forgot = self.hub.store.kv_delete(&parked_key(&self.base.id, agent));
                logged(&self.base.id, "forget a parked session", forgot);
            }
        }
    }

    /// `newSession`: every agent's conversation starts over, not only the current one's, and none is told what was
    /// said before.
    pub(in crate::agent::bot) fn forget_parked(&mut self) {
        let bot = self.base.id.clone();
        for step in 0..fallback::steps(&self.base) {
            let forgot = self.hub.store.kv_delete(&parked_key(&bot, fallback::backend(&self.base, step)));
            logged(&bot, "forget a parked session", forgot);
        }
        self.relay.recap = None;
        logged(
            &bot,
            "mark the new session",
            self.hub.store.kv_set(&fresh_key(&bot), &self.hub.store.last_seq(&bot).to_string()),
        );
    }
}
