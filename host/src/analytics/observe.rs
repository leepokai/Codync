//! What an API call stands for: `api::dispatch` reports each successful call as one event.
//! Values come from a request only when they are fixed names, booleans, counts or catalog ids.

use super::{Event, platform_of};
use crate::api::devices::Caller;
use crate::hub::Hub;
use crate::store::BotConfig;
use serde_json::{Map, Value};

/// Methods that stand for one event with one fixed property.
const FIXED: [(&str, Event, &str, &str); 14] = [
    ("editHandoff", Event::HandoffAnswered, "action", "edit"),
    ("keepConversation", Event::HandoffAnswered, "action", "keep"),
    ("credentialSaveLogin", Event::CredentialSaved, "kind", "login"),
    ("credentialSetOnePassword", Event::CredentialSaved, "kind", "onepassword"),
    ("credentialUpdateConnector", Event::CredentialSaved, "kind", "connector"),
    ("setComposioKey", Event::CredentialSaved, "kind", "composio"),
    ("setVoiceKey", Event::CredentialSaved, "kind", "voice"),
    ("setAgentEnv", Event::CredentialSaved, "kind", "agent_env"),
    // Never the computer's name, the invite or the recovery code.
    ("networkInvite", Event::NetworkChanged, "action", "invite"),
    ("networkPromote", Event::NetworkChanged, "action", "promote"),
    ("networkRemove", Event::NetworkChanged, "action", "remove"),
    ("networkRecover", Event::NetworkChanged, "action", "recover"),
    ("setNetworkRules", Event::NetworkChanged, "action", "rules"),
    ("networkLeave", Event::NetworkChanged, "action", "leave"),
];

/// Switches the host owns (`setting_changed`): the method and the setting it names.
const SETTINGS: [(&str, &str); 5] = [
    ("setCloud", "remote_access"),
    ("setApproval", "approval"),
    ("setHostAutomaticUpdates", "automatic_updates"),
    ("setScreenEnabled", "remote_screen"),
    ("setAssistants", "cloud_assistants"),
];

/// The event an API call stands for, with its properties, read before the call runs.
pub fn observe(caller: &Caller, method: &str, b: &Value) -> Option<(Event, Map<String, Value>)> {
    let mut p = Map::new();
    let event = if let Some(&(_, event, key, value)) = FIXED.iter().find(|f| f.0 == method) {
        // An empty key or token removes the saved one.
        let cleared = matches!(method, "setComposioKey" | "setVoiceKey" | "credentialSetOnePassword")
            && ["key", "token"].iter().all(|k| b[*k].as_str().is_none_or(|v| v.trim().is_empty()));
        if cleared {
            return None;
        }
        p.insert(key.into(), value.into());
        event
    } else if let Some(&(_, setting)) = SETTINGS.iter().find(|s| s.0 == method) {
        let on = if method == "setApproval" { b["approval"] == "auto" } else { b["enabled"] == true };
        p.insert("setting".into(), setting.into());
        p.insert("on".into(), on.into());
        Event::SettingChanged
    } else {
        call_event(method, b, &mut p)?
    };
    let client = if matches!(caller, Caller::Local) { "local" } else { "remote" };
    p.insert("client".into(), client.into());
    Some((event, p))
}

