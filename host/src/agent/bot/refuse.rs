//! Refusing an agent's permission request without a card: by the user's `deny` rule or because the command is
//! forbidden for every bot (`agent::blocklist`).

use super::Actor;
use crate::store::EntryKind;
use serde_json::{Value, json};

impl Actor {
    /// Answers permission request `rpc_id` with the agent's reject option (a cancel when it has none) and says
    /// why in the chat.
    pub(super) async fn refuse(&mut self, rpc_id: Value, options: &[Value], why: &str) {
        let notice = json!({"text": why, "style": "info"});
        self.hub.add_entry(&self.lane, EntryKind::Notice, self.turn.unwrap_or(0), &notice);
        if !self.pick_option(&rpc_id, options, &["reject_once", "reject_always"], false).await
            && let Some(conn) = &self.conn
        {
            let _ = conn.acp.respond(rpc_id, json!({"outcome": {"outcome": "cancelled"}})).await;
        }
    }
}
