//! Opt-in product analytics (`PostHog`), decided once per computer by its owner
//! (`setAnalytics`; nothing is sent until then). Events name the feature that was used
//! and never carry content: no message text, prompts, file names, paths or bot names.
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
use tokio::sync::mpsc;

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
    SessionReset,
    PermissionAnswered,
    RoutineSaved,
    RoutineDeleted,
    RoutineRun,
    VoiceCallFinished,
    RemoteScreenOpened,
    ConnectorInstalled,
    SkillInstalled,
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
            Self::SessionReset => "session_reset",
            Self::PermissionAnswered => "permission_answered",
            Self::RoutineSaved => "routine_saved",
            Self::RoutineDeleted => "routine_deleted",
            Self::RoutineRun => "routine_run",
            Self::VoiceCallFinished => "voice_call_finished",
            Self::RemoteScreenOpened => "remote_screen_opened",
            Self::ConnectorInstalled => "connector_installed",
            Self::SkillInstalled => "skill_installed",
        }
    }

    /// Events the computer's own apps report through `track` (the rest come from API calls).
    fn client_reported(self) -> bool {
        matches!(self, Self::AppOpened | Self::OnboardingCompleted | Self::SignedIn | Self::SignedOut)
    }
}

pub struct Analytics {
    /// `None` until the owner has chosen.
    enabled: Mutex<Option<bool>>,
    tx: mpsc::UnboundedSender<Value>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Value>>>,
}

impl Analytics {
    pub fn load(store: &Store) -> Self {
        let enabled = match store.kv_get(KV_ENABLED).as_deref() {
            Some("1") => Some(true),
            Some("0") => Some(false),
            _ => None,
        };
        let (tx, rx) = mpsc::unbounded_channel();
        Self { enabled: Mutex::new(enabled), tx, rx: Mutex::new(Some(rx)) }
    }

    /// `null` until asked, which is when the computer's apps ask.
    pub fn state(&self) -> Option<bool> {
        *self.enabled.locked()
    }

    pub fn set_enabled(&self, store: &Store, on: bool) -> Result<()> {
        store.kv_set(KV_ENABLED, if on { "1" } else { "0" })?;
        *self.enabled.locked() = Some(on);
        Ok(())
    }

    fn enabled(&self) -> bool {
        self.state() == Some(true)
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

/// Queues one event when the owner said yes. Signed in to a Codync account, events are
/// linked to that account; otherwise to this install's random id, with no person profile.
pub fn capture(hub: &Hub, event: Event, mut properties: Map<String, Value>) {
    if !hub.analytics.enabled() {
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
    properties.insert("$process_person_profile".into(), account.is_some().into());
    queue(hub, event.name(), account.as_deref().unwrap_or(&hub.host_id), properties);
}

fn queue(hub: &Hub, event: &str, distinct_id: &str, mut properties: Map<String, Value>) {
    properties.extend([
        ("$lib".into(), "codync-host".into()),
        ("app_version".into(), env!("CARGO_PKG_VERSION").into()),
        ("env".into(), (if option_env!("CODYNC_ENV") == Some("main") { "main" } else { "dev" }).into()),
        ("$os".into(), std::env::consts::OS.into()),
        ("arch".into(), std::env::consts::ARCH.into()),
        ("$geoip_disable".into(), true.into()),
    ]);
    let _ = hub.analytics.tx.send(json!({"event": event, "distinct_id": distinct_id, "properties": properties}));
}

/// Sends queued events in batches every `FLUSH_EVERY`; turning analytics off drops them.
pub fn start(hub: &Arc<Hub>) {
    let Some(mut rx) = hub.analytics.rx.locked().take() else { return };
    let hub = hub.clone();
    tokio::spawn(async move {
        let mut pending: Vec<Value> = Vec::new();
        let mut tick = tokio::time::interval(FLUSH_EVERY);
        loop {
            tokio::select! {
                event = rx.recv() => match event {
                    Some(e) => pending.push(e),
                    None => return,
                },
                _ = tick.tick() => {
                    let sent = hub.analytics.enabled() && !pending.is_empty() && send(&pending).await;
                    if sent || !hub.analytics.enabled() {
                        pending.clear();
                    }
                }
            }
            if pending.len() > MAX_PENDING {
                pending.drain(..pending.len() - MAX_PENDING);
            }
        }
    });
}

async fn send(batch: &[Value]) -> bool {
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

    #[test]
    fn only_app_events_can_be_tracked_directly() {
        assert!(Event::deserialize(&json!("app_opened")).is_ok_and(Event::client_reported));
        assert!(!Event::deserialize(&json!("message_sent")).is_ok_and(Event::client_reported));
        assert!(Event::deserialize(&json!("anything_else")).is_err());
    }
}