/// The rest of `observe`: one arm per method.
fn call_event(method: &str, b: &Value, p: &mut Map<String, Value>) -> Option<Event> {
    Some(match method {
        "setCloudUrl" => {
            // The address itself stays off the event. `on` is only whether a custom URL was saved.
            let on = b["url"].as_str().is_some_and(|url| !url.trim().is_empty());
            p.insert("setting".into(), "remote_access_url".into());
            p.insert("on".into(), on.into());
            Event::SettingChanged
        }
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
        "renameSection" => {
            // Whether it took the section away; the names never leave the computer.
            p.insert("removed".into(), b["to"].as_str().is_none_or(|t| t.trim().is_empty()).into());
            Event::SectionRenamed
        }
        "deleteBot" => Event::BotDeleted,
        "send" => {
            let reply = b["replyTo"].as_str().is_some_and(|t| !t.is_empty());
            p.insert("reply".into(), reply.into());
            p.insert("attachments".into(), b["attachments"].as_array().map_or(0, Vec::len).into());
            Event::MessageSent
        }
        "stop" => Event::TurnStopped,
        "newSession" => Event::SessionReset,
        "respondPermission" => Event::PermissionAnswered,
        "react" => Event::ReactionAdded,
        "saveRoutine" => Event::RoutineSaved,
        "setRoutineEnabled" => {
            p.insert("enabled".into(), b["enabled"].as_bool()?.into());
            Event::RoutineToggled
        }
        "deleteRoutine" => Event::RoutineDeleted,
        "runRoutine" => Event::RoutineRun,
        "retryRoutineRun" => Event::RoutineRetried,
        "logCall" | "endCall" => {
            p.insert("seconds".into(), b["seconds"].as_u64().unwrap_or(0).into());
            Event::VoiceCallFinished
        }
        "screenPrepare" => Event::RemoteScreenOpened,
        // Handing the screen back is not a takeover.
        "screenTakeover" if b["on"] == true => Event::ScreenTakenOver,
        "installConnector" => {
            // A catalog name is public; a custom connector's URL or command is not.
            let catalog = b["registryName"].as_str().unwrap_or("custom");
            p.insert("connector".into(), catalog.into());
            Event::ConnectorInstalled
        }
        // A Composio app: counted once its connection is active (`answered`).
        "composioConnection" | "composioConnectFields" => {
            p.insert("connector".into(), "composio".into());
            Event::ConnectorInstalled
        }
        "importConnectors" => Event::ConnectorsImported,
        "importBots" => Event::BotsImported,
        "removeConnector" => Event::ConnectorRemoved,
        "connectorSignIn" => Event::ConnectorSigninStarted,
        "installSkill" => {
            p.insert("custom".into(), (!b["source"].is_string()).into());
            Event::SkillInstalled
        }
        "removeSkill" => Event::SkillRemoved,
        // A bot's `save_skill`: a new skill, or new text for one (`id`).
        "chatCall" if b["name"] == crate::chat::skills::TOOL => {
            let rewrite = crate::chat::skills::rewritten_id(&b["arguments"]).is_some();
            p.insert("change".into(), (if rewrite { "update" } else { "new" }).into());
            Event::SkillWritten
        }
        // A bot's `publish_artifact`: a new artifact, or a new version of one (`artifact`); never its title.
        "chatCall" if b["name"] == "publish_artifact" => {
            let update = b["arguments"]["artifact"].as_str().is_some_and(|id| !id.is_empty());
            p.insert("change".into(), (if update { "update" } else { "new" }).into());
            Event::ArtifactPublished
        }
        "agentSetup" | "agentAuthenticate" => {
            // An agent's catalog id; Node.js setup names none.
            if let Some(agent) = b["backend"].as_str().filter(|a| !a.is_empty()) {
                p.insert("agent".into(), agent.into());
            }
            if method == "agentSetup" { Event::AgentSetupStarted } else { Event::AgentSignedIn }
        }
        "installHostUpdate" => Event::HostUpdateInstalled,
        // Never the computer's name, the connector's address or the invite.
        "networkCreate" | "networkJoin" => {
            p.insert("via".into(), (if method == "networkCreate" { "create" } else { "invite" }).into());
            Event::NetworkJoined
        }
        "pairing" => Event::PairingCodeCreated,
        // Approving is `device_paired`, from `cloud::decide`.
        "decideAccessRequest" if b["approve"] == false => Event::AccessRequestDenied,
        "revokeDevice" => Event::DeviceRevoked,
        _ => return None,
    })
}

