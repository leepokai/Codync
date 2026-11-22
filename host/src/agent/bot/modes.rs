//! Keeps each live session in the bot's mode (`BotConfig::mode`, see `codync_agent::modes`): set once per session, and
//! again after the user picks another (none: the mode the session opened in). The agent may leave it during the
//! session (Plan, once the plan is approved; Auto, on a model without it), and that stands until the next session.
//! Asked for Auto, an agent without it keeps its own mode and Codync asks about whatever it asks; a mode the agent
//! lacks or refuses fails the turn instead. A fallback step without it runs in its freest mode that is no freer than
//! the bot's (`Modes::stand_in`), with a notice; with none, or refused, the turn moves on to the bot's next agent.
//! A turn a network rule holds to a ceiling runs in a mode no freer than it (`sync_mode_under`), and the bot's own
//! comes back before its next turn.

use super::Actor;
use super::fast_mode::Listing;
use anyhow::{Result, bail};
use codync_agent::acp;
use codync_agent::modes::{Ceiling, Mode, Modes, Switch, switch};
use serde_json::{Value, json};

/// The mode a network rule holds a turn to: `Allow` and `Deny` hold none (a denied sender gets no turn).
fn ceiling(access: codync_network::Access) -> Option<Ceiling> {
    match access {
        codync_network::Access::Ask => Some(Ceiling::Ask),
        codync_network::Access::Auto => Some(Ceiling::Auto),
        codync_network::Access::Allow | codync_network::Access::Deny => None,
    }
}

impl Actor {
    /// Records the session's modes from an agent answer or a `config_option_update`; the one it opened in stays.
    pub(super) fn note_modes(&mut self, sid: &str, options: &Value) {
        let Some(conn) = self.conn.as_mut() else { return };
        if let Some(mut modes) = Modes::from_session(options, &self.cfg.backend) {
            if let Some(known) = conn.modes.get(sid) {
                modes.opening.clone_from(&known.opening);
                modes.lowered_from.clone_from(&known.lowered_from);
            }
            conn.modes.insert(sid.to_owned(), modes);
        }
    }

    /// The session is in mode `id` now: a `current_mode_update`, or a switch the agent answered without its modes.
    pub(super) fn note_current_mode(&mut self, sid: &str, id: Option<&str>) {
        if let Some(modes) = self.conn.as_mut().and_then(|c| c.modes.get_mut(sid)) {
            modes.current = id.map(str::to_owned);
        }
    }

    /// Switches a live session to the bot's mode before its turn, once per session. A mode the agent lacks or
    /// refuses fails the turn: the bot never works in a mode the user didn't pick, nor more freely than one they did.
    /// A fallback step lacking it runs in its stand-in, with a notice; without one, or refused, it's a `ModeMissing`
    /// failure, so the turn moves on to the next agent.
    pub(super) async fn sync_mode(&mut self, sid: &str) -> Result<()> {
        if let Some(ceiling) = ceiling(self.hub.ceilings.access(&self.cfg.id)) {
            return self.sync_mode_under(sid, ceiling).await;
        }
        let Some(conn) = self.conn.as_ref() else { return Ok(()) };
        if conn.mode_set.contains(sid) {
            return Ok(());
        }
        let wanted = self.cfg.mode.clone();
        let on_fallback = self.step().is_some_and(|s| s > 0);
        let modes = conn.modes.get(sid);
        let config = modes.and_then(|m| m.config_id.clone());
        // The mode to switch to, and the notice a stand-in posts once the agent is in it.
        let (mode, stand_in) = match switch(modes, wanted.as_deref()) {
            Switch::Stay => {
                self.mode_synced(sid);
                return Ok(());
            }
            Switch::To(mode) => (mode.clone(), None),
            Switch::Missing => {
                let wanted = wanted.as_deref().unwrap_or_default();
                let name = crate::agent::fallback::name(&self.cfg.backend);
                let message = format!(
                    "{name} doesn't have {wanted}, the mode this bot works in, so this turn didn't start. Pick \
                     another under Permissions (Auto carries over to every agent that has one)."
                );
                if !on_fallback {
                    bail!(message);
                }
                let Some(mode) = modes.and_then(|m| m.stand_in(wanted)) else {
                    return Err(acp::Failure::error(acp::FailureKind::ModeMissing, message));
                };
                let notice = format!(
                    "{name} doesn't have {wanted}, the mode this bot works in; it runs this bot as {} instead.",
                    mode.name
                );
                (mode.clone(), Some(notice))
            }
        };
        if let Err(e) = self.set_session_mode(sid, config, &mode).await {
            let message = format!("Couldn't switch the agent to {}, the mode this bot works in", mode.name);
            // Refused on a fallback step: the next agent may have it.
            if on_fallback && crate::agent::fallback::reason::of(&e).is_none() {
                return Err(acp::Failure::error(acp::FailureKind::ModeMissing, format!("{message}: {e:#}")));
            }
            return Err(e.context(format!("{message}, so this turn didn't start")));
        }
        if let Some(notice) = stand_in {
            self.notice(&notice, super::NoticeStyle::Info);
        }
        self.mode_synced(sid);
        Ok(())
    }

