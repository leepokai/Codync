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
pub use services::{bots_changed, login_profiles_changed, set_order, set_shown};

use crate::LockExt;
use crate::hub::Hub;
use futures::future::BoxFuture;
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

/// Usage of the plan `cfg`'s agent runs on (its login profile's, or the default login's) above
/// [`HEADROOM_PERCENT`] in any window. A note (today's tokens, a dollar balance) is not a plan window.
pub fn near_limit(hub: &Hub, cfg: &crate::store::BotConfig) -> bool {
    let profile = crate::agent::login_profiles::of_bot(&hub.store, &cfg.backend, cfg);
    hub.usage.locked().windows(&cfg.backend, profile).is_some_and(over_headroom)
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
    /// The default login's windows.
    pub windows: Vec<Window>,
    /// Where the numbers came from: `claude` (CLI), `statusline`, `agent`, `sessions`,
    /// or a short phrase for the other agents.
    pub source: &'static str,
    pub updated_at: i64,
    /// What the default login is called, while the agent has login profiles to tell it from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_login_profile: Option<String>,
    /// The agent's signed-in account login profiles, each with its own windows: none yet reads **No reading yet**.
    /// Which ones are listed is [`services::sync`]'s; a reading only fills a row that is there.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub login_profiles: Vec<LoginReading>,
}

/// One login profile's plan windows.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoginReading {
    pub id: String,
    pub name: String,
    pub windows: Vec<Window>,
    /// Zero until its first reading comes back.
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

    /// The windows of `backend`'s login: `profile`'s, or the default one's.
    pub fn windows(&self, backend: &str, profile: Option<&str>) -> Option<&[Window]> {
        let id = crate::agent::backends::agent_id(backend);
        let provider = self.providers.iter().find(|p| p.id == id)?;
        match profile {
            None => Some(&provider.windows),
            Some(profile) => provider.login_profiles.iter().find(|l| l.id == profile).map(|l| l.windows.as_slice()),
        }
    }

    /// The login profiles `reader`'s agent lists (`rows`: id and name) and what its default login is called beside
    /// them. A row that stays keeps its reading; an agent with rows is listed before its first reading.
    fn list_login_profiles(&mut self, reader: &Reader, default: Option<String>, rows: Vec<(String, String)>) {
        if rows.is_empty() && !self.providers.iter().any(|p| p.id == reader.id) {
            return;
        }
        let provider = self.provider(reader, reader.source);
        let old = std::mem::take(&mut provider.login_profiles);
        let row = |(id, name): (String, String)| {
            let kept = old.iter().find(|row| row.id == id);
            let windows = kept.map(|row| row.windows.clone()).unwrap_or_default();
            LoginReading { windows, updated_at: kept.map_or(0, |row| row.updated_at), id, name }
        };
        provider.login_profiles = rows.into_iter().map(row).collect();
        provider.default_login_profile = default.filter(|_| !provider.login_profiles.is_empty());
        self.tidy();
    }

    /// `reader`'s agent, listed with nothing read yet when it wasn't there.
    fn provider(&mut self, reader: &Reader, source: &'static str) -> &mut Provider {
        let at = self.providers.iter().position(|p| p.id == reader.id).unwrap_or_else(|| {
            let unread = Provider { updated_at: 0, ..Provider::new(reader.id, reader.name, source, Vec::new()) };
            self.providers.push(unread);
            self.providers.len() - 1
        });
        &mut self.providers[at]
    }

    /// An agent with no windows and no login profiles has nothing to show.
    fn tidy(&mut self) {
        self.providers.retain(|p| !p.windows.is_empty() || !p.login_profiles.is_empty());
        self.providers.sort_by_key(|p| p.id);
    }

    /// A reading of `reader`'s agent lands: on login profile `profile`'s row (dropped when the row is gone), or on
    /// the default login. An agent left with no windows and no rows is removed.
    fn land(&mut self, reader: &Reader, source: &'static str, profile: Option<&str>, reading: Reading) {
        let provider = self.provider(reader, source);
        let now = crate::store::now_ms();
        match profile {
            None => {
                reading.onto(&mut provider.windows);
                (provider.source, provider.updated_at) = (source, now);
            }
            Some(profile) => {
                if let Some(row) = provider.login_profiles.iter_mut().find(|l| l.id == profile) {
                    reading.onto(&mut row.windows);
                    row.updated_at = now;
                }
            }
        }
        self.tidy();
    }
}

