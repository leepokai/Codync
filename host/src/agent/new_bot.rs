//! A new bot's defaults, for a client's `createBot` and a bot's `create_bot` (`chat::manage`) alike.

use crate::hub::Hub;
use crate::market;
use crate::store::BotConfig;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::Arc;

/// The bot or group `b` asks for, with the defaults for what the caller left out.
pub async fn with_defaults(hub: &Arc<Hub>, mut b: Value) -> Result<BotConfig> {
    b["id"] = "".into();
    // A lead is made by the host or picked by the user (`chat::lead`), never on creation.
    b["lead"] = false.into();
    // Connectors are on for a new bot unless the client picked them.
    // Without a credential store (headless Linux) no connector can run, so the bot starts with none.
    if b["connectors"].is_null() && b["kind"] != "group" {
        b["connectors"] = match market::vault::unlock(hub.clone()).await {
            Ok(()) => json!(market::connector_ids(&hub.store)?),
            Err(e) => {
                tracing::warn!(
                    error = format!("{e:#}"),
                    "credential store unavailable; new bot starts without connectors"
                );
                json!([])
            }
        };
    }
    // Skills too: every installed one is on unless the client picked them, but a bot's skill stays with that bot.
    if b["skills"].is_null() && b["kind"] != "group" {
        b["skills"] = json!(market::Installed::Skills.ids(&hub.store)?);
    }
    // A bot works in its agent's Auto mode and asks about the rest, unless the client picked how it approves.
    if b["permission"].is_null() && b["mode"].is_null() && b["kind"] != "group" {
        b["mode"] = crate::agent::modes::AUTO.into();
    }
    // And the agent, when the client names none.
    if b["backend"].as_str().is_none_or(str::is_empty) && b["kind"] != "group" {
        let agents = tokio::task::spawn_blocking(super::backends::list).await?;
        b["backend"] = default_backend(&agents).into();
    }
    serde_json::from_value(b).context("invalid bot")
}

/// A new bot's agent (`agents` as `backends::list` gives them): Claude when it runs here, else the
/// first one Codync knows that does. A registry agent nobody chose is never the default.
fn default_backend(agents: &[Value]) -> &str {
    let runnable = || agents.iter().filter(|a| a["available"] == true);
    if runnable().any(|a| a["id"] == "claude") {
        return "claude";
    }
    runnable().find(|a| a["curated"] == true).and_then(|a| a["id"].as_str()).unwrap_or("claude")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_agent_is_claude_then_the_first_known_one() {
        assert_eq!(default_backend(&[]), "claude", "nothing installed");
        let mut agents = vec![
            json!({"id": "goose", "available": true, "curated": false}),
            json!({"id": "codex", "available": true, "curated": true}),
            json!({"id": "claude", "available": false, "curated": true}),
        ];
        assert_eq!(default_backend(&agents), "codex");
        agents[2]["available"] = true.into();
        assert_eq!(default_backend(&agents), "claude");
    }
}
