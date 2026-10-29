//! Product analytics (`PostHog`), on unless the computer's owner turned it off
//! (`setAnalytics`). Events name the feature that was used and never carry content:
//! no message text, prompts, file names, paths or bot names.
//! What is collected and why: docs/features/analytics.md.

use crate::LockExt as _;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, oneshot};

pub mod errors;
mod lifecycle;
mod observe;
pub mod reported;
mod session;
mod turns;

pub use lifecycle::{KV_STARTED, Via, device_paired, host_started, phone_connected};
pub use observe::{answered, observe_call};
pub use reported::track;
pub use turns::{TurnOutcome, TurnSource, agent_fell_back, turn_finished};

const KV_ENABLED: &str = "analytics_enabled";
/// The account the install id was last merged into (one `$identify` per account).
const KV_IDENTIFIED: &str = "analytics_identified";
/// Write-only project token: safe to ship in public builds.
const TOKEN: &str = "phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX";
const BATCH_URL: &str = "https://us.i.posthog.com/batch/";
const FLUSH_EVERY: Duration = Duration::from_secs(30);
/// Unsent events kept while offline; the oldest go first.
const MAX_PENDING: usize = 500;

/// Everything the host reports. Names follow the `object_verb` convention; each event's
/// sender and properties: docs/reference/analytics-events.md.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    AppOpened,
    WindowOpened,
    HostStarted,
    OnboardingCompleted,
    ComputerAccessFinished,
    SignInStarted,
    SignInFailed,
    SignedIn,
    SignedOut,
    PairingCodeCreated,
    DevicePaired,
    AccessRequestDenied,
    DeviceRevoked,
    PhoneConnected,
    StarPromptAnswered,
    LinkOpened,
    ScreenViewed,
    BotCreated,
    BotUpdated,
    BotsReordered,
    BotDeleted,
    MessageSent,
    TurnStopped,
    TurnFinished,
    /// A turn moved on to the bot's next agent (`agent::fallback`).
    AgentFellBack,
    SessionReset,
    PermissionAnswered,
    ReactionAdded,
    HandoffAnswered,
    RoutineSaved,
    RoutineToggled,
    RoutineDeleted,
    RoutineRun,
    RoutineRetried,
    VoiceCallStarted,
    VoiceCallFinished,
    /// How one engine of a call fared (time to ready, reconnects, a phase stuck, failure).
    VoiceCallQuality,
    RemoteScreenOpened,
    ScreenTakenOver,
    ConnectorInstalled,
    ConnectorsImported,
    ConnectorRemoved,
    ConnectorSigninStarted,
    SkillInstalled,
    SkillRemoved,
    SkillWritten,
    /// A bot's `publish_artifact`.
    ArtifactPublished,
    CredentialSaved,
    AgentSetupStarted,
    AgentSignedIn,
    SettingChanged,
    HostUpdateInstalled,
    /// `networkCreate` or `networkJoin`: this computer is in a network of the user's computers.
    NetworkJoined,
    NetworkChanged,
    /// Error tracking (`errors`): a panic or an uncaught error.
    Exception,
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Self::AppOpened => "app_opened",
            Self::WindowOpened => "window_opened",
            Self::HostStarted => "host_started",
            Self::OnboardingCompleted => "onboarding_completed",
            Self::ComputerAccessFinished => "computer_access_finished",
            Self::SignInStarted => "sign_in_started",
            Self::SignInFailed => "sign_in_failed",
            Self::SignedIn => "signed_in",
            Self::SignedOut => "signed_out",
            Self::PairingCodeCreated => "pairing_code_created",
            Self::DevicePaired => "device_paired",
            Self::AccessRequestDenied => "access_request_denied",
            Self::DeviceRevoked => "device_revoked",
            Self::PhoneConnected => "phone_connected",
            Self::StarPromptAnswered => "star_prompt_answered",
            Self::LinkOpened => "link_opened",
            Self::ScreenViewed => "screen_viewed",
            Self::BotCreated => "bot_created",
            Self::BotUpdated => "bot_updated",
            Self::BotsReordered => "bots_reordered",
            Self::BotDeleted => "bot_deleted",
            Self::MessageSent => "message_sent",
            Self::TurnStopped => "turn_stopped",
            Self::TurnFinished => "turn_finished",
            Self::AgentFellBack => "agent_fell_back",
            Self::SessionReset => "session_reset",
            Self::PermissionAnswered => "permission_answered",
            Self::ReactionAdded => "reaction_added",
            Self::HandoffAnswered => "handoff_answered",
            Self::RoutineSaved => "routine_saved",
            Self::RoutineToggled => "routine_toggled",
            Self::RoutineDeleted => "routine_deleted",
            Self::RoutineRun => "routine_run",
            Self::RoutineRetried => "routine_retried",
            Self::VoiceCallStarted => "voice_call_started",
            Self::VoiceCallFinished => "voice_call_finished",
            Self::VoiceCallQuality => "voice_call_quality",
            Self::RemoteScreenOpened => "remote_screen_opened",
            Self::ScreenTakenOver => "screen_taken_over",
            Self::ConnectorInstalled => "connector_installed",
            Self::ConnectorsImported => "connectors_imported",
            Self::ConnectorRemoved => "connector_removed",
            Self::ConnectorSigninStarted => "connector_signin_started",
            Self::SkillInstalled => "skill_installed",
            Self::SkillRemoved => "skill_removed",
            Self::SkillWritten => "skill_written",
            Self::ArtifactPublished => "artifact_published",
            Self::CredentialSaved => "credential_saved",
            Self::AgentSetupStarted => "agent_setup_started",
            Self::AgentSignedIn => "agent_signed_in",
            Self::SettingChanged => "setting_changed",
            Self::HostUpdateInstalled => "host_update_installed",
            Self::NetworkJoined => "network_joined",
            Self::NetworkChanged => "network_changed",
            Self::Exception => "$exception",
        }
    }
}

