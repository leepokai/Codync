//! The Gateway connection: Discord pushes each message over it, and the host answers the heartbeats.
//!
//! The connection opens with `IDENTIFY` (the bot token, the message intents) and is reopened with a
//! growing wait whenever it drops or Discord asks. A message is queued before it is acknowledged by
//! any later frame, so a drop right after receiving one loses nothing; the inbox drops duplicates.

use super::{Arc, Config, Context, Hub, LockExt, Result, ingress, load};
use crate::egress::{self, Policy};
use anyhow::{bail, ensure};
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// The intents the host asks for: guilds, guild messages, direct messages, and message content
/// (content is privileged and has to be enabled for the bot in Discord's developer portal).
const INTENTS: u64 = 1 | 512 | 4096 | 32768;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Where Discord's API and Gateway are, and how patiently to reconnect; tests point it at a fake.
pub struct Endpoint {
    pub api: String,
    pub gateway: String,
    pub policy: Policy,
    pub first_wait: Duration,
    pub max_wait: Duration,
    /// A connection that says nothing for this long is dead; Discord heartbeats more often.
    pub idle: Duration,
}

impl Endpoint {
    pub fn discord() -> Self {
        Self {
            api: "https://discord.com/api/v10".into(),
            gateway: "wss://gateway.discord.gg/?v=10&encoding=json".into(),
            policy: Policy::Only(&["discord.com", "gateway.discord.gg"]),
            first_wait: Duration::from_secs(1),
            max_wait: Duration::from_secs(60),
            idle: Duration::from_secs(90),
        }
    }

    pub fn socket_ok(&self, url: &str) -> bool {
        if !matches!(self.policy, Policy::Only(_)) {
            return url.starts_with("ws://") || url.starts_with("wss://");
        }
        url.strip_prefix("wss://")
            .and_then(|rest| rest.split(['/', '?', ':']).next())
            .is_some_and(|host| host == "gateway.discord.gg")
    }
}

pub(super) enum Action {
    Hello(u64),
    Heartbeat,
    Reconnect,
    Ignore,
}

pub(super) async fn run(hub: Arc<Hub>, ep: Endpoint) {
    let mut wait = ep.first_wait;
    let mut session: Option<String> = None;
    let mut sequence: u64 = 0;
    loop {
        match serve(&hub, &ep, &mut wait, &mut session, &mut sequence).await {
            Ok(()) => wait = (wait * 2).min(ep.max_wait),
            Err(error) => {
                set_state(&hub, &format!("error: {}", format!("{error:#}").chars().take(160).collect::<String>()));
                tracing::warn!(error = format!("{error:#}"), "Discord lost its connection");
                wait = (wait * 2).min(ep.max_wait);
            }
        }
        tokio::time::sleep(wait).await;
    }
}

fn set_state(hub: &Hub, state: &str) {
    if let Err(error) = hub.store.kv_set("discord.state", state) {
        tracing::warn!(error = format!("{error:#}"), "Discord state was not saved");
    }
}

/// One connection, until it drops or Discord asks for another.
async fn serve(
    hub: &Arc<Hub>,
    ep: &Endpoint,
    wait: &mut Duration,
    session: &mut Option<String>,
    sequence: &mut u64,
) -> Result<()> {
    if hub.store.kv_read(super::SLOT)?.is_none() {
        tokio::time::sleep(Duration::from_secs(5)).await;
        return Ok(());
    }
    crate::market::vault::unlock(hub.clone()).await?;
    let Some(c) = load(&hub.store)? else { return Ok(()) };
    ensure!(ep.socket_ok(&ep.gateway), "the gateway address is not Discord's");
    egress::check(ep.policy, &ep.gateway.replacen("wss://", "https://", 1)).await?;
    let (mut ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(ep.gateway.as_str()))
        .await
        .context("Discord did not answer in time")??;
    let mut heartbeat: Option<tokio::time::Interval> = None;
    loop {
        let message = if let Some(ticker) = heartbeat.as_mut() {
            tokio::select! {
                _ = ticker.tick() => {
                    send(&mut ws, &json!({"op": 1, "d": *sequence})).await?;
                    continue;
                }
                next = tokio::time::timeout(ep.idle, ws.next()) =>
                    next.context("Discord went silent")?.context("Discord closed")??,
            }
        } else {
            tokio::time::timeout(ep.idle, ws.next())
                .await
                .context("Discord went silent")?
                .context("Discord closed")??
        };
        let frame: Value = match message {
            Message::Text(text) => serde_json::from_str(text.as_str())?,
            Message::Close(_) => return Ok(()),
            _ => continue,
        };
        if let Some(s) = frame["s"].as_u64() {
            *sequence = s;
        }
        match action(&frame) {
            Action::Hello(seconds) => {
                // A dropped connection resumes where it stopped; a refused one identifies again.
                match &*session {
                    Some(id) => {
                        send(
                            &mut ws,
                            &json!({"op": 6, "d": {"token": c.bot_token, "session_id": id, "seq": *sequence}}),
                        )
                        .await?;
                    }
                    None => send(&mut ws, &identify(&c)).await?,
                }
                *wait = ep.first_wait;
                let mut ticker = tokio::time::interval(Duration::from_millis(seconds));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                heartbeat = Some(ticker);
            }
            Action::Heartbeat => send(&mut ws, &json!({"op": 1, "d": *sequence})).await?,
            Action::Reconnect => {
                if frame["op"] == 9 {
                    *session = None;
                }
                return Ok(());
            }
            Action::Ignore => {}
        }
        if frame["op"] == 0 && frame["t"] == "READY" {
            *session = frame["d"]["session_id"].as_str().map(str::to_owned);
            set_state(hub, "ok");
        }
        if frame["op"] == 0 && frame["t"] == "MESSAGE_CREATE" {
            let _configuration = hub.store.connector_lock.locked();
            let Some(current) = load(&hub.store)?.filter(|current| current.generation == c.generation) else {
                return Ok(());
            };
            if let Err(error) = ingress::accept(hub, &current, &frame["d"]) {
                tracing::warn!(error = format!("{error:#}"), "Discord message was not accepted");
            }
        }
    }
}

pub(super) fn action(frame: &Value) -> Action {
    match frame["op"].as_u64() {
        Some(10) => Action::Hello(frame["d"]["heartbeat_interval"].as_u64().unwrap_or(41_250).max(1)),
        Some(7 | 9) => Action::Reconnect,
        Some(1) => Action::Heartbeat,
        _ => Action::Ignore,
    }
}

pub(super) fn identify(c: &Config) -> Value {
    json!({"op": 2, "d": {"token": c.bot_token, "intents": INTENTS,
        "properties": {"os": std::env::consts::OS, "browser": "codync", "device": "codync"}}})
}

async fn send<S>(ws: &mut S, frame: &Value) -> Result<()>
where
    S: futures::Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    ws.send(Message::text(frame.to_string())).await.map_err(|e| anyhow::anyhow!("Discord socket: {e}"))
}

/// Refuses an address Discord did not give.
#[allow(dead_code)]
pub(super) fn _check(url: &str) -> Result<()> {
    if !url.starts_with("wss://gateway.discord.gg") {
        bail!("not a Discord gateway address");
    }
    Ok(())
}
