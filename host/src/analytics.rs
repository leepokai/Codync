//! Product analytics (`PostHog`), on unless the computer's owner turned it off
//! (`setAnalytics`). Events name the feature that was used and never carry content:
//! no message text, prompts, file names, paths or bot names.
//! What is collected and why: docs/features/analytics.md.

use crate::LockExt as _;
use crate::api::devices::Caller;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub mod errors;

const KV_ENABLED: &str = "analytics_enabled";
/// The account the install id was last merged into (one `$identify` per account).
const KV_IDENTIFIED: &str = "analytics_identified";
/// Write-only project token: safe to ship in public builds.
const TOKEN: &str = "phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX";
const BATCH_URL: &str = "https://us.i.posthog.com/batch/";
const FLUSH_EVERY: Duration = Duration::from_secs(30);
/// Unsent events kept while offline; the oldest go first.
const MAX_PENDING: usize = 500;

/// Everything the host reports. Names follow the `object_verb` convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    AppOpened,
    OnboardingCompleted,
    SignedIn,
    SignedOut,
    BotCreated,
    BotUpdated,
    BotsReordered,
    BotDeleted,
    MessageSent,
    TurnStopped,
    TurnFinished,
    SessionReset,
    PermissionAnswered,
    RoutineSaved,
    RoutineDeleted,
    RoutineRun,
    VoiceCallFinished,
    RemoteScreenOpened,
    ConnectorInstalled,
    SkillInstalled,
    /// Error tracking (`errors`): a panic or an uncaught error.
    Exception,
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Self::AppOpened => "app_opened",
            Self::OnboardingCompleted => "onboarding_completed",
            Self::SignedIn => "signed_in",
            Self::SignedOut => "signed_out",
            Self::BotCreated => "bot_created",
            Self::BotUpdated => "bot_updated",
            Self::BotsReordered => "bots_reordered",
            Self::BotDeleted => "bot_deleted",
            Self::MessageSent => "message_sent",
            Self::TurnStopped => "turn_stopped",
            Self::TurnFinished => "turn_finished",
            Self::SessionReset => "session_reset",
            Self::PermissionAnswered => "permission_answered",
            Self::RoutineSaved => "routine_saved",
            Self::RoutineDeleted => "routine_deleted",
            Self::RoutineRun => "routine_run",
            Self::VoiceCallFinished => "voice_call_finished",
            Self::RemoteScreenOpened => "remote_screen_opened",
            Self::ConnectorInstalled => "connector_installed",
            Self::SkillInstalled => "skill_installed",
            Self::Exception => "$exception",
        }
    }

    /// Events the computer's own apps report through `track` (the rest come from API calls).
    fn client_reported(self) -> bool {
        matches!(self, Self::AppOpened | Self::OnboardingCompleted | Self::SignedIn | Self::SignedOut)
    }
}

