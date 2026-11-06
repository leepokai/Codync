//! Subscription usage, read on the host.
//!
//! Claude: `claude -p /usage`, the status line, and the ACP rate-limit event.
//! Codex: the CLI's live account rate limits. The other agents Codync runs are
//! read the same way when they can report locally: `omp usage`, `amp usage`,
//! `kiro-cli /usage`, a `kimi` screen, Antigravity's language server, OpenCode's
//! database, Vibe's session logs, and the `JetBrains` IDE quota file.
//!
//! Cursor has no local report. The host reads the token the Cursor app saved
//! and asks `cursor.com/api/usage-summary`. That is the only provider call, and
//! the token is never logged.

mod amp;
mod antigravity;
mod claude;
mod codex;
mod command;
mod cursor;
mod junie;
mod kimi;
mod kiro;
mod omp;
mod opencode;
mod vibe;

pub use claude::{ingest_claude_rate_limit, ingest_statusline};

use crate::LockExt;
use crate::hub::Hub;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

const POLL_EVERY: Duration = Duration::from_mins(5);
/// Usage of a plan above which background work (dreaming, the lead's check-ins) waits.
const HEADROOM_PERCENT: f64 = 75.0;

/// Usage of harness `backend`'s plan above [`HEADROOM_PERCENT`] in any window.
/// A note (today's tokens, a dollar balance) is not a plan window.
pub fn near_limit(hub: &Hub, backend: &str) -> bool {
    hub.usage.locked().providers.iter().filter(|p| p.id == backend).any(|p| over_headroom(&p.windows))
}

fn over_headroom(windows: &[Window]) -> bool {
    windows.iter().any(|w| w.text.is_none() && w.percent > HEADROOM_PERCENT)
}

/// Everything the apps show about limits (wire shape of the `usage` event).
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub providers: Vec<Provider>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub windows: Vec<Window>,
    /// Where the numbers came from: `claude` (CLI), `statusline`, `agent`, `sessions`,
    /// or a short phrase for the other agents.
    pub source: &'static str,
    pub updated_at: i64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub id: String,
    pub label: String,
    /// Share of the cap already used, 0–100. Zero when [`Self::text`] is set.
    pub percent: f64,
    /// Epoch milliseconds, when known.
    pub resets_at: Option<i64>,
    /// Human text when only that is known ("Sep 26 at 12pm (Asia/Taipei)").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_text: Option<String>,
    /// A reading that isn't a share of a cap ("$4.20", "1.2M tokens", "Unlimited").
    /// Clients show this instead of the percent, and it never counts as the tightest limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Window {
    fn new(id: impl Into<String>, label: impl Into<String>, percent: f64, resets_at: Option<i64>) -> Self {
        Self { id: id.into(), label: label.into(), percent, resets_at, resets_text: None, text: None }
    }

    /// A row with no bar: today's tokens, an uncapped balance, an unlimited plan.
    fn note(id: impl Into<String>, label: impl Into<String>, text: impl Into<String>) -> Self {
        Self { text: Some(text.into()), ..Self::new(id, label, 0.0, None) }
    }
}

impl Usage {
    /// One window at a time, for Claude's status line and rate-limit events.
    /// The provider's other windows stay.
    fn merge(&mut self, id: &'static str, name: &'static str, source: &'static str, new: Vec<Window>) {
        let mut windows = self.providers.iter().find(|p| p.id == id).map(|p| p.windows.clone()).unwrap_or_default();
        windows.retain(|w| !new.iter().any(|n| n.id == w.id));
        windows.extend(new);
        windows.sort_by(|a, b| a.id.cmp(&b.id));
        self.providers.retain(|p| p.id != id);
        self.providers.push(Provider { id, name, windows, source, updated_at: crate::store::now_ms() });
        self.providers.sort_by_key(|p| p.id);
    }

    /// A probe's full reading. Windows it no longer reports are dropped, and an
    /// empty reading removes the provider.
    fn replace(&mut self, id: &'static str, name: &'static str, source: &'static str, new: Vec<Window>) {
        self.providers.retain(|p| p.id != id);
        if new.is_empty() {
            return;
        }
        let mut windows = new;
        windows.sort_by(|a, b| a.id.cmp(&b.id));
        self.providers.push(Provider { id, name, windows, source, updated_at: crate::store::now_ms() });
        self.providers.sort_by_key(|p| p.id);
    }
}

pub(super) fn merge(hub: &Hub, id: &'static str, name: &'static str, source: &'static str, windows: Vec<Window>) {
    let mut usage = hub.usage.locked().clone();
    usage.merge(id, name, source, windows);
    hub.set_usage(usage);
}

