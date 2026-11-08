//! A flow link's own model and effort: switched on the recipient's session for its step and put
//! back on the next turn (docs/features/routines.md, "A step's model").

use super::Actor;
use super::fast_mode::Listing;
use crate::chat::team::ModelOverride;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::BTreeMap;

impl Actor {
    /// Switches the session to the link's model and options before the step's turn.
    pub(super) async fn apply_turn_model(&mut self, sid: &str, over: &ModelOverride) {
        if over.is_empty() {
            return;
        }
        match self.switch_model(sid, over.model.as_deref(), &over.options).await {
            Ok(()) => self.turn_model = Some(sid.to_owned()),
            Err(error) => {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't set a flow step's model");
            }
        }
    }

    /// Puts the bot's own model and options back on the session a step switched.
    pub(super) async fn restore_turn_model(&mut self) {
        let Some(sid) = self.turn_model.take() else { return };
        let (model, options) = (self.cfg.model.clone(), self.cfg.model_options.clone());
        if let Err(error) = self.switch_model(&sid, model.as_deref(), &options).await {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't restore the bot's model after a flow step");
        }
    }

    async fn switch_model(&mut self, sid: &str, model: Option<&str>, options: &BTreeMap<String, String>) -> Result<()> {
        let Some(listing) = self.session_options(sid) else { return Ok(()) };
        let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) else {
            return self.set_options(sid, options).await;
        };
        match crate::agent::auth::model_config(&listing).and_then(|c| c["id"].as_str()) {
            Some(config_id) => {
                let answer = self.set_option(sid, config_id, model).await?;
                self.note_options(sid, &answer, Listing::Switch);
            }
            // An agent with no model option takes ACP's own switch.
            None => {
                self.request_with_timeout("session/set_model", json!({"sessionId": sid, "modelId": model})).await?;
            }
        }
        self.set_options(sid, options).await
    }

    async fn set_options(&mut self, sid: &str, options: &BTreeMap<String, String>) -> Result<()> {
        for (id, value) in options {
            let answer = self.set_option(sid, id, value).await?;
            self.note_options(sid, &answer, Listing::Switch);
        }
        Ok(())
    }

    async fn set_option(&mut self, sid: &str, config_id: &str, value: &str) -> Result<Value> {
        let params = json!({"sessionId": sid, "configId": config_id, "value": value});
        self.request_with_timeout("session/set_config_option", params).await
    }

    /// The session's options as the agent last listed them, while the session is live.
    fn session_options(&self, sid: &str) -> Option<Value> {
        let conn = self.conn.as_ref()?;
        if !conn.loaded.contains_key(sid) {
            return None;
        }
        Some(json!({"configOptions": conn.config_options.get(sid).cloned().unwrap_or_else(|| json!([]))}))
    }
}
