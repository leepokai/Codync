//! The palette's "Move active bots to the top": whether a bot with a new message moves to the top of
//! this computer's roster (`setAutoOrder`, kept by the host for every app).

use serde_json::{Value, json};

use super::{After, App};

impl App {
    /// Flips it; nothing before the `hello` says where it stands (or from a host without it).
    pub(super) fn toggle_auto_order(&mut self) {
        if let Some(on) = self.auto_order {
            self.call("setAutoOrder", json!({"enabled": !on}), After::AutoOrder);
        }
    }

    pub(super) fn auto_order_set(&mut self, v: &Value) {
        self.auto_order = v["enabled"].as_bool();
        self.flash(if self.auto_order == Some(true) {
            "Active bots move to the top"
        } else {
            "Bots keep the order you set"
        });
    }
}
