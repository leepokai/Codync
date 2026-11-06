//! The outbound WebSocket to this computer's relay in the Codync cloud (spec §7, §9.6).
//!
//! Every connection runs the same order: register (once per URL) → connect with
//! `Codync-Sig` → pull the cloud state (≤ 10 s; account devices wait for it; a private relay
//! has none) → publish the signed ACL → `acl.ok` → `ready`. Then device links are multiplexed
//! onto `channel::run` (the relay only ever sees ciphertext), queued mailbox sends are
//! delivered in order and acknowledged, and every change to the device table republishes the
//! ACL. Once ready, the routine webhooks are registered (and again whenever they change) and
//! public webhook deliveries arrive one at a time for an ack (§7.8). A reconnect drops all link
//! state.

use crate::LockExt;
use crate::hub::Hub;
use crate::remote::channel::{self, ChannelKind, Out, Transport};
use crate::remote::crypto::{self, b64};
use crate::remote::{cloud, owner};
use crate::store::DeviceSource;
use anyhow::{Context, Result, anyhow, bail};
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep, timeout};
use tokio_stream::StreamMap;
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest as _};
use tracing::Instrument as _;

mod delivery;
use delivery::{hook_item, mailbox_item};

const PATH: &str = "/v1/relay/host?v=1";
/// Must be exactly this string: the relay answers it without waking up.
const PING: &str = r#"{"t":"ping"}"#;
const PING_EVERY: Duration = Duration::from_secs(30);
/// Nothing heard for this long (not even a pong): the socket is dead.
const SILENT: Duration = Duration::from_secs(90);
const STATE_WAIT: Duration = Duration::from_secs(10);
const STATE_EVERY: Duration = Duration::from_mins(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const STABLE: Duration = Duration::from_mins(1);
const REPLACED_WAIT: Duration = Duration::from_mins(5);
/// A device's messages waiting for its channel; the relay already rate-limits devices.
const LINK_BUFFER: usize = 256;
const REPLACED: u16 = 4009;
const RATE_LIMITED: u16 = 4008;

/// How long the old relay stays up after the cloud is reconfigured, so phones that reach this
/// computer through it can reconnect once and learn the new address.
const MOVE_GRACE: Duration = Duration::from_secs(10);

/// Keeps this computer on the relay while the cloud is on; restarts when it's reconfigured.
pub async fn run(hub: Arc<Hub>) {
    let mut config = hub.cloud.config.subscribe();
    loop {
        config.mark_unchanged();
        let address = cloud::current(&hub.store);
        cloud::update_status(&hub, |s| {
            cloud::show(s, address.as_ref());
            s.approval = cloud::approval(&hub.store);
            s.assistants = crate::chat::team::assistant::enabled(&hub.store);
            s.registered = false;
            s.connected = false;
            if address.is_none() {
                s.owner = None;
                s.last_error = None;
            } else if s.owner.is_none() {
                // Until the cloud answers (or while it can't be reached), the account last seen.
                s.owner = owner::owner(&hub.store);
            }
        });
        let restart = if let Some(address) = address {
            let cloud_host = cloud::authority(&address.url).unwrap_or_else(|_| "unknown".into());
            let session = supervise(&hub, &address).instrument(tracing::info_span!("relay", cloud_host));
            tokio::pin!(session);
            tokio::select! {
                () = &mut session => Ok(()),
                r = config.changed() => {
                    // Every channel reconnects on this change (`channel::serve`) and reads the new
                    // address from `hello`. Phones on this relay need it to stay up while they do.
                    let _ = timeout(MOVE_GRACE, &mut session).await;
                    r
                }
            }
        } else {
            // No cloud: account devices can't be renewed, so they aren't honored either.
            hub.cloud_synced.store(false, Ordering::Release);
            config.changed().await
        };
        hub.cloud_synced.store(false, Ordering::Release);
        if restart.is_err() {
            return;
        }
    }
}

/// Exponential backoff with full jitter.
struct Backoff {
    next: Duration,
    max: Duration,
}

impl Backoff {
    fn new(max: Duration) -> Self {
        Self { next: Duration::from_secs(1), max }
    }

    fn wait(&mut self) -> Duration {
        let cap = self.next;
        self.next = (self.next * 2).min(self.max);
        let r = u64::from_le_bytes(crypto::random());
        let cap_ms = u64::try_from(cap.as_millis()).unwrap_or(u64::MAX);
        Duration::from_millis(r % (cap_ms + 1)).max(Duration::from_millis(250))
    }

    fn reset(&mut self) {
        self.next = Duration::from_secs(1);
    }
}

async fn supervise(hub: &Arc<Hub>, address: &cloud::Address) {
    let mut register_wait = Backoff::new(Duration::from_mins(30));
    let mut wait = Backoff::new(Duration::from_mins(1));
    let mut registered = false;
    loop {
        if !registered {
            if let Err(e) = cloud::register(hub, address).await {
                tracing::warn!(error = format!("{e:#}"), "couldn't register with the Codync cloud");
                cloud::update_status(hub, |s| s.last_error = Some(format!("{e:#}")));
                sleep(register_wait.wait()).await;
                continue;
            }
            registered = true;
            register_wait.reset();
        }
        if !owner::wants_relay(hub, address) {
            without_socket(hub, address).await;
            continue;
        }
        let started = Instant::now();
        let outcome = session(hub, address).await;
        cloud::update_status(hub, |s| s.connected = false);
        if !owner::wants_relay(hub, address) {
            continue;
        }
        match outcome {
            Ok(Some(REPLACED)) => {
                tracing::error!(
                    computer_id = hub.identity.computer_id(),
                    "another host took over this computer's relay; was identity.json copied to another machine?"
                );
                cloud::update_status(hub, |s| s.last_error = Some("Another computer is using this identity.".into()));
                sleep(REPLACED_WAIT).await;
                continue;
            }
            Ok(code) => tracing::info!(code, "relay connection closed"),
            Err(e) => {
                // The relay refused the upgrade: maybe the cloud forgot us; register again.
                if let Some(tungstenite::Error::Http(res)) = e.downcast_ref::<tungstenite::Error>() {
                    registered = !res.status().is_client_error();
                }
                tracing::warn!(error = format!("{e:#}"), "relay connection failed");
                cloud::update_status(hub, |s| s.last_error = Some(format!("{e:#}")));
            }
        }
        if started.elapsed() >= STABLE {
            wait.reset();
        }
        sleep(wait.wait()).await;
    }
}

/// A computer in no account holds no socket on the Codync relay: it reads its state now and then, so it notices a
/// claim completed elsewhere, until something needs the relay (`owner::wants_relay`).
async fn without_socket(hub: &Arc<Hub>, address: &cloud::Address) {
    while !owner::wants_relay(hub, address) {
        match cloud::pull(hub, &address.url).await {
            Ok(()) => hub.cloud_synced.store(true, Ordering::Release),
            Err(e) => tracing::warn!(error = format!("{e:#}"), "couldn't get the cloud state"),
        }
        if owner::wants_relay(hub, address) {
            return;
        }
        let _ = timeout(STATE_EVERY, hub.cloud.relay_wake.notified()).await;
    }
}

/// `https://x` → `wss://x/v1/relay/host?v=1`.
fn relay_url(base: &str) -> Result<String> {
    let rest = base
        .strip_prefix("https://")
        .map(|r| format!("wss://{r}"))
        .or_else(|| base.strip_prefix("http://").map(|r| format!("ws://{r}")))
        .ok_or_else(|| anyhow!("unsupported cloud URL"))?;
    Ok(format!("{rest}{PATH}"))
}

type Sink = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

/// One relay connection; returns the close code the relay sent, if any.
async fn session(hub: &Arc<Hub>, address: &cloud::Address) -> Result<Option<u16>> {
    let mut req = relay_url(&address.url)?.into_client_request()?;
    let sig = hub.identity.sign_request("GET", &cloud::authority(&address.url)?, PATH, b"");
    req.headers_mut().insert("Codync-Sig", sig.parse()?);
    let (ws, _) = timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(req))
        .await
        .map_err(|_| anyhow!("timed out connecting to the relay"))??;
    // Account devices wait for this connection's state pull (§4.3).
    hub.cloud_synced.store(false, Ordering::Release);
    cloud::update_status(hub, |s| {
        s.connected = true;
        s.last_error = None;
    });
    tracing::info!(computer_id = hub.identity.computer_id(), "connected to the relay");
    let (sink, mut stream) = ws.split();
    let mut s = Session {
        hub: hub.clone(),
        address: address.clone(),
        sink,
        links: HashMap::new(),
        outs: StreamMap::new(),
        pulls: JoinSet::new(),
        assistant: JoinSet::new(),
        held: None,
        ready: false,
        retried: false,
    };
    if timeout(STATE_WAIT, pull_state(hub.clone(), address.clone())).await.is_err() {
        tracing::warn!("the cloud state didn't arrive in time; account devices wait for it");
    }
    let mut auth = hub.auth.subscribe();
    auth.mark_unchanged();
    let mut hooks = hub.routines.subscribe_hooks();
    hooks.mark_unchanged();
    s.publish_acl().await?;
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.reset();
    let mut poll = tokio::time::interval(STATE_EVERY);
    poll.reset();
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            // Links first: a pairing answer and its `4100` go out before the ACL that ends the offer.
            biased;
            Some((link, out)) = s.outs.next() => s.link_out(link, out).await?,
            msg = stream.next() => {
                heard = Instant::now();
                match msg {
                    None => return Ok(None),
                    Some(Err(e)) => return Err(e).context("relay socket"),
                    Some(Ok(Message::Text(t))) => s.on_message(t.as_str()).await?,
                    Some(Ok(Message::Close(frame))) => return Ok(frame.map(|f| u16::from(f.code))),
                    Some(Ok(_)) => {}
                }
            }
            changed = auth.changed() => {
                if changed.is_err() {
                    return Ok(None);
                }
                s.publish_acl().await?;
            }
            changed = hooks.changed() => {
                if changed.is_err() {
                    return Ok(None);
                }
                if s.ready {
                    s.publish_hooks().await?;
                }
            }
            _ = ping.tick() => {
                if !owner::wants_relay(hub, address) {
                    return Ok(None);
                }
                if heard.elapsed() > SILENT {
                    bail!("the relay stopped answering");
                }
                s.sink.send(Message::Text(PING.into())).await?;
                // The state pull failed on this connection: account devices stay out until one works.
                if !hub.cloud_synced.load(Ordering::Acquire) && s.pulls.is_empty() {
                    s.pull();
                }
            }
            // Also without an owner: a claim whose `cloud.changed` never arrived is picked up here.
            _ = poll.tick() => s.pull(),
            Some(_) = s.pulls.join_next() => s.deliver_held().await?,
            Some(Ok(result)) = s.assistant.join_next() => s.send(&result).await?,
        }
    }
}

