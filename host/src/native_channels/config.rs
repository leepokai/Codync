use crate::{LockExt, hub::Hub, market::Connector, store::Store};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) const COMMAND: &str = "codync-hermes-channel";
pub(super) const PREFIX: &str = "hermes-channel-";

pub(super) fn kind(c: &Connector) -> Option<&str> {
    let kind = c.catalog.as_deref()?.strip_prefix(PREFIX)?;
    ["signal", "email", "homeassistant"].contains(&kind).then_some(kind)
}

pub(super) fn fingerprint(c: &Connector) -> String {
    digest(&serde_json::to_string(c).unwrap_or_default())
}

pub(super) fn digest(value: &str) -> String {
    crate::chat::files::digest_hex(&Sha256::digest(value.as_bytes()))
}

pub(super) fn current(store: &Store, id: &str) -> Result<Option<Connector>> {
    Ok(crate::market::connectors(store)?.into_iter().find(|c| c.id == id && kind(c).is_some()))
}

pub(super) fn matches(store: &Store, c: &Connector) -> Result<bool> {
    Ok(current(store, &c.id)?.is_some_and(|now| fingerprint(&now) == fingerprint(c)))
}

pub(super) fn state(store: &Store, id: &str) -> Result<Option<Value>> {
    store
        .kv_read(&format!("native-channel.state:{id}"))?
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

pub(super) fn generation(store: &Store, c: &Connector) -> Result<Option<String>> {
    Ok(state(store, &c.id)?
        .filter(|v| v["base"] == fingerprint(c))
        .and_then(|v| v["generation"].as_str().map(str::to_owned)))
}

pub(super) fn save_state(store: &Store, c: &Connector, generation: &str, status: &str) -> Result<()> {
    let _guard = store.connector_lock.locked();
    if !matches(store, c)? {
        return Ok(());
    }
    let mut value = state(store, &c.id)?
        .filter(|v| v["base"] == fingerprint(c) && v["generation"] == generation)
        .unwrap_or_else(|| json!({}));
    value["base"] = fingerprint(c).into();
    value["generation"] = generation.into();
    value["status"] = status.into();
    value["updatedAt"] = crate::store::now_ms().into();
    store.kv_set(&format!("native-channel.state:{}", c.id), &value.to_string())
}

pub(super) fn bot(hub: &Hub, c: &Connector) -> Result<String> {
    let configured = c.env.get("CODYNC_CHANNEL_BOT").map(|s| s.trim()).filter(|s| !s.is_empty());
    let bots =
        hub.store.bots()?.into_iter().filter(|b| {
            !b.deleted && !b.config.is_group() && !b.config.hidden && !crate::external::is_external(&b.config)
        });
    let candidates: Vec<_> = bots
        .filter(|b| {
            configured.map_or_else(
                || b.config.connectors.contains(&c.id),
                |id| b.config.id == id && b.config.connectors.contains(&c.id),
            )
        })
        .collect();
    ensure!(candidates.len() == 1, "Choose exactly one bot for this messaging connection");
    Ok(candidates[0].config.id.clone())
}

pub(super) fn native_users(c: &Connector, state: &Value) -> Vec<String> {
    if kind(c) == Some("homeassistant") {
        return vec!["homeassistant".into()];
    }
    // The transport resolves native per-profile and global policy after applying overrides.
    let users: Vec<String> =
        state["nativeUsers"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect();
    if kind(c) == Some("email") { users.into_iter().map(|s| s.to_lowercase()).collect() } else { users }
}

pub(super) fn split(value: &str) -> Vec<String> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty()).take(100).map(str::to_owned).collect()
}

pub(super) fn authorized(hub: &Hub, c: &Connector, generation: &str, user: &str) -> Result<bool> {
    if !matches(&hub.store, c)? || self::generation(&hub.store, c)?.as_deref() != Some(generation) {
        return Ok(false);
    }
    if let Some(allowed) = hub.store.native_user_decision(&c.id, generation, user)? {
        return Ok(allowed);
    }
    let state = state(&hub.store, &c.id)?.context("Native channel identity is not ready")?;
    Ok(native_users(c, &state).iter().any(|item| item == user || item == "*"))
}

pub(crate) fn prepare(c: &mut Connector) -> Result<()> {
    let Some(platform) = kind(c).map(str::to_owned) else { return Ok(()) };
    ensure!(
        c.command.as_deref() == Some(COMMAND) && c.url.is_none() && c.args.is_empty(),
        "Native messaging apps use the host-owned transport"
    );
    for (name, value) in &c.env {
        ensure!(name.len() <= 80 && value.len() <= 8192 && !value.contains('\0'), "Invalid native channel setting");
        ensure!(
            name.starts_with("CODYNC_CHANNEL_")
                || name.starts_with("SIGNAL_")
                || name.starts_with("EMAIL_")
                || name.starts_with("HASS_"),
            "Unexpected native messaging field"
        );
    }
    if platform == "signal" {
        if let Some(url) = c.env.get("SIGNAL_HTTP_URL").filter(|s| !s.trim().is_empty()) {
            url_allowed(url)?;
        }
    }
    if platform == "homeassistant" {
        if let Some(url) = c.env.get("HASS_URL").filter(|s| !s.trim().is_empty()) {
            url_allowed(url)?;
        }
        ensure!(
            c.env.get("HASS_WATCH_ENTITIES").is_some_and(|s| !split(s).is_empty()),
            "Choose Home Assistant entities explicitly"
        );
    }
    if let Some(code) = c.env.get("CODYNC_CHANNEL_PAIR_CODE").filter(|s| !s.is_empty()) {
        ensure!(
            (12..=64).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric()),
            "Pairing code must contain 12–64 letters or digits"
        );
    }
    if let Some(secret) = c.env.get("CODYNC_CHANNEL_WEBHOOK_SECRET").filter(|s| !s.is_empty()) {
        ensure!(secret.len() >= 32, "Email webhook secret must contain at least 32 characters");
    }
    c.registry_name = Some(format!("codync-native/{platform}"));
    Ok(())
}

fn url_allowed(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)?;
    ensure!(
        ["http", "https"].contains(&url.scheme())
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "Use a native service HTTP(S) URL without embedded credentials"
    );
    Ok(())
}

pub(crate) fn claim_prompt(hub: &Hub, id: &str) -> Result<bool> {
    let _guard = hub.store.connector_lock.locked();
    let Some(entry) = hub.store.entry_checked(id)? else { return Ok(false) };
    let f = &entry.data["nativeFence"];
    let (Some(connection), Some(expected), Some(event), Some(user)) =
        (f["connection"].as_str(), f["generation"].as_str(), f["event"].as_str(), f["user"].as_str())
    else {
        return Ok(false);
    };
    let Some(c) = current(&hub.store, connection)? else { return Ok(false) };
    if generation(&hub.store, &c)?.as_deref() != Some(expected)
        || !authorized(hub, &c, expected, user)?
        || bot(hub, &c).ok().as_deref() != Some(entry.bot_id.as_str())
    {
        return Ok(false);
    }
    let ready = state(&hub.store, connection)?.is_some_and(|s| s["status"] == "ready");
    ensure!(ready, "Native channel is reconnecting; the queued message is deferred");
    hub.store.native_claim_prompt(connection, expected, event, id)
}