pub struct Analytics {
    enabled: Mutex<bool>,
    tx: mpsc::UnboundedSender<Msg>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Msg>>>,
    session: Mutex<session::Session>,
    /// The UTC day each phone last connected (`phone_connected`), in memory only.
    phones: Mutex<HashMap<String, chrono::NaiveDate>>,
}

enum Msg {
    Event(Value),
    /// A panic, captured off the panicking thread (`errors::report_panics`).
    Report(errors::Report),
    /// Send what's queued now and answer when done.
    Flush(oneshot::Sender<()>),
}

impl Analytics {
    pub fn load(store: &Store) -> Self {
        let enabled = store.kv_get(KV_ENABLED).as_deref() != Some("0");
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            enabled: Mutex::new(enabled),
            tx,
            rx: Mutex::new(Some(rx)),
            session: Mutex::default(),
            phones: Mutex::default(),
        }
    }

    pub fn state(&self) -> bool {
        *self.enabled.locked()
    }

    pub fn set_enabled(&self, store: &Store, on: bool) -> Result<()> {
        store.kv_set(KV_ENABLED, if on { "1" } else { "0" })?;
        *self.enabled.locked() = on;
        Ok(())
    }
}

/// A phone's platform as the catalog names it.
fn platform(p: &str) -> &'static str {
    match p {
        "ios" => "ios",
        "android" => "android",
        "macos" => "macos",
        _ => "other",
    }
}

/// The platform of the paired phone `key` (`other` for one this computer doesn't have).
fn platform_of(hub: &Hub, key: &str) -> &'static str {
    platform(hub.store.device(key).as_ref().map_or("", |d| d.platform.as_str()))
}

/// Queues one event unless the owner turned analytics off. Signed in to a Codync account,
/// events are linked to that account; otherwise to this install's random id. Both get a
/// person profile.
pub fn capture(hub: &Hub, event: Event, mut properties: Map<String, Value>) {
    if !hub.analytics.state() {
        return;
    }
    let account = hub.cloud.status().owner.map(|o| o.user_id);
    if let Some(user) = &account
        && hub.store.kv_get(KV_IDENTIFIED).as_deref() != Some(user.as_str())
    {
        // Merges this install's earlier, anonymous events into the account.
        let identify = json!({"$anon_distinct_id": hub.host_id, "$process_person_profile": true});
        queue(hub, "$identify", user, identify.as_object().cloned().unwrap_or_default());
        if let Err(e) = hub.store.kv_set(KV_IDENTIFIED, user) {
            tracing::warn!(error = format!("{e:#}"), "couldn't save the analytics identity");
        }
    }
    properties.insert("$process_person_profile".into(), true.into());
    queue(hub, event.name(), account.as_deref().unwrap_or(&hub.host_id), properties);
}

