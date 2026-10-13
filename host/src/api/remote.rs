//! Phones and accounts: push/activity registration, pairing, devices, cloud access and claims.

use super::devices::Caller;
use super::str_arg;
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::remote::crypto;
use crate::store::DeviceSource;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    Ok(ControlFlow::Break(match method {
        "registerDevice" => {
            let ticket = str_arg(&b, "ticket")?;
            if let Some(relay) = b["relay"].as_str().filter(|r| r.starts_with("https://")) {
                hub.store.kv_set("relay_url", relay.trim_end_matches('/'))?;
            }
            let push_key = b["pushKey"].as_str();
            if let Some(k) = push_key {
                crypto::unb64_n::<32>(k).context("`pushKey` must be a 32-byte X25519 key")?;
            }
            let name = b["name"].as_str().unwrap_or("iPhone");
            hub.store.add_push_ticket(ticket, caller.device_key(), push_key, b["ctx"].as_str(), name)?;
            json!({})
        }
        "unregisterDevice" => {
            hub.store.remove_push_tickets(caller.device_key())?;
            json!({})
        }
        "registerActivity" => {
            let bot_id = str_arg(&b, "botId")?;
            let row = hub.store.bot(bot_id)?.filter(|row| !row.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            hub.store.add_activity_ticket(str_arg(&b, "ticket")?, bot_id, caller.device_key())?;
            // Registration can finish after the task does. Send the current state immediately.
            let state = hub.bot_json(&row);
            let status = match state["status"].as_str() {
                Some("working") => BotStatus::Working,
                Some("needsInput") => BotStatus::NeedsInput,
                Some("error") => BotStatus::Error,
                _ => BotStatus::Idle,
            };
            // A send is acknowledged before its actor necessarily starts. An unanswered user
            // message must not end the activity as "done" during that window.
            let pending = status == BotStatus::Idle
                && hub.store.last_message(bot_id).is_some_and(|message| message.author.is_none());
            if !pending {
                crate::remote::push::live_activity_update(
                    hub,
                    bot_id,
                    &crate::hub::Runtime {
                        status,
                        started_at: state["startedAt"].as_i64(),
                        ..crate::hub::Runtime::default()
                    },
                );
            }
            json!({})
        }
        "pairing" => pairing(hub).await?,
        "devices" => {
            let connected = hub.connected.locked().clone();
            let devices: Vec<Value> = hub
                .store
                .devices()?
                .into_iter()
                .map(|d| {
                    let mut v = serde_json::to_value(&d).expect("Device is plain data and always serializes");
                    v["connected"] = connected.contains_key(&d.key).into();
                    v
                })
                .collect();
            json!({"devices": devices})
        }
        "revokeDevice" => {
            let key = str_arg(&b, "key")?;
            let device = hub.store.device(key).ok_or_else(|| anyhow!("unknown device"))?;
            hub.revoke_device(key)?;
            if let Some(grant) = device.grant_id.filter(|_| device.source == DeviceSource::Account) {
                tokio::spawn(crate::remote::cloud::revoke_grant(hub.clone(), grant));
            }
            json!({})
        }
        "accessRequests" => json!({"requests": hub.cloud.requests_json()}),
        "decideAccessRequest" => {
            let approve = b["approve"].as_bool().ok_or_else(|| anyhow!("`approve` is required"))?;
            crate::remote::cloud::decide(hub, str_arg(&b, "requestId")?, approve).await?;
            json!({})
        }
        "unclaim" => {
            crate::remote::cloud::unclaim(hub).await?;
            json!({})
        }
        "cloudStatus" => serde_json::to_value(hub.cloud.status())?,
        "setCloud" => {
            let enabled = b["enabled"].as_bool().ok_or_else(|| anyhow!("`enabled` is required"))?;
            serde_json::to_value(crate::remote::cloud::set_cloud(hub, enabled)?)?
        }
        "setApproval" => {
            let approval = serde_json::from_value(b["approval"].clone()).context("`approval` is code or auto")?;
            serde_json::to_value(crate::remote::cloud::set_approval(hub, approval)?)?
        }
        "claimSign" => claim_sign(hub, &b).await?,
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}

/// A new one-time pairing code and the QR that carries it (§4.1).
async fn pairing(hub: &Arc<Hub>) -> Result<Value> {
    let port = hub.port;
    // Shells out to `tailscale`: keep it off the async workers.
    let urls = tokio::task::spawn_blocking(move || crate::service::addresses(port)).await?;
    let cloud = crate::remote::cloud::url(&hub.store);
    if urls.is_empty() && cloud.is_none() {
        bail!("No network address a phone could reach, and the Codync cloud is off.");
    }
    let issued = hub.pairing.locked().issue();
    hub.auth_changed();
    let url = crate::service::pairing_url(&crate::service::PairingQr {
        name: &crate::service::host_name(),
        computer_id: &hub.identity.computer_id(),
        sign_key: &hub.identity.sign_pub_b64(),
        box_key: &hub.identity.box_pub_b64(),
        code: &issued.code,
        urls: &urls,
        cloud: cloud.as_deref(),
    });
    Ok(json!({"pairingUrl": url, "urls": urls, "expiresAt": issued.expires_at}))
}

/// Signs this computer into an account claim (§4.2 A) for the signed-in desktop app to complete.
async fn claim_sign(hub: &Arc<Hub>, b: &Value) -> Result<Value> {
    let field = |k: &str| -> Result<&str> {
        let v = str_arg(b, k)?;
        // Fields are newline-separated in the signed string: a newline would forge another field.
        if v.is_empty() || v.contains('\n') {
            bail!("`{k}` is invalid");
        }
        Ok(v)
    };
    let (claim_id, nonce, user_id) = (field("claimId")?, field("nonce")?, field("userId")?);
    let computer_id = hub.identity.computer_id();
    let box_key = hub.identity.box_pub_b64();
    let input = crypto::claim_input(claim_id, nonce, user_id, &computer_id, &box_key);
    Ok(json!({
        "computerId": computer_id,
        "signKey": hub.identity.sign_pub_b64(),
        "boxKey": box_key,
        "name": crate::service::host_name(),
        "platform": std::env::consts::OS,
        "device": tokio::task::spawn_blocking(crate::service::device).await?,
        "version": env!("CARGO_PKG_VERSION"),
        "sig": crypto::b64(&hub.identity.sign(input.as_bytes())),
    }))
}
