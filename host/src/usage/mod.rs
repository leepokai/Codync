//! Subscription usage, read on the host.
//!
//! Claude: `claude -p /usage`, the status line, and the ACP rate-limit event.
//! Codex: the CLI's live account rate limits. The other agents Codync runs are
//! read the same way when they can report locally: `omp usage`, `amp usage`,
//! `hermes usage`, `auggie account status`, `kilo profile`, `kiro-cli /usage`, a `kimi`
//! screen, Antigravity's ACP `/usage` command or its IDE's language server, Copilot's
//! headless server, Grok's ACP billing extension, OpenCode's database, Vibe's session logs,
//! and the `JetBrains` IDE quota file. Which of them get read and shown: [`services`].
//! Failures: [`reading`].
//!
//! Cursor's saved CLI or app login reads the dashboard service's weekly unified
//! allowance, or the website's usage summary for legacy monthly plans. These are
//! the only provider requests, and the token is never logged.

mod acp_read;
mod amp;
mod antigravity;
mod auggie;
mod claude;
mod codex;
mod command;
mod copilot;
mod cursor;
mod grok;
mod hermes;
mod junie;
mod kilo;
mod kimi;
mod kiro;
mod omp;
mod opencode;
mod reading;
mod services;
mod vibe;

pub use claude::{ingest_claude_rate_limit, ingest_statusline};
pub(crate) use command::strip_ansi;
pub use services::{bots_changed, set_order, set_shown};

use crate::LockExt;
use crate::hub::Hub;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::Serialize;
use services::{Reader, Service};
use std::sync::Arc;
use std::time::Duration;

const POLL_EVERY: Duration = Duration::from_mins(5);
/// How long a forced refresh holds up whoever asked. A slower reader lands through `usage` events.
const REFRESH_WAIT: Duration = Duration::from_mins(1);
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
    /// The agents the user can choose among in the Usage screen.
    pub services: Vec<Service>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    /// The agent's ACP registry id, which its logo goes by. Without one, its `id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<&'static str>,
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
    /// What the apps get: without the readings of the agents the user doesn't show.
    pub fn visible(&self) -> Self {
        let hidden = |id: &str| self.services.iter().any(|service| service.id == id && !service.shown);
        let rank = |id: &str| self.services.iter().position(|service| service.id == id).unwrap_or(usize::MAX);
        let mut providers: Vec<Provider> =
            self.providers.iter().filter(|provider| !hidden(provider.id)).cloned().collect();
        providers.sort_by_key(|provider| rank(provider.id));
        Self { providers, services: self.services.clone() }
    }

    /// One window at a time, for Claude's status line and rate-limit events.
    /// The provider's other windows stay.
    fn merge(&mut self, id: &'static str, name: &'static str, source: &'static str, new: Vec<Window>) {
        let mut windows = self.providers.iter().find(|p| p.id == id).map(|p| p.windows.clone()).unwrap_or_default();
        windows.retain(|w| !new.iter().any(|n| n.id == w.id));
        windows.extend(new);
        windows.sort_by(|a, b| a.id.cmp(&b.id));
        self.providers.retain(|p| p.id != id);
        self.providers.push(Provider::new(id, name, source, windows));
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
        self.providers.push(Provider::new(id, name, source, windows));
        self.providers.sort_by_key(|p| p.id);
    }
}

impl Provider {
    fn new(id: &'static str, name: &'static str, source: &'static str, windows: Vec<Window>) -> Self {
        Self { id, name, registry: services::registry(id), windows, source, updated_at: crate::store::now_ms() }
    }
}

pub(super) fn merge(hub: &Hub, id: &'static str, name: &'static str, source: &'static str, windows: Vec<Window>) {
    hub.update_usage(|usage| usage.merge(id, name, source, windows));
}

