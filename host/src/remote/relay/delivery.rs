//! What comes in through the relay one item at a time: public routine webhooks (§7.8) and queued mailbox sends
//! (§6.4, §7.7).

use crate::api;
use crate::api::devices::{self, Caller};
use crate::hub::Hub;
use crate::remote::crypto;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

/// A mailbox send older than this is refused (TTL 24 h + clock slack).
const MAILBOX_MAX_AGE_MS: i64 = 25 * 60 * 60 * 1000;

/// Runs one public webhook delivery and says how it went:
/// `{"t":"hook.ack","seq","ok"}`, plus `retry` (try again later) and `code` when refused.
pub(super) fn hook_item(hub: &Arc<Hub>, item: &Value) -> Value {
    let seq = item["seq"].clone();
    let hook = item["hook"].as_str().unwrap_or_default();
    let headers = item["headers"]
        .as_object()
        .map(|h| h.iter().filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?.to_owned()))).collect())
        .unwrap_or_default();
    let Some(body) = item["body"].as_str().and_then(|b| crypto::unb64(b).ok()) else {
        return json!({"t": "hook.ack", "seq": seq, "ok": false, "code": "invalid"});
    };
    match hub.routines.receive(hub, hook, &headers, &body) {
        Ok(_) => json!({"t": "hook.ack", "seq": seq, "ok": true}),
        Err(e) => {
            tracing::info!(hook, code = e.code(), reason = %e, "refused a webhook delivery");
            json!({"t": "hook.ack", "seq": seq, "ok": false, "retry": e.retry(), "code": e.code()})
        }
    }
}

// MARK: mailbox (§6.4, §7.7)

/// Why a queued send was refused (`mbox.ack.code`).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum MailboxFailure {
    Unauthorized,
    UnknownBot,
    Invalid,
}

/// Delivers one queued send and says how it went: `{"t":"mbox.ack","seq","ok","code"?}`.
pub(super) async fn mailbox_item(hub: &Arc<Hub>, item: &Value) -> Value {
    let seq = item["seq"].clone();
    match deliver(hub, item).await {
        Ok(()) => json!({"t": "mbox.ack", "seq": seq, "ok": true}),
        Err(code) => {
            tracing::info!(?code, "refused a queued message");
            json!({"t": "mbox.ack", "seq": seq, "ok": false, "code": code})
        }
    }
}

async fn deliver(hub: &Arc<Hub>, item: &Value) -> Result<(), MailboxFailure> {
    use MailboxFailure::{Invalid, Unauthorized, UnknownBot};
    let from = item["from"].as_str().unwrap_or_default();
    let device = devices::authorize(hub, from).map_err(|_| Unauthorized)?;
    let dk = crypto::unb64_n::<32>(from).map_err(|_| Invalid)?;
    let nonce = item["nonce"].as_str().filter(|n| !n.is_empty()).ok_or(Invalid)?;
    let blob = item["d"].as_str().and_then(|d| crypto::unb64(d).ok()).ok_or(Invalid)?;
    let plain = crypto::open_mailbox(&hub.identity.box_secret, &hub.identity.cid_raw(), &dk, nonce, &blob)
        .map_err(|_| Invalid)?;
    let inner: Value = serde_json::from_slice(&plain).map_err(|_| Invalid)?;
    let b = &inner["b"];
    let fresh = inner["ts"].as_i64().is_some_and(|ts| crate::store::now_ms() - ts <= MAILBOX_MAX_AGE_MS);
    if inner["m"] != "send" || b["clientNonce"].as_str() != Some(nonce) || !fresh {
        return Err(Invalid);
    }
    let bot = b["botId"].as_str().ok_or(Invalid)?;
    if hub.store.bot(bot).ok().flatten().is_none_or(|r| r.deleted) {
        return Err(UnknownBot);
    }
    let caller = Caller::Device { key: device.key, scopes: device.scopes };
    // `send` skips a clientNonce it already has: a redelivery never runs twice.
    api::dispatch(hub, &caller, "send", b.clone())
        .await
        .map(drop)
        .map_err(|e| if e.is::<devices::Forbidden>() { Unauthorized } else { Invalid })
}
