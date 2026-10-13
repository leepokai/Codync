//! Usage analytics: the ^k toggle and `app_opened`. Sharing is on unless turned off.
//! The TUI only talks to the host over its loopback API, so it can always decide for this computer.

use serde_json::{Value, json};

use super::{After, App};

impl App {
    /// Every `hello`: keep the setting; the first one of this run also reports the launch.
    pub(super) fn analytics_hello(&mut self, v: &Value) {
        self.analytics = v["analytics"].as_bool();
        if self.opened {
            return;
        }
        self.opened = true;
        self.call("track", json!({"event": "app_opened"}), After::Tracked);
    }

    /// ^k → Share usage analytics: flips it.
    pub(super) fn toggle_analytics(&mut self) {
        if let Some(on) = self.analytics {
            self.call("setAnalytics", json!({"enabled": !on}), After::Analytics);
        }
    }

    pub(super) fn analytics_set(&mut self, v: &Value) {
        self.analytics = v["enabled"].as_bool();
        self.flash(if self.analytics == Some(true) {
            "Sharing usage analytics"
        } else {
            "Not sharing usage analytics"
        });
    }
}
