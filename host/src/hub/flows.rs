//! Telling clients which routine-flow canvases changed.

use super::Hub;
use crate::LockExt;
use crate::store::FlowChange;
use anyhow::Result;
use serde_json::{Value, json};

/// The event a client reloads a canvas on: the bots whose canvas changed, at `rev`.
pub fn flow_event(rev: i64, bots: &[String]) -> Value {
    json!({"type": "routineFlow", "rev": rev, "botIds": bots})
}

impl Hub {
    /// Saves a routine-flow change, which stamps its own rev, and broadcasts it in rev order.
    /// A save that changed nothing (`None`) broadcasts nothing.
    pub fn emit_flow_change<T>(&self, save: impl FnOnce() -> Result<(T, Option<FlowChange>)>) -> Result<T> {
        let _g = self.emit_lock.locked();
        let (value, change) = save()?;
        if let Some(change) = change {
            let _ = self.events.send(flow_event(change.rev, &change.bots));
        }
        Ok(value)
    }
}
