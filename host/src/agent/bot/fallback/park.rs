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

/// `login`: the step's login (`fallback::login`), an agent's id when it runs on the computer's own.
fn parked_key(bot: &str, login: &str) -> String {
    format!("fallback.session.{bot}.{login}")
}

/// The chat's last entry when the main session left the agent on `login`: what it saw.
pub(super) fn left_key(bot: &str, login: &str) -> String {
    format!("fallback.left.{bot}.{login}")
}

/// The chat's last entry when the user started a new session: no agent hears what came before it.
pub(super) fn fresh_key(bot: &str) -> String {
    format!("fallback.fresh.{bot}")
}

/// This bookkeeping failing costs a resume or a catch-up, not the turn: logged.
pub(super) fn logged(bot: &str, what: &str, result: anyhow::Result<()>) {
    if let Err(error) = result {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't {what}");
    }
}

/// Where the bot starts after a host restart: its active agent, its session id, and any handoff owed if the
/// saved fallback login could no longer run and the primary took over.
pub(in crate::agent::bot) struct Resumed {
    pub cfg: BotConfig,
    pub session_id: Option<String>,
    pub recap: Option<(String, Option<String>)>,
}

/// The bot as the agent its main session was on when the host stopped runs it, along with that agent's session id.
pub(in crate::agent::bot) fn resume_step(hub: &Hub, base: &BotConfig, saved_session: Option<String>) -> Resumed {
    let Some(saved_login) = hub.store.kv_get(&on_key(&base.id)) else {
        return Resumed { cfg: base.clone(), session_id: saved_session, recap: None };
    };
    if let Some(step) = fallback::step_of(&hub.store, base, &saved_login) {
        let cfg = fallback::effective(&hub.store, base, step);
        if step > 0 {
            hub.set_runtime(&base.id, |r| r.agent_step = Some(AgentStep::new(&cfg, step, None)));
        }
        return Resumed { cfg, session_id: saved_session, recap: None };
    }
    if let Some(cfg) = fallback::for_login(&hub.store, base, &saved_login) {
        hub.set_runtime(&base.id, |r| r.agent_step = Some(AgentStep::new(&cfg, 1, None)));
        return Resumed { cfg, session_id: saved_session, recap: None };
    }
    // The saved fallback login can no longer run (such as a deleted profile): keep its session id off `base` and
    // restore `base`'s own parked session instead, marking a catch-up or recap as owed.
    let key = parked_key(&base.id, &fallback::login::of(base));
    let parked = hub.store.kv_get(&key).filter(|s| !s.is_empty());
    logged(&base.id, "take the primary's parked session", hub.store.kv_delete(&key));
    logged(&base.id, "clear the missing fallback's agent mark", hub.store.kv_delete(&on_key(&base.id)));
    if let Err(error) = hub.store.set_session(&base.id, parked.as_deref()) {
        tracing::warn!(bot = %base.id, error = format!("{error:#}"), "couldn't restore the primary's session");
    }
    let recap = Some((fallback::login::backend_of(&saved_login).to_owned(), saved_session));
    Resumed { cfg: base.clone(), session_id: parked, recap }
}

impl Actor {
    /// Moves the main session to `target`'s agent: parks this agent's session and resumes the other's.
    pub(super) async fn switch_to(&mut self, target: BotConfig) {
        let bot = self.base.id.clone();
        let from_login = fallback::login::of(&self.cfg);
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
        let marked = if ran { store.kv_set(&left_key(&bot, &from_login), &left.to_string()) } else { Ok(()) };
        let saved = marked.and_then(|()| match &self.session_id {
            Some(sid) => store.kv_set(&parked_key(&bot, &from_login), sid),
            None => store.kv_delete(&parked_key(&bot, &from_login)),
        });
        logged(&bot, "park the agent's session", saved);
        if let Some(c) = self.conn.take() {
            c.acp.kill().await;
        }
        // A routine run's session belongs to the agent it ran on.
        for slot in self.run_slots() {
            self.set_session(Some(&slot), None);
        }
        self.owe_recap();
        self.cfg = target;
        let key = parked_key(&bot, &fallback::login::of(&self.cfg));
        let parked = self.hub.store.kv_get(&key).filter(|s| !s.is_empty());
        logged(&bot, "take the agent's parked session", self.hub.store.kv_delete(&key));
        self.set_session(None, parked.as_deref());
        self.mark_step();
    }

    /// Records which agent the bot is on, for a host restart and the apps.
    pub(super) fn mark_step(&self) {
        let bot = self.base.id.clone();
        let own = fallback::login(&self.hub.store, &self.base, 0);
        let current = fallback::login::of(&self.cfg);
        let on_own = current == own;
        let stored = if on_own {
            self.hub.store.kv_delete(&on_key(&bot))
        } else {
            self.hub.store.kv_set(&on_key(&bot), &current)
        };
        logged(&bot, "record the bot's agent", stored);
        // Still on the same agent and model (a settings change elsewhere): it's been on it since it moved.
        let same = self.hub.runtime(&bot).agent_step.filter(|s| {
            s.agent == self.cfg.backend && s.login_profile == self.cfg.login_profile && s.model == self.cfg.model
        });
        let reason = self.relay.stopped.map(|(r, _)| r);
        let on = (!on_own).then(|| {
            let step = self.step().unwrap_or(1);
            same.unwrap_or_else(|| AgentStep::new(&self.cfg, step, reason))
        });
        self.hub.set_runtime(&bot, |r| r.agent_step = on);
    }

    /// After a settings change (`old` before it): forgets the parked sessions of agents whose step changed or left the
    /// chain.
    pub(super) fn forget_changed(&self, old: &BotConfig) {
        let store = &self.hub.store;
        for step in 0..fallback::steps(store, old) {
            let login = fallback::login(store, old, step);
            let same = fallback::step_of(store, &self.base, &login).is_some_and(|s| {
                let (a, b) = (fallback::effective(store, old, step), fallback::effective(store, &self.base, s));
                (a.command, a.model, a.model_options) == (b.command, b.model, b.model_options)
            });
            if !same && login != fallback::login::of(&self.cfg) {
                let forgot = store.kv_delete(&parked_key(&self.base.id, &login));
                logged(&self.base.id, "forget a parked session", forgot);
            }
        }
    }

    /// `newSession`: every agent's conversation starts over, not only the current one's, and none is told what was
    /// said before.
    pub(in crate::agent::bot) fn forget_parked(&mut self) {
        let bot = self.base.id.clone();
        for key in self.hub.store.kv_prefix(&format!("fallback.session.{bot}.")) {
            let forgot = self.hub.store.kv_delete(&key);
            logged(&bot, "forget a parked session", forgot);
        }
        self.relay.recap = None;
        self.handed_off(false);
        logged(
            &bot,
            "mark the new session",
            self.hub.store.kv_set(&fresh_key(&bot), &self.hub.store.last_seq(&bot).to_string()),
        );
    }
}