fn queue(hub: &Hub, event: &str, distinct_id: &str, mut properties: Map<String, Value>) {
    properties.extend([
        ("$lib".into(), "codync-host".into()),
        ("app_version".into(), env!("CARGO_PKG_VERSION").into()),
        ("env".into(), (if option_env!("CODYNC_ENV") == Some("main") { "main" } else { "dev" }).into()),
        ("$os".into(), std::env::consts::OS.into()),
        ("arch".into(), std::env::consts::ARCH.into()),
        // Joins this computer's events across accounts and with its phones' `computer_paired`.
        ("computer".into(), computer_tag(&hub.identity.computer_id()).into()),
        ("$session_id".into(), hub.analytics.session.locked().id(SystemTime::now()).to_string().into()),
    ]);
    let _ = hub.analytics.tx.send(Msg::Event(json!({
        "event": event,
        "distinct_id": distinct_id,
        // When it happened, not when its batch left; the uuid makes a resent batch count once.
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "uuid": uuid::Uuid::new_v4().to_string(),
        "properties": properties,
    })));
}

/// A computer's analytics key: a hash of its id, so a phone's `computer_paired` joins the
/// computer's events without revealing the id the relay knows it by.
pub fn computer_tag(computer_id: &str) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    let hash = Sha256::digest(format!("codync-analytics:{computer_id}"));
    hash[..8].iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Sends queued events in batches every `FLUSH_EVERY`; turning analytics off drops them.
pub fn start(hub: &Arc<Hub>) {
    let Some(mut rx) = hub.analytics.rx.locked().take() else { return };
    errors::report_panics(hub);
    let hub = hub.clone();
    tokio::spawn(async move {
        let mut pending: Vec<Value> = Vec::new();
        let mut tick = tokio::time::interval(FLUSH_EVERY);
        loop {
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some(Msg::Event(e)) => pending.push(e),
                    Some(Msg::Report(r)) => errors::capture_report(&hub, r),
                    Some(Msg::Flush(done)) => {
                        flush(&hub, &mut pending).await;
                        let _ = done.send(());
                    }
                    None => return,
                },
                _ = tick.tick() => flush(&hub, &mut pending).await,
            }
            if pending.len() > MAX_PENDING {
                pending.drain(..pending.len() - MAX_PENDING);
            }
        }
    });
}

async fn flush(hub: &Hub, pending: &mut Vec<Value>) {
    let on = hub.analytics.state();
    let sent = on && !pending.is_empty() && send(pending).await;
    if sent || !on {
        pending.clear();
    }
}

/// Sends what's queued before the host exits, so a restart or update loses nothing.
pub async fn flush_before_exit(hub: &Hub) {
    let (tx, rx) = oneshot::channel();
    if hub.analytics.tx.send(Msg::Flush(tx)).is_ok() {
        let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
    }
}

async fn send(batch: &[Value]) -> bool {
    // Debug builds (`cargo test`, `cargo run`) are throwaway hosts: each e2e test's fresh
    // home would count as a new user, and one built with `CODYNC_ENV=main` lands in production.
    if cfg!(debug_assertions) {
        return true;
    }
    let body = json!({"api_key": TOKEN, "batch": batch});
    match crate::http().post(BATCH_URL).json(&body).timeout(Duration::from_secs(15)).send().await {
        Ok(r) if r.status().is_success() => true,
        Ok(r) => {
            tracing::debug!(status = %r.status(), "analytics batch refused");
            // Refused for good (bad payload): retrying would only repeat it.
            r.status().is_client_error()
        }
        Err(e) => {
            tracing::debug!(error = format!("{e:#}"), "analytics batch not sent");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_until_the_owner_turns_it_off() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert!(Analytics::load(&store).state());
        Analytics::load(&store).set_enabled(&store, false).unwrap();
        assert!(!Analytics::load(&store).state(), "an off stays off");
    }

    #[test]
    fn computer_tag_matches_the_iphone() {
        // Same vector as the iPhone's AnalyticsTests.
        assert_eq!(computer_tag("AAECAwQFBgcICQoLDA0ODw"), "2d6878bee20aba04");
    }
}