pub async fn poll(hub: Arc<Hub>) {
    loop {
        read_all(&hub).await;
        let next = tokio::time::Instant::now() + POLL_EVERY;
        // An agent that just became wanted (a new bot's, one the user turned on) reads right away.
        while tokio::time::timeout_at(next, hub.usage_wake.notified()).await.is_ok() {
            read(&hub, readers(&hub, services::unread_readers).await).await;
        }
    }
}

/// A forced read for a caller that's waiting, which gets what has landed by [`REFRESH_WAIT`].
pub async fn refresh(hub: &Arc<Hub>) {
    let reading = tokio::spawn({
        let hub = hub.clone();
        async move { read_all(&hub).await }
    });
    // Past the wait the reading carries on by itself.
    if let Ok(Err(error)) = tokio::time::timeout(REFRESH_WAIT, reading).await {
        tracing::warn!(%error, "a usage refresh failed");
    }
}

async fn read_all(hub: &Arc<Hub>) {
    let wanted = |hub: &Hub| {
        services::sync(hub);
        services::wanted_readers(&hub.store)
    };
    read(hub, readers(hub, wanted).await).await;
}

/// The readers `pick` names, away from the async workers: it goes to the database and the disk.
async fn readers(hub: &Arc<Hub>, pick: fn(&Hub) -> Vec<&'static Reader>) -> Vec<&'static Reader> {
    let hub = hub.clone();
    tokio::task::spawn_blocking(move || pick(&hub)).await.unwrap_or_else(|error| {
        tracing::warn!(%error, "couldn't list the agents to read");
        Vec::new()
    })
}

/// Each reading lands as it comes in: a slow agent doesn't hold back the others.
async fn read(hub: &Hub, readers: Vec<&'static Reader>) {
    if crate::agent::backends::agent_checks_off() {
        return;
    }
    let mut readings: FuturesUnordered<_> =
        readers.into_iter().map(|reader| async move { (reader, (reader.read)(hub).await) }).collect();
    while let Some((reader, windows)) = readings.next().await {
        apply(hub, windows, reader.id, reader.name, reader.source);
    }
}

fn iso_ms(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|at| at.timestamp_millis())
}

/// A window id made from its label.
fn slug(label: &str) -> String {
    label.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
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
    hub.update_usage(|usage| usage.replace(id, name, source, windows));
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
    fn the_apps_get_only_what_the_user_shows() {
        let window = Window::new("weekly", "Weekly", 10.0, None);
        let provider = |id| Provider { windows: vec![window.clone()], ..Provider::new(id, id, "test", Vec::new()) };
        let usage = Usage {
            providers: vec![provider("claude"), provider("cursor"), provider("codex")],
            services: vec![
                Service { id: "claude", name: "Claude", registry: None, shown: true },
                Service { id: "cursor", name: "Cursor", registry: None, shown: false },
            ],
        };
        let visible = usage.visible();
        let ids: Vec<_> = visible.providers.iter().map(|p| p.id).collect();
        // A reading with no row yet (a status line before the first sync) isn't lost.
        assert_eq!(ids, ["claude", "codex"]);
        assert_eq!(visible.services.len(), 2, "the chooser still lists the hidden one");
    }

    #[test]
    fn a_reading_names_the_registry_id_its_logo_goes_by() {
        let mut u = Usage::default();
        u.replace("copilot", "GitHub Copilot", "test", vec![Window::note("plan", "Plan", "Unlimited")]);
        u.replace("hermes", "Hermes Agent", "test", vec![Window::note("plan", "Plan", "Unlimited")]);
        let value = serde_json::to_value(&u).unwrap();
        assert_eq!(value["providers"][0]["registry"], "github-copilot-cli");
        assert!(value["providers"][1].get("registry").is_none(), "no registry entry: clients use the id");
    }

    #[test]
    fn a_note_does_not_count_as_the_plan_running_out() {
        let note = Window { percent: 99.0, ..Window::note("cost", "Cost today", "$9") };
        assert!(!over_headroom(&[note]));
        assert!(over_headroom(&[Window::new("five_hour", "5-hour", 80.0, None)]));
    }
}
