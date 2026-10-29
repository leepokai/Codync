//! GPT-Live control plane. The apps carry WebRTC audio; this host owns tools and session state.

mod prompts;
mod protocol;
#[cfg(test)]
mod tests;
mod transcript;
pub(super) use transcript::continues;

use super::{CallMode, CallState, STATE, Speaker, begun, checked, save, state};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use protocol::{Action, Protocol};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

const MAX_SESSIONS: usize = 32;
const MAX_EVENT_BYTES: usize = 64 * 1024;
type Slot = Arc<AsyncMutex<Session>>;

#[derive(Default)]
pub struct Calls(Mutex<HashMap<String, Slot>>);

#[derive(Default)]
struct Session {
    id: String,
    epoch: i64,
    protocol: Protocol,
}

impl Calls {
    fn slot(&self, hub: &Hub, call: &str) -> Result<Slot> {
        let mut slots = self.0.locked();
        slots.retain(|id, _| super::is_live(&hub.store, id));
        if let Some(slot) = slots.get(call) {
            return Ok(slot.clone());
        }
        if slots.len() >= MAX_SESSIONS {
            bail!("Too many GPT-Live calls are open. End a call first.");
        }
        let slot = Arc::new(AsyncMutex::new(Session::default()));
        slots.insert(call.to_owned(), slot.clone());
        Ok(slot)
    }

    fn get(&self, call: &str) -> Result<Slot> {
        self.0
            .locked()
            .get(call)
            .cloned()
            .ok_or_else(|| anyhow!("This Live session is no longer active. Reconnect the call."))
    }

    fn take(&self, call: &str) -> Option<Slot> {
        self.0.locked().remove(call)
    }
}

/// Ending or switching voice engines retires the provider session even if the frontend vanished.
pub(super) fn end(hub: &Arc<Hub>, call: &str) {
    let Some(slot) = hub.live_calls.take(call) else { return };
    let hub = hub.clone();
    tokio::spawn(async move {
        let mut session = slot.lock().await;
        session.protocol.closed = true;
        if !session.id.is_empty() {
            crate::voice::live::close_abandoned(&hub.store, &session.id).await;
        }
    });
}

fn field<'a>(b: &'a Value, name: &str) -> Result<&'a str> {
    b[name].as_str().ok_or_else(|| anyhow!("`{name}` is required"))
}

fn owned(hub: &Hub, call: &str, owner: &str, session: Option<&str>) -> Result<CallState> {
    let s = state(&hub.store, call).ok_or_else(|| anyhow!("The call has ended."))?;
    if s.owner != owner {
        bail!("Only the device holding this call may control it.");
    }
    if let Some(id) = session
        && s.live_session.as_deref() != Some(id)
    {
        bail!("This Live session was replaced. Reconnect the call.");
    }
    Ok(s)
}

