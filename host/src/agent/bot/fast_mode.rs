//! Fast mode: keeps each live session's option where the bot wants it, and tells clients what
//! the agent offers (docs/features/marketplace.md, "Fast mode").

use super::{Actor, NoticeStyle};
use crate::agent::config::{FastModeSupport, fast_mode_change, fast_mode_config, fast_mode_on, fast_mode_refusal};
use serde_json::{Value, json};

/// Where a session's config options come from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Listing {
    /// `session/new` or `session/load`.
    Session,
    /// The answer to our own Fast mode switch.
    Switch,
    /// A `config_option_update`, or the answer to a mode switch.
    Update,
}

impl Actor {
    /// Records a session's options from an agent answer or a `config_option_update`: its modes, and Fast mode.
    pub(super) fn note_options(&mut self, sid: &str, options: &Value, listing: Listing) {
        self.note_modes(sid, options);
        if !options["configOptions"].is_array() {
            // A session without config options has no Fast mode to offer; other answers that
            // leave them out say nothing, and neither overrides what the agent listed before.
            if listing == Listing::Session && self.fast_support == FastModeSupport::Unknown {
                self.publish_fast_support(FastModeSupport::Unsupported);
            }
            return;
        }
        let Some(conn) = self.conn.as_mut() else { return };
        let option = fast_mode_config(options).cloned();
        let was_on = conn.fast_options.get(sid).is_some_and(fast_mode_on);
        let support = match &option {
            None => FastModeSupport::Unsupported,
            // Turned off right after it was on, or despite our asking: the agent refused it.
            Some(o) if self.cfg.fast_mode && !fast_mode_on(o) && (listing == Listing::Switch || was_on) => {
                FastModeSupport::Refused(fast_mode_refusal(o))
            }
            Some(_) => FastModeSupport::Offered,
        };
        match option {
            Some(o) => conn.fast_options.insert(sid.to_owned(), o),
            None => conn.fast_options.remove(sid),
        };
        self.set_fast_support(support);
    }

    /// Switches a live session's Fast mode to the bot's setting before a turn. Once the agent
    /// refused it, asks again only after the user changes the setting.
    pub(super) async fn sync_fast_mode(&mut self, sid: &str) {
        let on = self.cfg.fast_mode;
        if on && matches!(self.fast_support, FastModeSupport::Refused(_)) {
            return;
        }
        let Some(option) = self.conn.as_ref().and_then(|c| c.fast_options.get(sid)) else { return };
        let (Some(config_id), Some(value)) = (option["id"].as_str(), fast_mode_change(option, on)) else { return };
        let params = json!({"sessionId": sid, "configId": config_id, "value": value});
        match self.session_request("session/set_config_option", params).await {
            Ok(answer) => self.note_options(sid, &answer, Listing::Switch),
            Err(error) => {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't set Fast mode");
                self.notice(&format!("Couldn't change Fast mode: {error:#}"), NoticeStyle::Info);
            }
        }
    }

    /// The user changed the setting: a refusal no longer stands, so the next turn asks again.
    pub(super) fn fast_mode_changed(&mut self) {
        if matches!(self.fast_support, FastModeSupport::Refused(_)) {
            self.publish_fast_support(FastModeSupport::Offered);
        }
    }

    fn set_fast_support(&mut self, support: FastModeSupport) {
        // A refusal stands until the user changes the setting, whatever other sessions list.
        let support = match (&self.fast_support, support) {
            (FastModeSupport::Refused(why), FastModeSupport::Offered) => FastModeSupport::Refused(why.clone()),
            (_, support) => support,
        };
        self.publish_fast_support(support);
    }

    fn publish_fast_support(&mut self, support: FastModeSupport) {
        if support == self.fast_support {
            return;
        }
        if self.cfg.fast_mode && support == FastModeSupport::Unsupported {
            self.notice("Fast mode isn't available for this agent or model.", NoticeStyle::Info);
        }
        if support.stored() != self.fast_support.stored()
            && let Some(stored) = support.stored()
            && let Err(error) = self.hub.store.kv_set(&stored_key(&self.cfg.id), stored)
        {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store Fast mode support");
        }
        self.fast_support = support.clone();
        self.hub.set_runtime(&self.cfg.id, |r| r.fast_mode = support);
    }
}

/// kv key for what the bot's agent last said about Fast mode.
pub(super) fn stored_key(bot_id: &str) -> String {
    format!("fast-mode.{bot_id}")
}
