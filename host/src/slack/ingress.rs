//! Verify the raw request before parsing or acknowledging an event.
use super::{Arc, Config, Context, Hub, LockExt, Result, Value, json, load};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use hmac::{KeyInit as _, Mac as _};
use sha2::{Digest, Sha256};

fn one<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    let mut values = headers.get_all(key).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}
fn authorized(secret: &str, headers: &HeaderMap, body: &[u8], now: i64) -> bool {
    let Some(timestamp) = one(headers, "x-slack-request-timestamp") else { return false };
    let Ok(seconds) = timestamp.parse::<i64>() else { return false };
    if seconds.abs_diff(now) > 300 {
        return false;
    }
    let Some(hex) = one(headers, "x-slack-signature").and_then(|s| s.strip_prefix("v0=")) else { return false };
    if hex.len() != 64 {
        return false;
    }
    let signature: Option<Vec<u8>> =
        (0..64).step_by(2).map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect();
    let Some(signature) = signature else { return false };
    let Ok(mut mac) = hmac::Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else { return false };
    mac.update(format!("v0:{timestamp}:").as_bytes());
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}
pub(super) fn stable_id(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    let hash = digest.finalize();
    format!("slack-{}", crate::chat::files::digest_hex(&hash))
}
pub(crate) async fn receive(State(hub): State<Arc<Hub>>, headers: HeaderMap, body: Bytes) -> Response {
    if body.len() > 256 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let result = (|| -> Result<Response> {
        let _configuration = hub.store.connector_lock.locked();
        let Some(c) = load(&hub.store)? else { return Ok(StatusCode::UNAUTHORIZED.into_response()) };
        // An empty secret would verify anything: with Socket Mode only, this endpoint is closed.
        if c.signing_secret.is_empty() || !authorized(&c.signing_secret, &headers, &body, crate::store::now_ms() / 1000)
        {
            return Ok(StatusCode::UNAUTHORIZED.into_response());
        }
        let envelope: Value = serde_json::from_slice(&body)?;
        if envelope["type"] == "url_verification" {
            let challenge = envelope["challenge"].as_str().filter(|s| s.len() <= 256).context("Invalid challenge")?;
            return Ok(Json(json!({"challenge":challenge})).into_response());
        }
        accept(&hub, &c, &envelope)?;
        Ok(StatusCode::OK.into_response())
    })();
    match result {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error,"Slack delivery could not be accepted");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
/// Queues the prompt in one Slack `event_callback`, whether it came signed over HTTP or through Socket Mode. An event
/// that is not for a bound bot, from an allowed person, in a conversation the bot is in, is dropped without error.
pub(super) fn accept(hub: &Hub, c: &Config, envelope: &Value) -> Result<()> {
    if envelope["type"] != "event_callback" || envelope["team_id"] != c.workspace_id {
        return Ok(());
    }
    let event = &envelope["event"];
    if event.get("bot_id").is_some() || event.get("subtype").is_some() || event["user"] == c.bot_user_id {
        return Ok(());
    }
    let Some(binding) = c.bindings.iter().find(|b| {
        event["channel"] == b.channel_id
            && event["user"].as_str().is_some_and(|u| b.allowed_users.iter().any(|v| v == u))
    }) else {
        return Ok(());
    };
    hub.store.bot(&binding.bot_id)?.filter(|b| !b.deleted && !b.config.is_group()).context("Bound bot unavailable")?;
    let event_id =
        envelope["event_id"].as_str().filter(|s| !s.is_empty() && s.len() <= 100).context("Missing event id")?;
    let ts = event["ts"].as_str().filter(|s| valid_ts(s)).context("Missing message timestamp")?;
    let thread = event["thread_ts"].as_str().filter(|s| valid_ts(s)).unwrap_or(ts);
    let mention = event["type"] == "app_mention";
    let direct = event["type"] == "message" && event["channel_type"] == "im";
    let reply = event["type"] == "message"
        && thread != ts
        && hub.store.slack_thread_exists(&c.generation, &binding.channel_id, thread)?;
    if !mention && !direct && !reply {
        return Ok(());
    }
    let message = normalize(c, event, &binding.channel_id, thread).context("Missing user")?;
    if message.text.is_empty() || message.text.len() > 32 * 1024 {
        return Ok(());
    }
    let incoming = crate::store::slack::SlackIncoming {
        event: event_id.into(),
        generation: c.generation.clone(),
        bot: binding.bot_id.clone(),
        channel: binding.channel_id.clone(),
        thread: thread.into(),
        entry: stable_id(&[&c.generation, &binding.channel_id, ts]),
        text: message.prompt(),
        user: message.person,
    };
    hub.store.enqueue_slack(&incoming)?;
    let root = stable_id(&[&c.generation, &binding.channel_id, thread, "root"]);
    hub.store.map_slack_thread(&incoming, &root)?;
    Ok(())
}

/// A Slack message event in the shape every channel shares: the bot's own mention is not part of what was said.
fn normalize(c: &Config, event: &Value, channel: &str, thread: &str) -> Option<crate::channels::Incoming> {
    let files = event["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["name"].as_str().filter(|n| n.len() <= 255))
        .map(|name| crate::channels::FileRef { name: name.to_owned() })
        .take(10)
        .collect();
    Some(crate::channels::Incoming {
        channel: channel.into(),
        person: event["user"].as_str()?.into(),
        thread: thread.into(),
        text: event["text"]
            .as_str()
            .unwrap_or_default()
            .replace(&format!("<@{}>", c.bot_user_id), "")
            .trim()
            .to_owned(),
        files,
    })
}

fn valid_ts(value: &str) -> bool {
    value.len() <= 32 && value.contains('.') && value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verifies_exact_body_and_fresh_timestamp_and_rejects_duplicate_headers() {
        let body = b"{\"text\":\"hello\"}";
        let mut mac = hmac::Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(b"v0:1000:");
        mac.update(body);
        let sig = format!("v0={}", crate::chat::files::digest_hex(&mac.finalize().into_bytes()));
        let mut headers = HeaderMap::new();
        headers.insert("x-slack-request-timestamp", "1000".parse().unwrap());
        headers.insert("x-slack-signature", sig.parse().unwrap());
        assert!(authorized("secret", &headers, body, 1000));
        assert!(!authorized("secret", &headers, b"{}", 1000));
        assert!(!authorized("secret", &headers, body, 1301));
        headers.append("x-slack-request-timestamp", "1000".parse().unwrap());
        assert!(!authorized("secret", &headers, body, 1000));
    }
}