/// Creates the media session with the key kept in the host vault, never returned to the app.
pub async fn start(hub: &Arc<Hub>, b: &Value, owner: &str) -> Result<Value> {
    let bot = field(b, "botId")?;
    let call = checked(field(b, "callId")?)?;
    let cfg = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("Unknown bot."))?.config;
    let model = field(b, "model")?;
    let voice = field(b, "voice")?;
    let sdp = field(b, "sdp")?;
    // Validate caller-supplied fields before opening or registering anything.
    crate::voice::live::request(model, voice, sdp, "", "", &json!([]))?;
    {
        let _one = STATE.locked();
        let mut s = begun(&hub.store, bot, call)?.ok_or_else(|| anyhow!("The call has ended."))?;
        if s.bot != bot {
            bail!("That call belongs to another bot.");
        }
        if !s.owner.is_empty() && s.owner != owner {
            bail!("Only the device holding this call may control it.");
        }
        owner.clone_into(&mut s.owner);
        s.mode = CallMode::Realtime;
        save(&hub.store, call, &s)?;
    }
    let slot = hub.live_calls.slot(hub, call)?;
    let mut session = slot.lock().await;
    // Reserve a new generation before the provider request: old events cannot execute during setup.
    let generation = uuid::Uuid::new_v4().to_string();
    let so_far = {
        let _one = STATE.locked();
        let mut s = owned(hub, call, owner, None)?;
        s.live_session = Some(generation.clone());
        save(&hub.store, call, &s)?;
        s.lines
    };
    if !session.id.is_empty() {
        crate::voice::live::close_abandoned(&hub.store, &session.id).await;
    }
    *session = Session::default();
    crate::market::vault::unlock(hub.clone()).await?;
    let backend = super::context::with_rules(
        hub,
        &cfg,
        Some(crate::voice::Provider::OpenAi),
        b["language"].as_str(),
        &so_far,
        prompts::backend(&cfg),
    )
    .await;
    let body = crate::voice::live::request(
        model,
        voice,
        sdp,
        &prompts::voice(&cfg, &so_far),
        &backend,
        &super::tools::declarations(),
    )?;
    let epoch = now_ms();
    let (id, answer) = crate::voice::live::create(&hub.store, body).await?;
    let adopted = {
        let _one = STATE.locked();
        owned(hub, call, owner, Some(&generation)).and_then(|mut s| {
            s.live_session = Some(id.clone());
            s.last = now_ms();
            save(&hub.store, call, &s)
        })
    };
    if let Err(e) = adopted {
        crate::voice::live::close_abandoned(&hub.store, &id).await;
        return Err(e);
    }
    session.id.clone_from(&id);
    session.epoch = epoch;
    super::announce::watch(hub, call, bot);
    Ok(json!({"sessionId": id, "sdp": answer}))
}

/// Receives provider events in wire order; every emitted event is returned for the same media session.
pub async fn event(hub: &Arc<Hub>, b: &Value, owner: &str) -> Result<Value> {
    let call = checked(field(b, "callId")?)?;
    let id = field(b, "sessionId")?;
    if b["event"].to_string().len() > MAX_EVENT_BYTES {
        bail!("Live event is too large.");
    }
    let slot = hub.live_calls.get(call)?;
    let mut session = slot.lock().await;
    check_session(hub, call, owner, id, &session)?;
    let action = session.protocol.receive(&b["event"])?;
    let events = match action {
        Action::None => vec![],
        Action::Transcript { speaker, text, start_ms, end_ms } => {
            let _one = STATE.locked();
            let mut s = owned(hub, call, owner, Some(id))?;
            s.last = now_ms();
            transcript::append(&mut s, speaker, &text, start_ms, end_ms, session.epoch);
            save(&hub.store, call, &s)?;
            vec![]
        }
        Action::Tools { calls, continue_backend } => {
            let mut out = Vec::new();
            for tool in calls {
                check_session(hub, call, owner, id, &session)?;
                let result = match tool.arguments {
                    Ok(args) => super::tools::call(hub, call, &tool.name, &args, &Value::Null).await,
                    Err(error) => json!({"error": error}),
                };
                check_session(hub, call, owner, id, &session)?;
                out.push(json!({"type": "response.item.create", "event_id": uuid::Uuid::new_v4().to_string(),
                    "item": {"type": "function_call_output", "call_id": tool.id, "output": result.to_string()}}));
            }
            if continue_backend {
                out.push(json!({"type": "response.create", "event_id": uuid::Uuid::new_v4().to_string()}));
            }
            out
        }
    };
    Ok(json!({"events": events}))
}

fn check_session(hub: &Hub, call: &str, owner: &str, id: &str, session: &Session) -> Result<()> {
    owned(hub, call, owner, Some(id))?;
    if session.id != id || session.protocol.closed {
        bail!("This Live session is no longer active.");
    }
    Ok(())
}

/// Bot results and notices are spoken; progress stays quiet and can inform the next answer.
pub async fn update(hub: &Arc<Hub>, b: &Value, owner: &str) -> Result<Value> {
    let call = checked(field(b, "callId")?)?;
    let id = field(b, "sessionId")?;
    let slot = hub.live_calls.get(call)?;
    let session = slot.lock().await;
    check_session(hub, call, owner, id, &session)?;
    let text = field(b, "text")?;
    if text.len() > 32 * 1024 {
        bail!("Live update is too large.");
    }
    Ok(json!({"events": protocol::updates(field(b, "kind")?, text)?}))
}
