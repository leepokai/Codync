//! Agent sessions that run outside Codync: Claude Code, Gemini CLI or Antigravity in the person's own terminal.
//!
//! `codync-host hook` (see `cli::hook`) runs on every hook event of those agents and posts it to
//! `POST /ingest/hook`. Each session becomes a bot with the `external` backend: it has no agent process of its
//! own, but its status, tool calls and approval cards reach every client exactly like a bot's do. A permission
//! request holds the hook open until a person answers on any client, or the wait runs out and the terminal asks.
//!
//! The hook design follows the MIT-licensed Coucou (github.com/Louis-CFM/coucou): it never blocks the agent.

pub mod install;
pub mod normalize;
pub mod reply;

mod event;
mod permission;
mod session;
mod tool;

use crate::LockExt;
use crate::hub::Hub;
use crate::store::BotConfig;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

/// The `backend` of a bot standing for a terminal session.
pub const BACKEND: &str = "external";

/// Set in the environment of every agent Codync starts itself, so the hook ignores them.
pub const OWN_AGENT_ENV: &str = "CODYNC_BOT";

/// How long a permission card waits for a person. The hook itself waits a little longer, so the host is the
/// one to give up first and can say so on the card.
pub const DECISION_WAIT: Duration = Duration::from_secs(108);

/// Sessions idle this long are forgotten (a terminal closed without a `SessionEnd`).
const STALE_AFTER: Duration = Duration::from_hours(6);

pub fn is_external(cfg: &BotConfig) -> bool {
    cfg.backend == BACKEND
}

/// What a person chose for a permission request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

impl Decision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Default)]
pub struct External {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    sessions: HashMap<String, session::Session>,
    /// Approval card id → the hook request waiting for it.
    waiting: HashMap<String, oneshot::Sender<Option<Decision>>>,
    /// Sessions whose bot left the roster: not watched again until the terminal starts a new session.
    dismissed: std::collections::HashSet<String>,
}

/// Handles one hook envelope (`{agent, terminal, payload}`). The reply is `{"decision": "allow" | "deny" | null}`;
/// only a permission request ever gets a decision.
pub async fn ingest(hub: &Arc<Hub>, body: &Value) -> Value {
    let Some(event) = event::Event::parse(body) else { return json!({"decision": null}) };
    if event.name == "PermissionRequest" {
        return json!({"decision": permission::request(hub, &event).await.map(Decision::as_str)});
    }
    session::record(hub, &event);
    json!({"decision": null})
}

impl External {
    /// Answers the approval card `entry_id` if it belongs to a terminal session. `None` for `option_id` is a
    /// cancel: the terminal asks as usual. False when the card isn't one of ours.
    pub fn answer(&self, hub: &Hub, entry_id: &str, option_id: Option<&str>) -> bool {
        permission::answer(self, hub, entry_id, option_id)
    }

    /// The bot left the roster (or the host is shutting it down): its session is not watched again until the
    /// terminal starts a new one, and a hook waiting on one of its cards is told nobody decided.
    pub fn forget(&self, hub: &Hub, bot_id: &str) {
        let entries: Vec<String> = {
            let mut state = self.state.locked();
            let keys: Vec<String> =
                state.sessions.iter().filter(|(_, s)| s.bot == bot_id).map(|(key, _)| key.clone()).collect();
            for key in &keys {
                state.sessions.remove(key);
                state.dismissed.insert(key.clone());
            }
            state.waiting.keys().cloned().collect()
        };
        for entry in entries {
            if hub.store.entry(&entry).is_none_or(|e| e.bot_id != bot_id) {
                continue;
            }
            if let Some(waiting) = self.state.locked().waiting.remove(&entry) {
                let _ = waiting.send(None);
            }
        }
    }
}

#[cfg(test)]
mod tests;