/// A state pull on the current connection; success lets account devices in. A private relay
/// has no accounts: nothing is pulled, so its empty grant list never removes an account device,
/// and those devices stay out, as with the cloud off, until the Codync cloud renews them.
async fn pull_state(hub: Arc<Hub>, address: cloud::Address) {
    if address.kind == cloud::RelayKind::Private {
        return;
    }
    match cloud::pull(&hub, &address.url).await {
        Ok(()) => hub.cloud_synced.store(true, Ordering::Release),
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "couldn't get the cloud state");
            cloud::update_status(&hub, |s| s.last_error = Some(format!("{e:#}")));
        }
    }
}

struct Session {
    hub: Arc<Hub>,
    address: cloud::Address,
    sink: Sink,
    /// Device messages into each link's channel.
    links: HashMap<String, mpsc::Sender<Value>>,
    /// What each link's channel sends back.
    outs: StreamMap<String, ReceiverStream<Out>>,
    /// State pulls in flight; dropped (aborted) with the connection.
    pulls: JoinSet<()>,
    /// Cloud assistant calls being answered (a listing may wait for the other computers), so
    /// they don't hold up the devices' messages; their `assistant.result` goes out when done.
    assistant: JoinSet<Value>,
    /// An account device's queued send that arrived before this connection's state pull
    /// succeeded. The relay sends the next item only after an ack, so there is at most one.
    held: Option<Value>,
    ready: bool,
    /// The last ACL was refused once already.
    retried: bool,
}

