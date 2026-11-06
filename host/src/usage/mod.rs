//! Subscription usage, read on the host.
//!
//! Claude: `claude -p /usage`, the status line, and the ACP rate-limit event.
//! Codex: the newest rollout's `rate_limits`. The other agents Codync runs are
//! read the same way when they can report locally: `omp usage`, `amp usage`,
//! `kiro-cli /usage`, `gemini /usage`, a `kimi` screen, Antigravity's language
//! server, OpenCode's console usage endpoint, Vibe's session logs, and the
//! `JetBrains` IDE quota file.
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
mod gemini;
mod grok;
mod junie;
mod kimi;
mod kiro;
mod omp;
mod opencode;
mod vibe;

pub use claude::{ingest_claude_rate_limit, ingest_statusline};

use crate::LockExt;
use crate::hub::Hub;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const POLL_EVERY: Duration = Duration::from_mins(5);
/// The last reading, so the counters are on screen while a fresh probe runs.
const KV_USAGE: &str = "usage.cache";
/// The timer and several clients can ask for a reading at once: one probe owns it.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// When the last probe finished. A client that waited for the one in flight
/// gets its result instead of starting another probe right away.
static LAST_REFRESH: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
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
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub providers: Vec<Provider>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub windows: Vec<Window>,
    /// Where the numbers came from: `claude` (CLI), `statusline`, `agent`, `sessions`,
    /// or a short phrase for the other agents.
    pub source: String,
    pub updated_at: i64,
    /// Why a provider has no windows (`unsupported`, `unavailable`), when it can't report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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
    fn merge(&mut self, id: &str, name: &str, source: &'static str, new: Vec<Window>) {
        let mut windows = self.providers.iter().find(|p| p.id == id).map(|p| p.windows.clone()).unwrap_or_default();
        windows.retain(|w| !new.iter().any(|n| n.id == w.id));
        windows.extend(new);
        windows.sort_by(|a, b| a.id.cmp(&b.id));
        self.providers.retain(|p| p.id != id);
        self.providers.push(Provider {
            id: id.into(),
            name: name.into(),
            windows,
            source: source.into(),
            updated_at: crate::store::now_ms(),
            status: None,
        });
        self.providers.sort_by(|a, b| a.id.cmp(&b.id));
    }

    /// A probe's full reading. Windows it no longer reports are dropped, and an
    /// empty reading removes the provider.
    fn replace(&mut self, id: &str, name: &str, source: &'static str, new: Vec<Window>) {
        self.providers.retain(|p| p.id != id);
        if new.is_empty() {
            return;
        }
        let mut windows = new;
        windows.sort_by(|a, b| a.id.cmp(&b.id));
        self.providers.push(Provider {
            id: id.into(),
            name: name.into(),
            windows,
            source: source.into(),
            updated_at: crate::store::now_ms(),
            status: None,
        });
        self.providers.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

pub(super) fn merge(hub: &Hub, id: &str, name: &str, source: &'static str, windows: Vec<Window>) {
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

/// The last reading saved on this computer, so a restart shows the counters
/// immediately instead of waiting a minute for the first probe.
pub fn load(store: &crate::store::Store) -> Usage {
    store.kv_read(KV_USAGE).ok().flatten().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

pub fn save(store: &crate::store::Store, usage: &Usage) {
    if let Ok(raw) = serde_json::to_string(usage) {
        let _ = store.kv_set(KV_USAGE, &raw);
    }
}

pub async fn refresh(hub: &Hub) {
    // The timer and several clients can request a refresh together. One probe
    // owns the work; the others wait for it and then receive its result, so a
    // manual refresh never answers with a half-filled list.
    let Ok(_refresh) = REFRESH.try_lock() else {
        let _done = REFRESH.lock().await;
        return;
    };
    if LAST_REFRESH.locked().is_some_and(|last| last.elapsed() < Duration::from_secs(2)) {
        return;
    }
    // Antigravity's adapters report per account; the language server below fills a base reading when they can't.
    antigravity::refresh(hub).await;
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
        gemini_windows,
        grok_windows,
    ) = tokio::join!(
        codex::collect(),
        claude::collect(&hub.store),
        omp::collect(),
        amp::collect(),
        kiro::collect(),
        tokio::task::spawn_blocking(kimi::collect),
        antigravity::collect(),
        opencode::collect(),
        tokio::task::spawn_blocking(vibe::collect),
        tokio::task::spawn_blocking(junie::collect),
        cursor::collect(),
        gemini::collect(),
        grok::collect(),
    );
    let backends = tokio::task::spawn_blocking(crate::agent::backends::list).await.unwrap_or_default();
    let signed_out: std::collections::HashSet<&str> =
        backends.iter().filter(|b| b["signedIn"].as_bool() == Some(false)).filter_map(|b| b["id"].as_str()).collect();
    // A signed-out CLI can leave local history behind (OpenCode's database). That
    // is not a plan this account can spend, so it must not become a bar.
    {
        let mut usage = hub.usage.locked().clone();
        let before = usage.providers.len();
        usage.providers.retain(|p| !signed_out_base(&signed_out, &p.id));
        if usage.providers.len() != before {
            hub.set_usage(usage);
        }
    }
    apply(hub, &signed_out, codex, "codex", "Codex", "codex");
    apply(hub, &signed_out, claude, "claude", "Claude", "claude");
    apply(hub, &signed_out, omp_windows, "omp", "oh-my-pi", "omp usage");
    apply(hub, &signed_out, amp_windows, "amp", "Amp", "amp usage");
    apply(hub, &signed_out, kiro_windows, "kiro", "Kiro CLI", "kiro-cli");
    apply(hub, &signed_out, joined(kimi_windows), "kimi", "Kimi Code", "kimi");
    apply(hub, &signed_out, antigravity, "antigravity", "Google Antigravity", "antigravity");
    apply(hub, &signed_out, opencode, "opencode", "OpenCode", "opencode usage");
    apply(hub, &signed_out, joined(vibe_notes), "vibe", "Mistral Vibe", "vibe logs");
    apply(hub, &signed_out, joined(junie), "junie", "Junie", "JetBrains IDE");
    apply(hub, &signed_out, cursor_windows, "cursor", "Cursor", "Cursor");
    apply(hub, &signed_out, gemini_windows, "gemini", "Gemini CLI", "gemini /usage");
    apply(hub, &signed_out, grok_windows, "grok", "Grok", "grok billing");
    ensure_connected(hub, &backends);
    *LAST_REFRESH.locked() = Some(std::time::Instant::now());
}

/// A connected agent that couldn't report is still shown, with its status, so
/// its counters don't silently disappear from the sidebar and Settings.
fn ensure_connected(hub: &Hub, connected: &[serde_json::Value]) {
    let mut wanted: Vec<(String, String)> = Vec::new();
    for backend in connected {
        if backend["installed"] != true || backend["signedIn"] != true {
            continue;
        }
        let Some(id) = backend["id"].as_str() else { continue };
        let Some(name) = reported_name(id) else { continue };
        wanted.push((id.to_owned(), name.to_owned()));
    }
    let mut usage = hub.usage.locked().clone();
    // Gemini (Antigravity) and Gemini CLI are the same Google account: when the
    // plan is already reported, a second "unavailable" row only adds noise.
    let gemini_covered = usage.providers.iter().any(|p| p.id == "antigravity" && !p.windows.is_empty());
    let wanted: Vec<(String, String)> =
        wanted.into_iter().filter(|(id, _)| !(gemini_covered && id == "gemini")).collect();
    let ids: Vec<&str> = wanted.iter().map(|(id, _)| id.as_str()).collect();
    let before = usage.providers.len();
    usage.providers.retain(|p| p.source != "agent" || ids.contains(&p.id.as_str()));
    let mut changed = usage.providers.len() != before;
    for (id, name) in wanted {
        if usage.providers.iter().any(|p| p.id == id) {
            continue;
        }
        usage.providers.push(Provider {
            id,
            name,
            windows: Vec::new(),
            source: "agent".into(),
            updated_at: crate::store::now_ms(),
            status: Some("unavailable".into()),
        });
        changed = true;
    }
    if changed {
        usage.providers.sort_by(|a, b| a.id.cmp(&b.id));
        hub.set_usage(usage);
    }
}

/// The agents whose plan limits Codync can read, by backend id.
fn reported_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "claude" => "Claude",
        "codex" => "Codex",
        "grok" => "Grok",
        "gemini" => "Gemini CLI",
        "antigravity" => "Gemini",
        "cursor" => "Cursor",
        "opencode" => "OpenCode",
        "kimi" => "Kimi Code",
        "amp" => "Amp",
        "kiro" => "Kiro CLI",
        "omp" => "oh-my-pi",
        "vibe" => "Mistral Vibe",
        "junie" => "Junie",
        _ => return None,
    })
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

