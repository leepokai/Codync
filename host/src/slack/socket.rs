//! Slack Socket Mode: the host opens the connection to Slack, so no public HTTPS endpoint is needed.
//!
//! `apps.connections.open` (with the app-level `xapp-` token) answers a `wss` address; Slack then pushes each event
//! as an envelope over it, and wants the envelope acknowledged. An event is acknowledged only after it was queued, so
//! one that failed is delivered again; the inbox drops one it already has. The connection is reopened with a growing
//! wait, and whenever Slack asks (`disconnect`) or the configuration changes.

use super::{Arc, Hub, LockExt, Result, SLOT, Value, ingress, load};
use crate::egress::{self, Policy};
use anyhow::Context;
use futures::{SinkExt as _, StreamExt as _};
use serde_json::json;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// A connection that says nothing for this long is dead: Slack pings more often.
const READ_IDLE: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Without an app token there is nothing to connect: look again for a new configuration this often.
const UNCONFIGURED_POLL: Duration = Duration::from_secs(5);

/// Where Slack's API is and how patiently to reconnect; tests point it at a fake.
pub(super) struct Endpoint {
    pub api: String,
    pub policy: Policy,
    pub first_wait: Duration,
    pub max_wait: Duration,
}

impl Endpoint {
    pub(super) fn slack() -> Self {
        Self {
            api: "https://slack.com/api".into(),
            policy: Policy::Only(&["slack.com"]),
            first_wait: Duration::from_secs(1),
            max_wait: Duration::from_secs(60),
        }
    }

    /// Slack's own sockets are `wss` on its domain; only a test endpoint may use anything else.
    pub(super) fn socket_ok(&self, url: &str) -> bool {
        if !matches!(self.policy, Policy::Only(_)) {
            return url.starts_with("ws://") || url.starts_with("wss://");
        }
        url.strip_prefix("wss://")
            .and_then(|rest| rest.split(['/', '?', ':']).next())
            .is_some_and(|host| host.ends_with(".slack.com"))
    }
}

/// What one frame from Slack asks of the connection.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Frame {
    Hello,
    /// The event was queued: tell Slack, with this envelope id.
    Ack(String),
    /// Slack is moving the connection, or the configuration it was opened for is gone.
    Reconnect,
    Ignore,
}

pub(super) async fn run(hub: Arc<Hub>, ep: Endpoint) {
    let mut wait = ep.first_wait;
    loop {
        match serve(&hub, &ep, &mut wait).await {
            Ok(false) => {
                tokio::time::sleep(UNCONFIGURED_POLL).await;
                continue;
            }
            Ok(true) => {}
            Err(error) => tracing::warn!(error = format!("{error:#}"), "Slack Socket Mode lost its connection"),
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(ep.max_wait);
    }
}

/// One connection, until it ends. False when there is no app token to connect with.
async fn serve(hub: &Arc<Hub>, ep: &Endpoint, wait: &mut Duration) -> Result<bool> {
    if hub.store.kv_read(SLOT)?.is_none() {
        return Ok(false);
    }
    crate::market::vault::unlock(hub.clone()).await?;
    let Some(c) = load(&hub.store)?.filter(|c| !c.app_token.is_empty()) else { return Ok(false) };
    let url = open(ep, &c.app_token).await?;
    let (mut ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url.as_str()))
        .await
        .context("Slack did not answer in time")??;
    loop {
        let next = tokio::time::timeout(READ_IDLE, ws.next()).await.context("Slack went silent")?;
        let Some(message) = next else { return Ok(true) };
        match message? {
            Message::Text(text) => match handle(hub, &c.generation, &text)? {
                Frame::Hello => *wait = ep.first_wait,
                Frame::Ack(id) => ws.send(Message::text(json!({"envelope_id": id}).to_string())).await?,
                Frame::Reconnect => return Ok(true),
                Frame::Ignore => {}
            },
            Message::Close(_) => return Ok(true),
            _ => {}
        }
    }
}

/// Asks Slack for a socket address to connect to.
async fn open(ep: &Endpoint, app_token: &str) -> Result<String> {
    let url = format!("{}/apps.connections.open", ep.api);
    let client = egress::client(ep.policy, &url).await?;
    let response = client.post(&url).bearer_auth(app_token).timeout(CONNECT_TIMEOUT).send().await?;
    anyhow::ensure!(response.status().is_success(), "Slack answered {}", response.status());
    let body = egress::json(response, 64 * 1024).await?;
    anyhow::ensure!(body["ok"] == true, "Slack refused the app token: {}", body["error"].as_str().unwrap_or("unknown"));
    let socket = body["url"].as_str().context("Slack sent no socket address")?;
    anyhow::ensure!(ep.socket_ok(socket), "Slack sent an address that is not its own");
    Ok(socket.to_owned())
}

pub(super) fn handle(hub: &Hub, generation: &str, text: &str) -> Result<Frame> {
    let frame: Value = serde_json::from_str(text)?;
    Ok(match frame["type"].as_str() {
        Some("hello") => Frame::Hello,
        Some("disconnect") => Frame::Reconnect,
        Some("events_api") => {
            let id = frame["envelope_id"].as_str().filter(|s| !s.is_empty() && s.len() <= 100);
            let id = id.context("An event came without an envelope id")?;
            let _configuration = hub.store.connector_lock.locked();
            let Some(c) = load(&hub.store)?.filter(|c| c.generation == generation) else { return Ok(Frame::Reconnect) };
            ingress::accept(hub, &c, &frame["payload"])?;
            Frame::Ack(id.to_owned())
        }
        _ => Frame::Ignore,
    })
}