impl Session {
    async fn send(&mut self, v: &Value) -> Result<()> {
        self.sink.send(Message::Text(v.to_string().into())).await.context("relay socket")
    }

    fn pull(&mut self) {
        self.pulls.spawn(pull_state(self.hub.clone(), self.address.clone()));
    }

    /// Delivers the held mailbox item once a state pull has succeeded.
    async fn deliver_held(&mut self) -> Result<()> {
        if !self.hub.cloud_synced.load(Ordering::Acquire) {
            return Ok(());
        }
        let Some(item) = self.held.take() else { return Ok(()) };
        let ack = mailbox_item(&self.hub, &item).await;
        self.send(&ack).await
    }

    async fn publish_acl(&mut self) -> Result<()> {
        let msg = signed_acl(&self.hub)?;
        self.send(&msg).await
    }

    /// The whole webhook set; the relay replaces what it had.
    async fn publish_hooks(&mut self) -> Result<()> {
        let hooks = self.hub.routines.hooks();
        self.send(&json!({"t": "hooks", "hooks": hooks})).await
    }

    async fn link_out(&mut self, link: String, out: Out) -> Result<()> {
        let msg = match out {
            Out::Msg(m) => json!({"t": "data", "link": link, "m": m}),
            Out::Close(code, reason) => {
                self.links.remove(&link);
                self.outs.remove(&link);
                json!({"t": "close", "link": link, "code": code, "reason": reason})
            }
        };
        self.send(&msg).await
    }