/// How a reading changes the windows a login already shows.
enum Reading {
    /// Some windows at a time, for Claude's status line and rate-limit events. The others stay.
    Some(Vec<Window>),
    /// A probe's full reading: windows it no longer reports are dropped.
    All(Vec<Window>),
    /// The probe couldn't tell: what's shown stays.
    Unknown,
}

impl Reading {
    fn onto(self, windows: &mut Vec<Window>) {
        match self {
            Self::Some(new) => {
                windows.retain(|w| !new.iter().any(|n| n.id == w.id));
                windows.extend(new);
            }
            Self::All(new) => *windows = new,
            Self::Unknown => {}
        }
        windows.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

impl Provider {
    fn new(id: &'static str, name: &'static str, source: &'static str, windows: Vec<Window>) -> Self {
        Self {
            id,
            name,
            registry: services::registry(id),
            windows,
            source,
            updated_at: crate::store::now_ms(),
            default_login_profile: None,
            login_profiles: Vec::new(),
        }
    }
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

/// Each reading lands as it comes in: a slow agent doesn't hold back the others. A reader that can read a login
/// profile reads each one listed under its agent, beside the default login.
async fn read(hub: &Hub, readers: Vec<&'static Reader>) {
    if crate::agent::backends::agent_checks_off() {
        return;
    }
    let mut readings: FuturesUnordered<BoxFuture<'_, ()>> = FuturesUnordered::new();
    for reader in readers {
        readings.push(Box::pin(async move {
            if let Some(windows) = (reader.read)(hub).await {
                hub.update_usage(|usage| usage.land(reader, reader.source, None, Reading::All(windows)));
            }
        }));
        for profile in listed_profiles(hub, reader) {
            readings.push(Box::pin(async move {
                let reading = read_profile(hub, reader, &profile).await.map_or(Reading::Unknown, Reading::All);
                hub.update_usage(|usage| usage.land(reader, reader.source, Some(&profile), reading));
            }));
        }
    }
    while readings.next().await.is_some() {}
}

/// The login profiles listed under `reader`'s agent, when it can read one.
fn listed_profiles(hub: &Hub, reader: &Reader) -> Vec<String> {
    if reader.read_profile.is_none() {
        return Vec::new();
    }
    let usage = hub.usage.locked();
    let rows = usage.providers.iter().find(|p| p.id == reader.id).map(|p| p.login_profiles.as_slice());
    rows.unwrap_or_default().iter().map(|row| row.id.clone()).collect()
}

/// `profile`'s windows, read by its agent's own CLI pointed at the profile's folder. `None`: it couldn't tell.
async fn read_profile(hub: &Hub, reader: &Reader, profile: &str) -> Option<Vec<Window>> {
    use crate::agent::login_profiles;
    use anyhow::Context;
    let read = reader.read_profile?;
    let ready = || {
        // Held while the CLI runs, so the profile isn't removed under it.
        let running = login_profiles::hold(&hub.store, reader.id, Some(profile))?.context("another agent's profile")?;
        anyhow::Ok((running, login_profiles::env(&hub.store, reader.id, Some(profile))?.0))
    };
    match ready() {
        Ok((_running, env)) => read(env).await,
        Err(error) => {
            tracing::debug!(agent = reader.id, error = format!("{error:#}"), "couldn't read a login profile's usage");
            None
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn reader(id: &str) -> &'static Reader {
        services::reader(id).expect("a reader")
    }

    fn land(usage: &mut Usage, id: &str, source: &'static str, profile: Option<&str>, reading: Reading) {
        usage.land(reader(id), source, profile, reading);
    }

    #[test]
    fn merge_replaces_only_matching_windows() {
        let mut u = Usage::default();
        let both = vec![Window::new("five_hour", "5-hour", 10.0, None), Window::new("seven_day", "Weekly", 50.0, None)];
        land(&mut u, "claude", "claude", None, Reading::Some(both));
        land(&mut u, "claude", "statusline", None, Reading::Some(vec![Window::new("five_hour", "5-hour", 12.0, None)]));
        let c = &u.providers[0];
        assert_eq!((c.windows.len(), c.source), (2, "statusline"));
        assert!((c.windows[0].percent - 12.0).abs() < f64::EPSILON, "five_hour updated");
        let wire = serde_json::to_value(&u).unwrap();
        assert_eq!(wire["providers"][0]["windows"][1]["resetsAt"], Value::Null);
        assert!(wire["providers"][0].get("loginProfiles").is_none(), "an agent on one login says nothing of profiles");
    }

    #[test]
    fn a_full_reading_drops_windows_the_probe_no_longer_reports() {
        let mut u = Usage::default();
        let first = vec![Window::new("monthly", "Monthly", 40.0, None), Window::note("on_demand", "On-Demand", "$1")];
        land(&mut u, "cursor", "Cursor", None, Reading::All(first));
        land(&mut u, "cursor", "Cursor", None, Reading::All(vec![Window::note("unlimited", "Monthly", "Unlimited")]));
        assert_eq!(u.providers[0].windows.len(), 1);
        assert_eq!(u.providers[0].windows[0].id, "unlimited");
        land(&mut u, "cursor", "Cursor", None, Reading::All(Vec::new()));
        assert_eq!(u.providers, []);
    }

    #[test]
    fn each_login_profile_keeps_its_own_windows_beside_the_default_login() {
        let mut u = Usage::default();
        let weekly = |percent| vec![Window::new("seven_day", "Weekly", percent, None)];
        // A reading for a profile nobody listed is dropped.
        land(&mut u, "claude", "claude", Some("work"), Reading::All(weekly(90.0)));
        assert_eq!(u.providers, []);

        let rows = vec![("work".to_owned(), "Work".to_owned()), ("side".to_owned(), "Side".to_owned())];
        u.list_login_profiles(reader("claude"), Some("me@example.com".into()), rows);
        let wire = serde_json::to_value(&u).unwrap();
        assert_eq!(wire["providers"][0]["windows"], serde_json::json!([]), "listed before anything is read");
        assert_eq!(wire["providers"][0]["defaultLoginProfile"], "me@example.com");
        assert_eq!(wire["providers"][0]["loginProfiles"][1]["updatedAt"], 0);

        land(&mut u, "claude", "claude", None, Reading::All(weekly(10.0)));
        land(&mut u, "claude", "claude", Some("work"), Reading::All(weekly(90.0)));
        land(
            &mut u,
            "claude",
            "agent",
            Some("work"),
            Reading::Some(vec![Window::new("five_hour", "5-hour", 5.0, None)]),
        );
        land(&mut u, "claude", "claude", Some("side"), Reading::Unknown);
        let percents = |windows: &[Window]| windows.iter().map(|w| w.percent).collect::<Vec<_>>();
        assert_eq!(percents(u.windows("claude-acp", None).unwrap()), [10.0], "by its registry id too");
        assert_eq!(percents(u.windows("claude", Some("work")).unwrap()), [5.0, 90.0]);
        assert_eq!(u.windows("claude", Some("side")).unwrap(), []);
        assert_ne!(u.providers[0].login_profiles[1].updated_at, 0, "asked, so it isn't read again right away");
        assert_eq!(u.windows("claude", Some("gone")), None);

        // One renamed and one removed: the one that stays keeps its reading.
        u.list_login_profiles(reader("claude"), None, vec![("work".to_owned(), "Day job".to_owned())]);
        let work = &u.providers[0].login_profiles[0];
        assert_eq!((work.name.as_str(), work.windows.len(), u.providers[0].login_profiles.len()), ("Day job", 2, 1));
        // With the last one gone, the default login is the agent's only one again.
        u.list_login_profiles(reader("claude"), Some("me@example.com".into()), Vec::new());
        assert_eq!((u.providers[0].default_login_profile.as_deref(), u.providers[0].windows.len()), (None, 1));
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
        land(&mut u, "copilot", "test", None, Reading::All(vec![Window::note("plan", "Plan", "Unlimited")]));
        land(&mut u, "hermes", "test", None, Reading::All(vec![Window::note("plan", "Plan", "Unlimited")]));
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