pub struct Analytics {
    enabled: Mutex<bool>,
    tx: mpsc::UnboundedSender<Msg>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Msg>>>,
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
        Self { enabled: Mutex::new(enabled), tx, rx: Mutex::new(Some(rx)) }
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

/// `track`: an event the computer's own app reports (app opened, signed in, …).
pub fn track(hub: &Hub, b: &Value) -> Result<()> {
    let event = Event::deserialize(&b["event"]).map_err(|_| anyhow::anyhow!("unknown event"))?;
    anyhow::ensure!(event.client_reported(), "unknown event");
    capture(hub, event, Map::new());
    Ok(())
}

/// The event an API call stands for, with its properties, read before the call runs.
pub fn observe(caller: &Caller, method: &str, b: &Value) -> Option<(Event, Map<String, Value>)> {
    let mut p = Map::new();
    let event = match method {
        "createBot" => {
            let group = b["kind"] == "group";
            p.insert("kind".into(), (if group { "group" } else { "bot" }).into());
            if group {
                p.insert("members".into(), b["members"].as_array().map_or(0, Vec::len).into());
            } else {
                p.insert("agent".into(), b["backend"].clone());
                p.insert("model".into(), b["model"].clone());
            }
            Event::BotCreated
        }
        "updateBot" => Event::BotUpdated,
        "reorderBots" => Event::BotsReordered,
        "deleteBot" => Event::BotDeleted,
        "send" => {
            let in_thread = b["threadId"].as_str().is_some_and(|t| !t.is_empty());
            p.insert("in_thread".into(), in_thread.into());
            p.insert("attachments".into(), b["attachments"].as_array().map_or(0, Vec::len).into());
            Event::MessageSent
        }
        "stop" => Event::TurnStopped,
        "newSession" => Event::SessionReset,
        "respondPermission" => Event::PermissionAnswered,
        "saveRoutine" => Event::RoutineSaved,
        "deleteRoutine" => Event::RoutineDeleted,
        "runRoutine" => Event::RoutineRun,
        "logCall" => {
            p.insert("seconds".into(), b["seconds"].as_u64().unwrap_or(0).into());
            Event::VoiceCallFinished
        }
        "screenPrepare" => Event::RemoteScreenOpened,
        "installConnector" => {
            // A catalog name is public; a custom connector's URL or command is not.
            let catalog = b["registryName"].as_str().unwrap_or("custom");
            p.insert("connector".into(), catalog.into());
            Event::ConnectorInstalled
        }
        "installSkill" => {
            p.insert("custom".into(), (!b["source"].is_string()).into());
            Event::SkillInstalled
        }
        _ => return None,
    };
    let client = if matches!(caller, Caller::Local) { "local" } else { "remote" };
    p.insert("client".into(), client.into());
    Some((event, p))
}

/// How an agent's turn ended (`turn_finished`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    /// `end_turn`: the agent finished.
    Done,
    /// The user stopped it.
    Stopped,
    /// The agent process or connection failed.
    Failed,
    /// It ended short of `end_turn` (output or step limit, refusal).
    Incomplete,
}

/// What started the turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnSource {
    User,
    Routine,
    Group,
    Bot,
}

/// `turn_finished`: the agent, how the turn ended, why it failed and how long it ran;
/// never its text or the error's.
pub fn turn_finished(
    hub: &Hub,
    agent: &str,
    outcome: TurnOutcome,
    stop_reason: &str,
    failure: Option<&anyhow::Error>,
    source: TurnSource,
    ms: i64,
) {
    let outcome = match outcome {
        TurnOutcome::Done => "done",
        TurnOutcome::Stopped => "stopped",
        TurnOutcome::Failed => "failed",
        TurnOutcome::Incomplete => "incomplete",
    };
    // ACP's stop reasons plus ours; anything else an adapter invents is just `other`.
    let stop_reason = match stop_reason {
        "end_turn" | "cancelled" | "max_tokens" | "max_turn_requests" | "refusal" | "error" | "start_failed" => {
            stop_reason
        }
        _ => "other",
    };
    let source = match source {
        TurnSource::User => "user",
        TurnSource::Routine => "routine",
        TurnSource::Group => "group",
        TurnSource::Bot => "bot",
    };
    let mut p = json!({"agent": agent, "outcome": outcome, "stop_reason": stop_reason, "source": source, "seconds": ms.max(0) / 1000});
    if let Some(e) = failure {
        let (kind, rpc_code) = failure_kind(e);
        p["error"] = kind.into();
        if let Some(code) = rpc_code {
            p["rpc_code"] = code.into();
        }
    }
    capture(hub, Event::TurnFinished, p.as_object().cloned().unwrap_or_default());
}

/// Why a turn failed, as a category (plus the agent's JSON-RPC code): never the error's text,
/// which can hold paths and agent output.
fn failure_kind(e: &anyhow::Error) -> (&'static str, Option<i64>) {
    use crate::agent::acp::{Failure, FailureKind, RpcError};
    if let Some(f) = e.chain().find_map(|c| c.downcast_ref::<Failure>()) {
        let kind = match f.kind {
            FailureKind::NotSetUp => "not_installed",
            FailureKind::Timeout => "timeout",
            FailureKind::Exited => "exited",
        };
        return (kind, None);
    }
    if crate::agent::acp::needs_sign_in(e) {
        return ("sign_in", None);
    }
    for cause in e.chain() {
        if let Some(r) = cause.downcast_ref::<RpcError>() {
            return ("agent_error", Some(r.code));
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            return (if io.kind() == std::io::ErrorKind::NotFound { "not_installed" } else { "io" }, None);
        }
    }
    ("other", None)
}

