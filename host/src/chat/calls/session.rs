//! Starting or replacing a voice provider session for a call.

use super::{CallMode, CallState, STATE, announce, begun, checked, context, save, tools};
use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

/// `startCall` from device `owner`: the call is on (again, for a session the provider replaced: its transcript
/// stays). Hands back what the client needs: the voice's instructions and tools, and a realtime provider's credential.
pub async fn start(hub: &Arc<Hub>, b: &Value, owner: &str) -> Result<Value> {
    let bot = b["botId"].as_str().ok_or_else(|| anyhow!("`botId` is required"))?;
    let call = checked(b["callId"].as_str().unwrap_or_default())?;
    let mode = if b["mode"].is_null() { CallMode::Realtime } else { CallMode::deserialize(&b["mode"])? };
    let cfg = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    let so_far = {
        let _one = STATE.locked();
        match begun(&hub.store, bot, call)? {
            Some(s) if s.bot != bot => bail!("that call belongs to another bot"),
            // A session the provider replaced, maybe on another device or in another mode now.
            Some(s) => {
                let s = CallState { owner: owner.to_owned(), mode, ..s };
                save(&hub.store, call, &s)?;
                s.lines
            }
            None => bail!("that call has ended"),
        }
    };
    announce::watch(hub, call, bot);
    let provider = b["provider"].as_str().map(crate::voice::Provider::parse).transpose()?;
    let mut out = json!({
        "instructions": context::instructions(hub, &cfg, provider, b["language"].as_str(), &so_far).await,
        "tools": tools::declarations(),
    });
    if let (Some(p), CallMode::Realtime) = (provider, mode) {
        // The provider's key is in the vault.
        crate::market::vault::unlock(hub.clone()).await?;
        let (model, voice) = (b["model"].as_str().unwrap_or_default(), b["voice"].as_str().unwrap_or_default());
        out["credential"] = crate::voice::session(&hub.store, p, model, voice).await?["credential"].clone();
    }
    Ok(out)
}