    /// Asks the agent to put session `sid` in `mode` (through the `config` option where it has one) and records it.
    async fn set_session_mode(&mut self, sid: &str, config: Option<String>, mode: &Mode) -> Result<()> {
        let (method, params) = match config {
            Some(config) => {
                ("session/set_config_option", json!({"sessionId": sid, "configId": config, "value": mode.id}))
            }
            None => ("session/set_mode", json!({"sessionId": sid, "modeId": mode.id})),
        };
        let answer = self.session_request(method, params).await?;
        self.note_current_mode(sid, Some(&mode.id));
        self.note_options(sid, &answer, Listing::Update);
        Ok(())
    }

    /// Session `sid` is in the bot's mode: nothing to switch until the user picks another, or a ceiling lowers it.
    fn mode_synced(&mut self, sid: &str) {
        let Some(conn) = self.conn.as_mut() else { return };
        conn.mode_set.insert(sid.to_owned());
        if let Some(modes) = conn.modes.get_mut(sid) {
            modes.lowered_from = None;
        }
    }

    /// A turn under a ceiling runs in a mode no freer than it, whatever the bot's: the bot's Approvals only decide
    /// what Codync does with the requests that reach it, and a freer mode sends fewer. The bot's own mode comes back
    /// before its next turn. An agent with no such mode (or that lists none, so how freely it works can't be told)
    /// doesn't take the turn.
    async fn sync_mode_under(&mut self, sid: &str, ceiling: Ceiling) -> Result<()> {
        let wanted = self.cfg.mode.clone();
        let Some(conn) = self.conn.as_mut() else { return Ok(()) };
        conn.mode_set.remove(sid);
        let found = conn.modes.get_mut(sid).and_then(|m| Some((m.under(wanted.as_deref(), ceiling)?.clone(), m)));
        let Some((mode, modes)) = found else {
            let name = crate::agent::fallback::name(&self.cfg.backend);
            crate::analytics::network_failed(&self.hub, crate::analytics::NetworkStage::Ceiling);
            bail!(
                "{name} has no mode careful enough for what this computer's network rules allow the sender, so \
                 this bot didn't work on its message"
            );
        };
        if modes.current.as_deref() == Some(mode.id.as_str()) {
            return Ok(());
        }
        if modes.lowered_from.is_none() {
            modes.lowered_from.clone_from(&modes.current);
        }
        let config = modes.config_id.clone();
        self.set_session_mode(sid, config, &mode)
            .await
            .map_err(|e| e.context(format!("Couldn't switch the agent to {}, so this turn didn't start", mode.name)))
    }

    /// The user picked another mode: every live session switches before its next turn.
    pub(super) fn mode_changed(&mut self) {
        if let Some(conn) = self.conn.as_mut() {
            conn.mode_set.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codync_network::Access;

    #[test]
    fn a_network_rule_holds_a_turn_to_ask_or_auto() {
        assert_eq!(ceiling(Access::Ask), Some(Ceiling::Ask));
        assert_eq!(ceiling(Access::Auto), Some(Ceiling::Auto));
        assert_eq!(ceiling(Access::Allow), None);
        assert_eq!(ceiling(Access::Deny), None);
    }
}
