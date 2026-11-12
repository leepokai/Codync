//! Keeps each live session in the bot's mode (`BotConfig::mode`, see `agent::modes`): set once per session, and
//! again after the user picks another (none: the mode the session opened in). The agent may leave it during the
//! session (Plan, once the plan is approved; Auto, on a model without it), and that stands until the next session.
//! Asked for Auto, an agent without it keeps its own mode and Codync asks about whatever it asks; a mode the agent
//! lacks or refuses fails the turn instead. A fallback step without it runs in its freest mode that is no freer than
//! the bot's (`Modes::stand_in`), with a notice; with none, or refused, the turn moves on to the bot's next agent.

use super::Actor;
use super::fast_mode::Listing;
use crate::agent::acp;
use crate::agent::modes::{Modes, Switch, switch};
use anyhow::{Result, bail};
use serde_json::{Value, json};

impl Actor {
    /// Records the session's modes from an agent answer or a `config_option_update`; the one it opened in stays.
    pub(super) fn note_modes(&mut self, sid: &str, options: &Value) {
        let Some(conn) = self.conn.as_mut() else { return };
        if let Some(mut modes) = Modes::from_session(options, &self.cfg.backend) {
            if let Some(known) = conn.modes.get(sid) {
                modes.opening.clone_from(&known.opening);
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
                if let Some(conn) = self.conn.as_mut() {
                    conn.mode_set.insert(sid.to_owned());
                }
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
        let (method, params) = match config {
            Some(config) => {
                ("session/set_config_option", json!({"sessionId": sid, "configId": config, "value": mode.id}))
            }
            None => ("session/set_mode", json!({"sessionId": sid, "modeId": mode.id})),
        };
        let answer = match self.session_request(method, params).await {
            Ok(answer) => answer,
            Err(e) => {
                let message = format!("Couldn't switch the agent to {}, the mode this bot works in", mode.name);
                // Refused on a fallback step: the next agent may have it.
                if on_fallback && crate::agent::fallback::reason::of(&e).is_none() {
                    return Err(acp::Failure::error(acp::FailureKind::ModeMissing, format!("{message}: {e:#}")));
                }
                return Err(e.context(format!("{message}, so this turn didn't start")));
            }
        };
        self.note_current_mode(sid, Some(&mode.id));
        self.note_options(sid, &answer, Listing::Update);
        if let Some(notice) = stand_in {
            self.notice(&notice, super::NoticeStyle::Info);
        }
        if let Some(conn) = self.conn.as_mut() {
            conn.mode_set.insert(sid.to_owned());
        }
        Ok(())
    }

    /// The user picked another mode: every live session switches before its next turn.
    pub(super) fn mode_changed(&mut self) {
        if let Some(conn) = self.conn.as_mut() {
            conn.mode_set.clear();
        }
    }
}
