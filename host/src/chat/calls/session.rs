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
    let google_search = google_search_enabled(mode, provider, b);
    let mut out = json!({
        "instructions": context::instructions(hub, &cfg, provider, b["language"].as_str(), &so_far, google_search).await,
        "tools": tools::declarations(),
        "googleSearch": google_search,
    });
    if let (Some(p), CallMode::Realtime) = (provider, mode) {
        // The provider's key is in the vault.
        crate::market::vault::unlock(hub.clone()).await?;
        let (model, voice) = (b["model"].as_str().unwrap_or_default(), b["voice"].as_str().unwrap_or_default());
        out["credential"] = crate::voice::session(&hub.store, p, model, voice).await?["credential"].clone();
    }
    Ok(out)
}

/// Search is an opt-in Gemini realtime capability, never a side effect of another engine's preference.
fn google_search_enabled(mode: CallMode, provider: Option<crate::voice::Provider>, body: &Value) -> bool {
    mode == CallMode::Realtime && provider == Some(crate::voice::Provider::Gemini) && body["googleSearch"] == true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::Provider;

    #[test]
    fn search_requires_an_explicit_opt_in_for_gemini_realtime() {
        let hub = context::tests::hub();
        assert_eq!(crate::voice::status(&hub.store).unwrap()["googleSearch"], true);
        for mode in [CallMode::Realtime, CallMode::Speech, CallMode::Device] {
            for provider in [None, Some(Provider::Gemini), Some(Provider::OpenAi)] {
                for requested in [Value::Null, json!(false), json!(true), json!("true")] {
                    let enabled = google_search_enabled(mode, provider, &json!({"googleSearch": requested}));
                    assert_eq!(
                        enabled,
                        mode == CallMode::Realtime && provider == Some(Provider::Gemini) && requested == true
                    );
                }
            }
        }
        assert!(!google_search_enabled(CallMode::Realtime, Some(Provider::Gemini), &json!({})));
    }
}