    async fn close_link(&mut self, link: &str, code: u16, reason: &str) -> Result<()> {
        self.links.remove(link);
        self.outs.remove(link);
        self.send(&json!({"t": "close", "link": link, "code": code, "reason": reason})).await
    }

    async fn on_message(&mut self, text: &str) -> Result<()> {
        let v: Value = serde_json::from_str(text).unwrap_or_default();
        match v["t"].as_str().unwrap_or_default() {
            "open" => {
                let (Some(link), Some(dk)) = (v["link"].as_str(), v["dk"].as_str()) else { return Ok(()) };
                let (in_tx, incoming) = mpsc::channel(LINK_BUFFER);
                let (outgoing, out_rx) = mpsc::channel(64);
                self.links.insert(link.to_owned(), in_tx);
                self.outs.insert(link.to_owned(), ReceiverStream::new(out_rx));
                let kind = ChannelKind::Relay {
                    link: link.to_owned(),
                    dk: dk.to_owned(),
                    pair: v["pair"].as_bool().unwrap_or(false),
                };
                tokio::spawn(channel::run(self.hub.clone(), Transport { incoming, outgoing }, kind));
            }
            "data" => {
                let link = v["link"].as_str().unwrap_or_default();
                let Some(tx) = self.links.get(link) else { return Ok(()) };
                let m = if v["m"].is_object() { v["m"].clone() } else { Value::Null };
                match tx.try_send(m) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        tracing::warn!(link, "device link overflowed; closing it");
                        self.close_link(link, RATE_LIMITED, "rateLimited").await?;
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        self.links.remove(link);
                    }
                }
            }
            // The device is gone: its channel ends when its input does.
            "close" => {
                if let Some(link) = v["link"].as_str() {
                    self.links.remove(link);
                    self.outs.remove(link);
                }
            }
            "mbox.item" => {
                let from = v["from"].as_str().unwrap_or_default();
                let account = self.hub.store.device(from).is_some_and(|d| d.source == DeviceSource::Account);
                let pulls = self.address.kind == cloud::RelayKind::Codync;
                if account && pulls && !self.hub.cloud_synced.load(Ordering::Acquire) {
                    // Refusing it would drop it for good; unacknowledged, it waits for the pull
                    // (or goes back to the queue if this connection ends). A private relay never
                    // pulls, so there it is refused instead of holding up the mailbox.
                    self.held = Some(v);
                    if self.pulls.is_empty() {
                        self.pull();
                    }
                    return Ok(());
                }
                let ack = mailbox_item(&self.hub, &v).await;
                self.send(&ack).await?;
            }
            "hook.item" => {
                let ack = hook_item(&self.hub, &v);
                self.send(&ack).await?;
            }
            // A private relay has no accounts, so no cloud assistant (§7.9).
            "assistant.call" if self.address.kind == cloud::RelayKind::Codync => {
                let hub = self.hub.clone();
                self.assistant.spawn(async move { crate::chat::team::assistant::answer(&hub, &v).await });
            }
            "cloud.changed" => self.pull(),
            "acl.ok" => {
                self.retried = false;
                if !self.ready {
                    self.ready = true;
                    self.send(&json!({"t": "ready"})).await?;
                    self.publish_hooks().await?;
                    tracing::info!(computer_id = self.hub.identity.computer_id(), "relay ready");
                }
            }
            "error" => {
                let code = v["code"].as_str().unwrap_or_default();
                if code == "badHooks" {
                    tracing::error!("the relay refused this computer's webhook list");
                }
                if !matches!(code, "staleVersion" | "badAcl") {
                    return Ok(());
                }
                if self.retried {
                    bail!("the relay refused this computer's ACL ({code})");
                }
                // The relay has a newer version than this database (deleted or restored): jump past it.
                self.retried = true;
                bump_acl_ver(&self.hub, v["ver"].as_i64().unwrap_or(0))?;
                self.publish_acl().await?;
            }
            _ => {}
        }
        Ok(())
    }
}