fn signed_out_base(signed_out: &std::collections::HashSet<&str>, id: &str) -> bool {
    signed_out.contains(id.split(':').next().unwrap_or(id))
}

fn apply(
    hub: &Hub,
    signed_out: &std::collections::HashSet<&str>,
    windows: Option<Vec<Window>>,
    id: &'static str,
    name: &'static str,
    source: &'static str,
) {
    let Some(windows) = windows else { return };
    if signed_out_base(signed_out, id) {
        return;
    }
    let mut usage = hub.usage.locked().clone();
    usage.replace(id, name, source, windows);
    hub.set_usage(usage);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn a_signed_out_backend_keeps_its_local_history_off_the_bars() {
        let signed_out = std::collections::HashSet::from(["opencode"]);
        assert!(signed_out_base(&signed_out, "opencode"));
        assert!(signed_out_base(&signed_out, "opencode:work"));
        assert!(!signed_out_base(&signed_out, "claude"));
    }

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
        assert_eq!((c.windows.len(), c.source.as_str()), (2, "statusline"));
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

    #[test]
    fn a_saved_reading_comes_back_after_a_restart() {
        let store = crate::store::tests::temp_store();
        let mut usage = Usage::default();
        usage.replace(
            "grok",
            "Grok",
            "grok billing",
            vec![Window::new("grok", "Weekly", 42.0, Some(1_794_096_000_000))],
        );
        save(&store, &usage);
        assert_eq!(load(&store), usage);
        assert_eq!(load(&crate::store::tests::temp_store()), Usage::default(), "another computer starts empty");
    }
}