/// `observe` plus what only the store knows: a resent message (same `clientNonce`) is
/// not a new one, and a message names its bot's agent.
pub fn observe_call(hub: &Hub, caller: &Caller, method: &str, b: &Value) -> Option<(Event, Map<String, Value>)> {
    let (event, mut p) = observe(caller, method, b)?;
    if event == Event::MessageSent {
        let bot = b["botId"].as_str().unwrap_or_default();
        let nonce = b["clientNonce"].as_str().unwrap_or_default();
        if !nonce.is_empty() && hub.store.find_by_nonce(bot, nonce).is_some() {
            return None;
        }
        if let Ok(Some(row)) = hub.store.bot(bot) {
            p.insert("kind".into(), (if row.config.is_group() { "group" } else { "bot" }).into());
            if !row.config.is_group() {
                p.insert("agent".into(), row.config.backend.into());
            }
        }
    }
    Some((event, p))
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
    fn observed_events_carry_no_content() {
        let b = json!({"botId": "b1", "text": "secret plan", "threadId": "e1", "attachments": ["u1", "u2"]});
        let (event, p) = observe(&Caller::Local, "send", &b).expect("send is tracked");
        assert_eq!(event, Event::MessageSent);
        assert_eq!(Value::Object(p.clone()), json!({"in_thread": true, "attachments": 2, "client": "local"}));
        assert!(!Value::Object(p).to_string().contains("secret"));

        let custom = json!({"name": "mine", "command": "/Users/me/bin/server"});
        let (_, p) = observe(&Caller::Local, "installConnector", &custom).expect("tracked");
        assert_eq!(p["connector"], "custom");
        assert!(observe(&Caller::Local, "history", &json!({})).is_none());
    }

    #[tokio::test]
    async fn a_resent_message_counts_once_and_names_its_agent() {
        let dir = std::env::temp_dir().join(format!("codync-analytics-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(
            store,
            "h".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "t".into(),
            0,
        );
        let cfg = serde_json::from_value(json!({"id": "b1", "name": "b", "backend": "codex", "cwd": "/tmp"})).unwrap();
        hub.store.save_bot(&cfg).unwrap();
        let send = json!({"botId": "b1", "text": "hi", "clientNonce": "n1"});

        let (_, p) = observe_call(&hub, &Caller::Local, "send", &send).expect("a new message is tracked");
        assert_eq!((p["agent"].as_str(), p["kind"].as_str()), (Some("codex"), Some("bot")));
        let lane = crate::store::Lane::main("b1");
        hub.store
            .insert_entry(&lane, crate::store::EntryKind::User, 1, &json!({"text": "hi", "clientNonce": "n1"}))
            .unwrap();
        assert!(observe_call(&hub, &Caller::Local, "send", &send).is_none(), "a replay is the same message");
        std::fs::remove_dir_all(&dir).unwrap();
    }

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

    #[test]
    fn a_failed_turn_names_its_kind_not_its_text() {
        use crate::agent::acp::{Failure, FailureKind, RpcError};
        use anyhow::Context as _;
        let setup = Failure::error(FailureKind::NotSetUp, "Codex isn't set up on this computer.");
        assert_eq!(failure_kind(&setup), ("not_installed", None));
        let hung = Err::<(), _>(Failure::error(FailureKind::Timeout, "no answer")).context("starting");
        assert_eq!(failure_kind(&hung.expect_err("failed")), ("timeout", None));
        let missing = std::io::Error::new(std::io::ErrorKind::NotFound, "/Users/me/bin/agent");
        let spawn = Err::<(), _>(missing).context("starting `/Users/me/bin/agent`");
        assert_eq!(failure_kind(&spawn.expect_err("failed")), ("not_installed", None));
        let auth = anyhow::Error::new(RpcError { code: -32000, message: "Authentication required".into() });
        assert_eq!(failure_kind(&auth), ("sign_in", None));
        let rpc = anyhow::Error::new(RpcError { code: -32603, message: "Internal error: /Users/me".into() });
        assert_eq!(failure_kind(&rpc), ("agent_error", Some(-32603)));
        assert_eq!(failure_kind(&anyhow::anyhow!("something else")), ("other", None));
    }

    #[test]
    fn only_app_events_can_be_tracked_directly() {
        assert!(Event::deserialize(&json!("app_opened")).is_ok_and(Event::client_reported));
        assert!(!Event::deserialize(&json!("message_sent")).is_ok_and(Event::client_reported));
        assert!(Event::deserialize(&json!("anything_else")).is_err());
    }
}