// MARK: ACL (§4.3)

fn bump_acl_ver(hub: &Hub, at_least: i64) -> Result<()> {
    let ver = hub.store.kv_get("acl_ver").and_then(|v| v.parse().ok()).unwrap_or(0_i64).max(at_least);
    hub.store.kv_set("acl_ver", &ver.to_string())
}

/// The next ACL (version bumped), signed: `{"t":"acl","d","sig"}`.
fn signed_acl(hub: &Hub) -> Result<Value> {
    let ver = hub.store.kv_get("acl_ver").and_then(|v| v.parse().ok()).unwrap_or(0_i64) + 1;
    hub.store.kv_set("acl_ver", &ver.to_string())?;
    let devices: Vec<Value> =
        hub.store.devices()?.into_iter().map(|d| json!({"dk": d.key, "grant": d.grant_id})).collect();
    let offers: Vec<Value> =
        hub.pairing.locked().offers().into_iter().map(|(id, exp)| json!({"id": id, "exp": exp})).collect();
    let acl = json!({
        "v": 1,
        "computerId": hub.identity.computer_id(),
        "ver": ver,
        "devices": devices,
        "offers": offers,
    })
    .to_string();
    Ok(json!({"t": "acl", "d": b64(acl.as_bytes()), "sig": b64(&hub.identity.sign(acl.as_bytes()))}))
}

#[cfg(test)]
mod tests;