pub async fn poll(hub: Arc<Hub>) {
    loop {
        refresh(&hub).await;
        tokio::time::sleep(POLL_EVERY).await;
    }
}

pub async fn refresh(hub: &Hub) {
    let (
        codex,
        claude,
        omp_windows,
        amp_windows,
        kiro_windows,
        kimi_windows,
        antigravity,
        opencode,
        vibe_notes,
        junie,
        cursor_windows,
    ) = tokio::join!(
        codex::collect(),
        claude::collect(),
        omp::collect(),
        amp::collect(),
        kiro::collect(),
        tokio::task::spawn_blocking(kimi::collect),
        antigravity::collect(),
        tokio::task::spawn_blocking(opencode::collect),
        tokio::task::spawn_blocking(vibe::collect),
        tokio::task::spawn_blocking(junie::collect),
        cursor::collect(),
    );
    apply(hub, codex, "codex", "Codex", "Codex CLI");
    apply(hub, claude, "claude", "Claude", "claude");
    apply(hub, omp_windows, "omp", "oh-my-pi", "omp usage");
    apply(hub, amp_windows, "amp", "Amp", "amp usage");
    apply(hub, kiro_windows, "kiro", "Kiro CLI", "kiro-cli");
    apply(hub, joined(kimi_windows), "kimi", "Kimi Code", "kimi");
    apply(hub, antigravity, "antigravity", "Google Antigravity", "antigravity");
    apply(hub, joined(opencode), "opencode", "OpenCode", "opencode database");
    apply(hub, joined(vibe_notes), "vibe", "Mistral Vibe", "vibe logs");
    apply(hub, joined(junie), "junie", "Junie", "JetBrains IDE");
    apply(hub, cursor_windows, "cursor", "Cursor", "Cursor");
}

fn joined(result: Result<Option<Vec<Window>>, tokio::task::JoinError>) -> Option<Vec<Window>> {
    result.ok().flatten()
}

pub(super) fn finite_i64(n: f64) -> Option<i64> {
    if !n.is_finite() {
        return None;
    }
    let rounded = n.round();
    #[allow(clippy::cast_precision_loss)]
    let (min, max) = (i64::MIN as f64, i64::MAX as f64);
    if !(min..=max).contains(&rounded) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(rounded as i64)
}

fn apply(hub: &Hub, windows: Option<Vec<Window>>, id: &'static str, name: &'static str, source: &'static str) {
    let Some(windows) = windows else { return };
    let mut usage = hub.usage.locked().clone();
    usage.replace(id, name, source, windows);
    hub.set_usage(usage);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn merge_replaces_only_matching_windows() {
        let mut u = Usage::default();
        u.merge(
            "claude",
            "Claude",
            "claude",
            vec![Window::new("five_hour", "5-hour", 10.0, None), Window::new("seven_day", "Weekly", 50.0, None)],
        );
        u.merge("claude", "Claude", "statusline", vec![Window::new("five_hour", "5-hour", 12.0, None)]);
        let c = &u.providers[0];
        assert_eq!((c.windows.len(), c.source), (2, "statusline"));
        assert!((c.windows[0].percent - 12.0).abs() < f64::EPSILON, "five_hour updated");
        assert_eq!(serde_json::to_value(&u).unwrap()["providers"][0]["windows"][1]["resetsAt"], Value::Null);
    }

    #[test]
    fn a_full_reading_drops_windows_the_probe_no_longer_reports() {
        let mut u = Usage::default();
        u.replace(
            "cursor",
            "Cursor",
            "Cursor",
            vec![Window::new("monthly", "Monthly", 40.0, None), Window::note("on_demand", "On-Demand", "$1")],
        );
        u.replace("cursor", "Cursor", "Cursor", vec![Window::note("unlimited", "Monthly", "Unlimited")]);
        assert_eq!(u.providers[0].windows.len(), 1);
        assert_eq!(u.providers[0].windows[0].id, "unlimited");
        u.replace("cursor", "Cursor", "Cursor", Vec::new());
        assert_eq!(u.providers, []);
    }

    #[test]
    fn a_note_is_not_serialized_as_a_missing_percent() {
        let window = Window::note("cost", "Cost today", "$1.50");
        let value = serde_json::to_value(&window).unwrap();
        assert_eq!(value["text"], "$1.50");
        assert_eq!(value["percent"], 0.0);
        assert!(value.get("resetsText").is_none());
    }

    #[test]
    fn a_note_does_not_count_as_the_plan_running_out() {
        let note = Window { percent: 99.0, ..Window::note("cost", "Cost today", "$9") };
        assert!(!over_headroom(&[note]));
        assert!(over_headroom(&[Window::new("five_hour", "5-hour", 80.0, None)]));
    }
}