/// `observe` plus what only the store knows: a resent message (same `clientNonce`) is
/// not a new one, a message names its bot's agent, an edit the settings it changes, a
/// permission answer the kind of option picked, and a phone its platform.
pub fn observe_call(hub: &Hub, caller: &Caller, method: &str, b: &Value) -> Option<(Event, Map<String, Value>)> {
    let (event, mut p) = observe(caller, method, b)?;
    match event {
        Event::MessageSent => {
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
        Event::BotUpdated => {
            if let Ok(Some(row)) = hub.store.bot(b["id"].as_str().unwrap_or_default()) {
                p.insert("fields".into(), changed_fields(&row.config, b).into());
            }
        }
        Event::PermissionAnswered => {
            let card = hub.store.entry(b["entryId"].as_str().unwrap_or_default());
            p.insert("choice".into(), choice(card.as_ref().map(|e| &e.data), &b["optionId"]).into());
        }
        _ => {}
    }
    if let Caller::Device { key, .. } = caller {
        p.insert("platform".into(), platform_of(hub, key).into());
    }
    Some((event, p))
}

/// What only the answer tells: how many connectors or bots an import added, whether a Composio app is
/// connected yet (its sign-in is polled until the connection is active), and whether a published
/// artifact's card went into its bot's chat. `None`: not an event.
pub fn answered(event: Event, mut p: Map<String, Value>, answer: &Value) -> Option<Map<String, Value>> {
    match event {
        Event::ConnectorsImported => {
            p.insert("count".into(), answer["items"].as_array().map_or(0, Vec::len).into());
        }
        // The file names its app: only the ones Codync reads leave the computer.
        Event::BotsImported => {
            let source = answer["source"].as_str().filter(|s| matches!(*s, "grok-bot" | "openbot"));
            p.insert("source".into(), source.unwrap_or("other").into());
            for key in ["bots", "groups"] {
                p.insert(key.into(), answer[key].as_array().map_or(0, Vec::len).into());
            }
        }
        // A routine run, a room or a bot request gets no card (`card` says why).
        Event::ArtifactPublished => {
            p.insert("card".into(), answer["card"].is_null().into());
        }
        Event::ConnectorInstalled
            if p.get("connector").and_then(Value::as_str) == Some("composio") && answer["status"] != "active" =>
        {
            return None;
        }
        _ => {}
    }
    Some(p)
}

/// What a failed call reports: a `network*` call's failure keeps what it was (`via` or
/// `action`) as `network_failed` with `stage` `action`; other failures report nothing.
pub fn failed(event: Event, mut p: Map<String, Value>) -> Option<(Event, Map<String, Value>)> {
    matches!(event, Event::NetworkJoined | Event::NetworkChanged).then(|| {
        p.insert("stage".into(), "action".into());
        (Event::NetworkFailed, p)
    })
}

/// The bot settings a patch changes, by name (never their values), sorted.
fn changed_fields(stored: &BotConfig, patch: &Value) -> Vec<String> {
    let Ok(Value::Object(before)) = serde_json::to_value(stored) else { return vec![] };
    let mut merged = before.clone();
    for (k, v) in patch.as_object().into_iter().flatten() {
        merged.insert(k.clone(), v.clone());
    }
    // Read back as a bot: unknown keys fall away, and a value written another way (`null` for
    // `false`, `""` for the bot's own workspace) counts as unchanged.
    let Ok(mut after) = serde_json::from_value::<BotConfig>(Value::Object(merged)) else { return vec![] };
    if !after.is_group() && after.cwd.is_empty() {
        after.cwd = crate::agent::workspace::path(&after.id).to_string_lossy().into_owned();
    }
    let Ok(Value::Object(after)) = serde_json::to_value(after) else { return vec![] };
    let mut fields: Vec<String> = patch
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k)
        .filter(|k| !matches!(k.as_str(), "id" | "createdAt") && after.get(*k) != before.get(*k))
        .cloned()
        .collect();
    fields.sort();
    fields
}

/// The kind of option a permission answer picked (ACP's `kind`), from its card.
fn choice(card: Option<&Value>, option: &Value) -> &'static str {
    let kind = card
        .and_then(|c| c["options"].as_array())
        .and_then(|o| o.iter().find(|o| !option.is_null() && o["optionId"] == *option))
        .and_then(|o| o["kind"].as_str());
    match kind {
        Some("allow_once") => "allow_once",
        Some("allow_always") => "allow_always",
        Some("reject_once") => "reject_once",
        Some("reject_always") => "reject_always",
        _ => "other",
    }
}

#[cfg(test)]
mod tests;
