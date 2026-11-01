//! A change to the bot's model options (effort, fast…) on the same model reaches its live sessions before their next
//! turn, keeping the agent's context, the way Fast mode does. Back to the agent's default restarts the agent instead:
//! a session can't be told to forget a value, and a fresh one opens on the agent's own.

use super::Actor;
use super::fast_mode::Listing;
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::BTreeMap;

/// The bot's options moved from `old` to `new` without dropping one, so live sessions can switch.
pub(super) fn switch_live(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> bool {
    old != new && old.keys().all(|k| new.contains_key(k))
}

impl Actor {
    /// Sets the bot's model options on a session opened before they changed. A value the agent refuses fails the
    /// turn, and the next one asks again: the bot never works on settings the user didn't pick.
    pub(super) async fn sync_model_options(&mut self, sid: &str) -> Result<()> {
        if !self.conn.as_ref().is_some_and(|c| c.stale_options.contains(sid)) {
            return Ok(());
        }
        for (id, value) in self.cfg.model_options.clone() {
            let params = json!({"sessionId": sid, "configId": id, "value": value});
            let answer = self
                .session_request("session/set_config_option", params)
                .await
                .with_context(|| format!("Couldn't set {id} to {value}, so this turn didn't start"))?;
            self.note_options(sid, &answer, Listing::Update);
        }
        if let Some(conn) = self.conn.as_mut() {
            conn.stale_options.remove(sid);
        }
        Ok(())
    }

    /// The user changed the options: every live session switches before its next turn.
    pub(super) fn model_options_changed(&mut self) {
        if let Some(conn) = self.conn.as_mut() {
            conn.stale_options = conn.loaded.keys().cloned().collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn a_new_or_changed_value_switches_live_and_a_reset_restarts() {
        assert!(switch_live(&options(&[]), &options(&[("effort", "high")])));
        assert!(switch_live(&options(&[("effort", "low")]), &options(&[("effort", "high"), ("fast", "true")])));
        assert!(!switch_live(&options(&[("effort", "low")]), &options(&[])), "back to the agent's default");
        assert!(!switch_live(&options(&[("effort", "low")]), &options(&[("effort", "low")])), "nothing changed");
    }
}
