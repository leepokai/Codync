//! Keeps each live session in the bot's mode (`BotConfig::mode`, see `agent::modes`): set once per session, and
//! again after the user picks another (none: the mode the session opened in). The agent may leave it during the
//! session (Plan, once the plan is approved; Auto, on a model without it), and that stands until the next session.
//! Asked for Auto, an agent without it keeps its own mode and Codync asks about whatever it asks; a mode the agent
//! lacks or refuses fails the turn instead.

use super::Actor;
use super::fast_mode::Listing;
use crate::agent::modes::{Modes, Switch, switch};
use anyhow::{Context, Result, bail};
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
    /// refuses fails the turn: the bot never works in a mode the user didn't pick.
    pub(super) async fn sync_mode(&mut self, sid: &str) -> Result<()> {
        let Some(conn) = self.conn.as_mut() else { return Ok(()) };
        if conn.mode_set.contains(sid) {
            return Ok(());
        }
        let modes = conn.modes.get(sid);
        let (mode, config) = match switch(modes, self.cfg.mode.as_deref()) {
            Switch::Stay => {
                conn.mode_set.insert(sid.to_owned());
                return Ok(());
            }
            Switch::To(mode) => (mode.clone(), modes.and_then(|m| m.config_id.clone())),
            Switch::Missing => bail!(
                "{} doesn't have {}, the mode this bot works in, so this turn didn't start. Pick another under \
                 Permissions (Auto carries over to every agent that has one).",
                crate::agent::fallback::name(&self.cfg.backend),
                self.cfg.mode.as_deref().unwrap_or_default()
            ),
        };
        let (method, params) = match config {
            Some(config) => {
                ("session/set_config_option", json!({"sessionId": sid, "configId": config, "value": mode.id}))
            }
            None => ("session/set_mode", json!({"sessionId": sid, "modeId": mode.id})),
        };
        let answer = self.session_request(method, params).await.with_context(|| {
            format!("Couldn't switch the agent to {}, the mode this bot works in, so this turn didn't start", mode.name)
        })?;
        self.note_current_mode(sid, Some(&mode.id));
        self.note_options(sid, &answer, Listing::Update);
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
