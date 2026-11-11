use super::config;
use crate::{LockExt, hub::Hub, market::Connector, store::Store};
use anyhow::{Result, ensure};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{KeyInit as _, Mac as _};
use rusqlite::params;
use serde_json::{Value, json};
use sha2::Sha256;
use std::{path::Path as FsPath, sync::Arc};

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

fn signed(headers: &HeaderMap, body: &[u8], secret: &str, now: i64) -> bool {
    let Some(timestamp) = header(headers, "x-codync-timestamp") else { return false };
    let Ok(seconds) = timestamp.parse::<i64>() else { return false };
    if seconds.abs_diff(now) > 300 {
        return false;
    }
    let Some(value) = header(headers, "x-codync-email-signature").and_then(|v| v.strip_prefix("v1=")) else {
        return false;
    };
    if value.len() != 64 {
        return false;
    }
    let bytes =
        (0..64).step_by(2).map(|i| u8::from_str_radix(value.get(i..i + 2)?, 16).ok()).collect::<Option<Vec<_>>>();
    let Some(bytes) = bytes else { return false };
    let Ok(mut mac) = hmac::Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else { return false };
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&bytes).is_ok()
}

pub(crate) async fn receive(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if body.is_empty() || body.len() > 20 * 1024 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let result = (|| -> Result<StatusCode> {
        let _guard = hub.store.connector_lock.locked();
        let Some(c) = config::current(&hub.store, &id)? else { return Ok(StatusCode::NOT_FOUND) };
        if config::kind(&c) != Some("email") {
            return Ok(StatusCode::NOT_FOUND);
        }
        let Some(secret) = c.env.get("CODYNC_CHANNEL_WEBHOOK_SECRET").filter(|s| s.len() >= 32) else {
            return Ok(StatusCode::UNAUTHORIZED);
        };
        if !signed(&headers, &body, secret, crate::store::now_ms() / 1000) {
            return Ok(StatusCode::UNAUTHORIZED);
        }
        let Some(generation) = config::generation(&hub.store, &c)? else { return Ok(StatusCode::SERVICE_UNAVAILABLE) };
        let event = config::digest(&format!("{}:{generation}:{}", c.id, config::digest(&STANDARD.encode(&body))));
        hub.store.with_connection(|db| {
            let count:i64=db.query_row("SELECT count(*) FROM native_channel_webhooks WHERE connection=?1 AND state<>'done'",[&id],|r|r.get(0))?;
            ensure!(count<32,"Email webhook queue is full");
            db.execute("INSERT INTO native_channel_webhooks(id,connection,generation,raw,created_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO NOTHING",params![event,id,generation,STANDARD.encode(&body),crate::store::now_ms()])?;
            Ok(())
        })?;
        Ok(StatusCode::ACCEPTED)
    })();
    result.unwrap_or(StatusCode::SERVICE_UNAVAILABLE).into_response()
}

pub(super) async fn pending(store: &Store, c: &Connector, generation: &str, cache: &FsPath) -> Result<Vec<Value>> {
    if config::kind(c) != Some("email") {
        return Ok(vec![]);
    }
    let pending=store.with_connection(|db| {
        let mut q=db.prepare("SELECT id,raw FROM native_channel_webhooks WHERE connection=?1 AND generation=?2 AND state='queued' ORDER BY created_at LIMIT 2")?;
        Ok(q.query_map(params![c.id,generation],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut commands = Vec::new();
    for (id, raw) in pending {
        let bytes = STANDARD.decode(raw)?;
        ensure!(bytes.len() <= 20 * 1024 * 1024, "Email webhook exceeded its limit");
        let path = cache.join(format!("webhook-{id}.eml"));
        tokio::fs::write(&path, bytes).await?;
        let claimed = store.with_connection(|db| {
            Ok(db
                .execute("UPDATE native_channel_webhooks SET state='claimed' WHERE id=?1 AND state='queued'", [&id])?
                == 1)
        })?;
        if claimed {
            commands.push(json!({"op":"webhook","id":id,"path":path}));
        }
    }
    Ok(commands)
}

pub(super) fn completed(store: &Store, id: &str, success: bool) -> Result<()> {
    store.with_connection(|db| {
        if success {
            db.execute("UPDATE native_channel_webhooks SET raw='',state='done' WHERE id=?1 AND state='claimed'", [id])?;
        } else {
            db.execute("UPDATE native_channel_webhooks SET state='queued' WHERE id=?1 AND state='claimed'", [id])?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn email_webhook_requires_fresh_exact_body_and_single_headers() {
        let mut mac = hmac::Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(b"1000.");
        mac.update(b"raw email");
        let signature = format!("v1={}", crate::chat::files::digest_hex(&mac.finalize().into_bytes()));
        let mut headers = HeaderMap::new();
        headers.insert("x-codync-timestamp", "1000".parse().unwrap());
        headers.insert("x-codync-email-signature", signature.parse().unwrap());
        assert!(signed(&headers, b"raw email", "secret", 1000));
        assert!(!signed(&headers, b"edited", "secret", 1000));
        assert!(!signed(&headers, b"raw email", "secret", 1301));
        headers.append("x-codync-timestamp", "1000".parse().unwrap());
        assert!(!signed(&headers, b"raw email", "secret", 1000));
    }
}
