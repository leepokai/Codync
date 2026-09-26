//! The end-to-end encrypted channel between a device and this host (spec §6.1–§6.6,
//! §9.5), independent of how it travels: a direct WebSocket (`GET /channel`, served
//! here) or a relay link (the relay client feeds a [`Transport`] per link).
//!
//! Flow: `hello` → checks (cheap first) → `welcome` → encrypted frames carrying the
//! inner RPC: requests run through [`api::dispatch`] (responses may come back out of
//! order), `sub` streams events or a setup terminal, `cancel` ends either.
//! The device's authorization is re-checked on every request, whenever the device
//! table changes and when its lease runs out; a revoked device is closed at once.

use crate::LockExt;
use crate::api;
use crate::crypto::{self, FrameError, Opener, Sealer};
use crate::devices::{self, Caller, RejectCode};
use crate::hub::Hub;
use crate::store::{Device, DeviceSource, Scope, now_ms};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use tokio::time::{Instant, sleep_until, timeout};
use x25519_dalek::StaticSecret;

/// Largest WebSocket message accepted (and the Cloudflare limit); 256 KiB chunks fit.
pub const MAX_WS_MESSAGE: usize = 1 << 20;
/// Largest reassembled device → host message.
const MAX_INBOUND: usize = 1 << 20;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Channel keys are replaced (close `4011`) after this long.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// A pairing channel only lives long enough to redeem its code.
const PAIRING_MAX_AGE: Duration = Duration::from_secs(120);
const DIRECT_PING: Duration = Duration::from_secs(15);
const MAX_REQUESTS: usize = 32;
const MAX_SUBS: usize = 4;

/// WebSocket close codes (§7.6).
mod close {
    pub const NORMAL: u16 = 1000;
    pub const TIMEOUT: u16 = 1011;
    pub const UNAUTHORIZED: u16 = 4001;
    pub const PROTOCOL: u16 = 4002;
    pub const REKEY: u16 = 4011;
    pub const TOO_LARGE: u16 = 4013;
    pub const PAIRED: u16 = 4100;
    pub const PAIRING_EXPIRED: u16 = 4410;
}

/// Something to put on the wire.
#[derive(Debug)]
pub enum Out {
    /// A plaintext channel message (`welcome`, `reject`, `f`).
    Msg(Value),
    /// Close with this code and reason; nothing follows.
    Close(u16, &'static str),
}

/// One channel's wire, whatever carries it. `incoming` yields the device's channel
/// messages (`Value::Null` for anything that isn't a JSON text message) and ends when
/// the device is gone.
pub struct Transport {
    pub incoming: mpsc::Receiver<Value>,
    pub outgoing: mpsc::Sender<Out>,
}

pub enum ChannelKind {
    Direct {
        peer: IpAddr,
    },
    /// A relay link, with what the relay's `open` said about it.
    Relay {
        link: String,
        dk: String,
        pair: bool,
    },
}

impl ChannelKind {
    /// Where pairing attempts are counted from.
    fn source(&self) -> String {
        match self {
            Self::Direct { peer } => peer.to_canonical().to_string(),
            Self::Relay { link, .. } => format!("link:{link}"),
        }
    }
}

/// Serves `GET /channel` after the WebSocket upgrade.
pub async fn serve_direct(hub: Arc<Hub>, socket: WebSocket, peer: IpAddr) {
    let (mut sink, mut stream) = socket.split();
    let (in_tx, incoming) = mpsc::channel(64);
    let (outgoing, mut out_rx) = mpsc::channel::<Out>(64);
    let reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let v = match msg {
                Message::Text(t) => serde_json::from_str(t.as_str()).unwrap_or(Value::Null),
                Message::Binary(_) => Value::Null,
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => continue,
            };
            if in_tx.send(v).await.is_err() {
                break;
            }
        }
    });
    let writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval(DIRECT_PING);
        ping.tick().await;
        loop {
            tokio::select! {
                out = out_rx.recv() => {
                    let msg = match out {
                        Some(Out::Msg(v)) => Message::Text(v.to_string().into()),
                        Some(Out::Close(code, reason)) => Message::Close(Some(CloseFrame { code, reason: reason.into() })),
                        None => Message::Close(None),
                    };
                    let last = matches!(msg, Message::Close(_));
                    if sink.send(msg).await.is_err() || last {
                        break;
                    }
                }
                _ = ping.tick() => {
                    if sink.send(Message::Ping(axum::body::Bytes::new())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    run(hub, Transport { incoming, outgoing }, ChannelKind::Direct { peer }).await;
    let _ = writer.await;
    reader.abort();
}

/// What the encrypted part of the channel sends: inner messages get sealed, the rest passes.
enum ToDevice {
    Inner(Value),
    Plain(Value),
    Close(u16, &'static str),
}

/// Owns the host → device counter: seals inner messages in order, stops at the first close.
async fn write_frames(mut sealer: Sealer, mut rx: mpsc::Receiver<ToDevice>, out: mpsc::Sender<Out>) {
    while let Some(msg) = rx.recv().await {
        let sent = match msg {
            ToDevice::Plain(v) => out.send(Out::Msg(v)).await.is_ok(),
            ToDevice::Close(code, reason) => {
                let _ = out.send(Out::Close(code, reason)).await;
                return;
            }
            ToDevice::Inner(v) => {
                let Some(frames) = sealer.seal(v.to_string().as_bytes()) else {
                    let _ = out.send(Out::Close(close::REKEY, "rekey")).await;
                    return;
                };
                let mut ok = true;
                for f in frames {
                    ok = ok && out.send(Out::Msg(f)).await.is_ok();
                }
                ok
            }
        };
        if !sent {
            return;
        }
    }
}

/// Runs one channel to its end.
pub async fn run(hub: Arc<Hub>, t: Transport, kind: ChannelKind) {
    let Transport { mut incoming, outgoing } = t;
    let Some(hs) = handshake(&hub, &mut incoming, &outgoing, &kind).await else { return };
    let (tx, rx) = mpsc::channel(256);
    let mut writer = tokio::spawn(write_frames(Sealer::new(hs.keys.h2d), rx, outgoing));
    let mut ch = Channel {
        hub,
        tx,
        jobs: Arc::default(),
        key: hs.key,
        caller: Arc::new(Caller::Local),
        lease: None,
        online: None,
    };
    let opener = Opener::new(hs.keys.d2h, MAX_INBOUND);
    let (code, reason) = if let Some(d) = hs.device {
        ch.set_device(&d);
        ch.serve(&mut incoming, opener, &mut writer).await
    } else {
        ch.caller = Arc::new(Caller::Pairing { key: ch.key.clone() });
        ch.pair(&mut incoming, opener, &kind.source()).await
    };
    for job in ch.jobs.locked().drain() {
        if let (_, Job::Sub(h)) = job {
            h.abort();
        }
    }
    let _ = ch.tx.send(ToDevice::Close(code, reason)).await;
    let hub = ch.hub.clone();
    drop(ch);
    let _ = writer.await;
    // Only now, with the answer and the `4100` handed to the transport: a relay publishes the
    // new ACL (offer gone) behind them on the same socket, so the pairing link isn't cut first.
    if code == close::PAIRED {
        hub.auth_changed();
    }
}

struct Handshake {
    key: String,
    keys: crypto::Keys,
    /// `None` for a pairing channel.
    device: Option<Device>,
}

async fn send_reject(out: &mpsc::Sender<Out>, code: RejectCode) {
    let _ = out.send(Out::Msg(json!({"t": "reject", "code": code, "message": code.message()}))).await;
    let _ = out.send(Out::Close(code.close_code(), "rejected")).await;
}

async fn handshake(
    hub: &Hub,
    incoming: &mut mpsc::Receiver<Value>,
    out: &mpsc::Sender<Out>,
    kind: &ChannelKind,
) -> Option<Handshake> {
    let protocol_error = async || {
        let _ = out.send(Out::Close(close::PROTOCOL, "protocolError")).await;
    };
    let hello = match timeout(HANDSHAKE_TIMEOUT, incoming.recv()).await {
        Ok(Some(v)) => v,
        Ok(None) => return None,
        Err(_) => {
            let _ = out.send(Out::Close(close::TIMEOUT, "no hello")).await;
            return None;
        }
    };
    let dk = hello["dk"].as_str().unwrap_or_default().to_owned();
    let Ok(dk_raw) = crypto::unb64_n::<32>(&dk) else {
        protocol_error().await;
        return None;
    };
    if hello["t"] != "hello" {
        protocol_error().await;
        return None;
    }
    if hello["v"] != 1 {
        send_reject(out, RejectCode::UnsupportedVersion).await;
        return None;
    }
    let pair = hello["pair"].as_bool().unwrap_or(false);
    if let ChannelKind::Relay { dk: open_dk, pair: open_pair, .. } = kind
        && (*open_dk != dk || *open_pair != pair)
    {
        protocol_error().await;
        return None;
    }
    // Authorization before any cryptography.
    let device = if pair {
        if !hub.pairing.locked().open() {
            send_reject(out, RejectCode::PairingClosed).await;
            return None;
        }
        None
    } else {
        match devices::authorize(hub, &dk) {
            Ok(d) => Some(d),
            Err(code) => {
                send_reject(out, code).await;
                return None;
            }
        }
    };
    let field = |k: &str| hello[k].as_str().unwrap_or_default();
    let (Ok(ek_d), Ok(n), Ok(sig)) =
        (crypto::unb64_n::<32>(field("ek")), crypto::unb64_n::<32>(field("n")), crypto::unb64_n::<64>(field("sig")))
    else {
        protocol_error().await;
        return None;
    };
    let cid = hub.identity.cid_raw();
    if crypto::verify(&dk_raw, &crypto::hs1_input(&cid, &dk_raw, &ek_d, &n), &sig).is_err() {
        send_reject(out, RejectCode::BadSignature).await;
        return None;
    }
    let ek_h = StaticSecret::from(crypto::random::<32>());
    let ek_h_pub = crypto::x25519_pub(&ek_h);
    let Ok(shared) = crypto::x25519(&ek_h, &ek_d) else {
        protocol_error().await;
        return None;
    };
    let th = crypto::transcript_hash(&cid, &dk_raw, &ek_d, &n, &ek_h_pub);
    let welcome =
        json!({"t": "welcome", "v": 1, "ek": crypto::b64(&ek_h_pub), "sig": crypto::b64(&hub.identity.sign(&th))});
    if out.send(Out::Msg(welcome)).await.is_err() {
        return None;
    }
    Some(Handshake { key: dk, keys: crypto::channel_keys(&shared, &th), device })
}

enum Job {
    Request,
    Sub(AbortHandle),
}

/// Marks a device connected while it has a live channel.
struct Online(Arc<Hub>, String);

impl Online {
    fn new(hub: Arc<Hub>, key: String) -> Self {
        *hub.connected.locked().entry(key.clone()).or_default() += 1;
        if let Err(error) = hub.store.touch_device(&key) {
            tracing::warn!(error = format!("{error:#}"), "couldn't record when a device was last seen");
        }
        Self(hub, key)
    }
}

impl Drop for Online {
    fn drop(&mut self) {
        let mut map = self.0.connected.locked();
        if let Some(n) = map.get_mut(&self.1) {
            *n -= 1;
            if *n == 0 {
                map.remove(&self.1);
            }
        }
    }
}

struct Channel {
    hub: Arc<Hub>,
    tx: mpsc::Sender<ToDevice>,
    jobs: Arc<Mutex<HashMap<u64, Job>>>,
    key: String,
    caller: Arc<Caller>,
    lease: Option<Instant>,
    online: Option<Online>,
}

type CloseWith = (u16, &'static str);

fn frame_close(e: &FrameError) -> CloseWith {
    match e {
        FrameError::Protocol => (close::PROTOCOL, "protocolError"),
        FrameError::TooLarge => (close::TOO_LARGE, "tooLarge"),
        FrameError::Rekey => (close::REKEY, "rekey"),
    }
}

fn err(id: u64, status: u16, message: &str) -> Value {
    json!({"id": id, "err": {"status": status, "message": message}})
}

impl Channel {
    fn set_device(&mut self, d: &Device) {
        self.caller = Arc::new(Caller::Device { key: d.key.clone(), scopes: d.scopes.clone() });
        self.lease = match (d.source, d.lease_until) {
            (DeviceSource::Account, Some(until)) => {
                let left = u64::try_from(until - now_ms()).unwrap_or(0) + 1;
                Some(Instant::now() + Duration::from_millis(left))
            }
            _ => None,
        };
    }

    async fn send(&self, v: Value) {
        let _ = self.tx.send(ToDevice::Inner(v)).await;
    }

    /// Re-checks the device; on failure sends `reject` and says how to close.
    async fn recheck(&mut self) -> Result<(), CloseWith> {
        match devices::authorize(&self.hub, &self.key) {
            Ok(d) => {
                self.set_device(&d);
                Ok(())
            }
            Err(code) => {
                // Gone from the table while connected: it was revoked.
                let code = if code == RejectCode::Unauthorized { RejectCode::Revoked } else { code };
                tracing::info!(device = self.key, ?code, "closing channel: device no longer authorized");
                let reject = json!({"t": "reject", "code": code, "message": code.message()});
                let _ = self.tx.send(ToDevice::Plain(reject)).await;
                Err((code.close_code(), "rejected"))
            }
        }
    }

    async fn serve(
        &mut self,
        incoming: &mut mpsc::Receiver<Value>,
        mut opener: Opener,
        writer: &mut tokio::task::JoinHandle<()>,
    ) -> CloseWith {
        let mut auth = self.hub.auth.subscribe();
        let rekey_at = Instant::now() + MAX_AGE;
        loop {
            let lease = self.lease;
            tokio::select! {
                msg = incoming.recv() => {
                    let Some(msg) = msg else { return (close::NORMAL, "") };
                    let message = match opener.open(&msg) {
                        Ok(m) => m,
                        Err(e) => return frame_close(&e),
                    };
                    if self.online.is_none() {
                        self.online = Some(Online::new(self.hub.clone(), self.key.clone()));
                    }
                    if let Some(bytes) = message
                        && let Err(close) = self.handle(&bytes).await
                    {
                        return close;
                    }
                }
                changed = auth.changed() => {
                    if changed.is_err() {
                        return (close::NORMAL, "");
                    }
                    if let Err(close) = self.recheck().await {
                        return close;
                    }
                }
                () = sleep_until(lease.unwrap_or(rekey_at)), if lease.is_some() => {
                    if let Err(close) = self.recheck().await {
                        return close;
                    }
                }
                () = sleep_until(rekey_at) => return (close::REKEY, "rekey"),
                _ = &mut *writer => return (close::NORMAL, ""),
            }
        }
    }

    /// One inner message from an authorized device.
    async fn handle(&mut self, bytes: &[u8]) -> Result<(), CloseWith> {
        let protocol = (close::PROTOCOL, "protocolError");
        let v: Value = serde_json::from_slice(bytes).map_err(|_| protocol)?;
        let id = v["id"].as_u64().filter(|id| u32::try_from(*id).is_ok()).ok_or(protocol)?;
        if v["cancel"] == true {
            self.cancel(id).await;
            return Ok(());
        }
        if self.jobs.locked().contains_key(&id) {
            self.send(err(id, 400, "that id is already in use")).await;
            return Ok(());
        }
        // Every request re-checks the device (revocation, lease) before it runs.
        self.recheck().await?;
        if let Some(kind) = v["sub"].as_str() {
            self.subscribe(id, kind, &v["b"]).await;
        } else if let Some(method) = v["m"].as_str() {
            self.request(id, method.to_owned(), v["b"].clone()).await;
        } else {
            return Err(protocol);
        }
        Ok(())
    }

    fn count(&self, subs: bool) -> usize {
        self.jobs.locked().values().filter(|j| matches!(j, Job::Sub(_)) == subs).count()
    }

    async fn request(&self, id: u64, method: String, b: Value) {
        if self.count(false) >= MAX_REQUESTS {
            self.send(err(id, 429, "too many requests in flight")).await;
            return;
        }
        self.jobs.locked().insert(id, Job::Request);
        let (hub, caller, jobs, tx) = (self.hub.clone(), self.caller.clone(), self.jobs.clone(), self.tx.clone());
        tokio::spawn(async move {
            let reply = match api::dispatch(&hub, &caller, &method, b).await {
                Ok(ok) => json!({"id": id, "ok": ok}),
                Err(e) => {
                    let (status, message) = api::error_status(&e);
                    err(id, status.as_u16(), &message)
                }
            };
            // Cancelled: the work is done (never aborted midway), the answer is dropped.
            if jobs.locked().remove(&id).is_some() {
                let _ = tx.send(ToDevice::Inner(reply)).await;
            }
        });
    }

    async fn subscribe(&self, id: u64, kind: &str, b: &Value) {
        if self.count(true) >= MAX_SUBS {
            self.send(err(id, 429, "too many subscriptions")).await;
            return;
        }
        let stream = match kind {
            "events" => match api::events_stream(
                &self.hub,
                b["since"].as_i64().unwrap_or(0),
                b["client"].as_str(),
                &self.caller,
            ) {
                Ok(s) => s.boxed(),
                Err(e) => {
                    self.send(err(id, 500, &format!("{e:#}"))).await;
                    return;
                }
            },
            "term" => {
                let Some(s) = api::term_events(&self.hub, b["term"].as_str().unwrap_or_default()) else {
                    self.send(err(id, 404, "that terminal is gone")).await;
                    return;
                };
                s.boxed()
            }
            _ => {
                self.send(err(id, 400, "unknown subscription")).await;
                return;
            }
        };
        let (jobs, tx) = (self.jobs.clone(), self.tx.clone());
        // Held while spawning so the task can't finish (and remove itself) before it's listed.
        let mut list = self.jobs.locked();
        let task = tokio::spawn(async move {
            let mut stream = stream;
            while let Some(ev) = stream.next().await {
                if tx.send(ToDevice::Inner(json!({"id": id, "ev": ev}))).await.is_err() {
                    return;
                }
            }
            if jobs.locked().remove(&id).is_some() {
                let _ = tx.send(ToDevice::Inner(json!({"id": id, "end": true}))).await;
            }
        });
        list.insert(id, Job::Sub(task.abort_handle()));
    }

    async fn cancel(&self, id: u64) {
        let job = self.jobs.locked().remove(&id);
        if let Some(Job::Sub(h)) = job {
            h.abort();
            self.send(json!({"id": id, "end": true})).await;
        }
    }

    /// A pairing channel: exactly one `pair`, answered, then closed with `4100` (§4.1).
    async fn pair(&mut self, incoming: &mut mpsc::Receiver<Value>, mut opener: Opener, source: &str) -> CloseWith {
        let protocol = (close::PROTOCOL, "protocolError");
        let deadline = Instant::now() + PAIRING_MAX_AGE;
        let bytes = loop {
            let msg = tokio::select! {
                msg = incoming.recv() => msg,
                () = sleep_until(deadline) => return (close::PAIRING_EXPIRED, "pairingExpired"),
            };
            let Some(msg) = msg else { return (close::NORMAL, "") };
            match opener.open(&msg) {
                Ok(Some(bytes)) => break bytes,
                Ok(None) => {}
                Err(e) => return frame_close(&e),
            }
        };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { return protocol };
        let Some(id) = v["id"].as_u64() else { return protocol };
        if v["m"] != "pair" {
            return protocol;
        }
        let b = &v["b"];
        let name = b["name"].as_str().map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 100);
        let platform = b["platform"].as_str().filter(|p| matches!(*p, "ios" | "macos"));
        let (Some(name), Some(platform)) = (name, platform) else {
            self.send(err(id, 400, "`name` and `platform` (ios or macos) are required")).await;
            return protocol;
        };
        if !self.hub.pairing.locked().attempt(source) {
            let code = RejectCode::RateLimited;
            let _ =
                self.tx.send(ToDevice::Plain(json!({"t": "reject", "code": code, "message": code.message()}))).await;
            return (code.close_code(), "rateLimited");
        }
        if !self.hub.pairing.locked().redeem(b["code"].as_str().unwrap_or_default()) {
            self.send(err(id, 403, "This pairing code expired. Show a new one on the computer.")).await;
            return (close::UNAUTHORIZED, "unauthorized");
        }
        let device = Device {
            key: self.key.clone(),
            name: name.to_owned(),
            platform: platform.to_owned(),
            source: DeviceSource::Local,
            grant_id: None,
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: None,
            created_at: now_ms(),
            last_seen_at: None,
        };
        if let Err(e) = self.hub.store.put_device(&device) {
            tracing::error!(error = format!("{e:#}"), "couldn't save a paired device");
            self.send(err(id, 500, "couldn't save this device")).await;
            return (close::NORMAL, "");
        }
        tracing::info!(device = self.key, platform, "device paired");
        let name = crate::service::host_name();
        self.send(json!({"id": id, "ok": {"computerId": self.hub.identity.computer_id(), "name": name}})).await;
        (close::PAIRED, "paired")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::identity::Identity;
    use crate::store::Store;
    use ed25519_dalek::SigningKey;

    fn temp_hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-ch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("t.db")).unwrap();
        let identity = Identity::load_or_create(&dir).unwrap();
        Hub::new(store, "host".into(), identity, "token".into(), 0)
    }

    /// The device end of a channel, over in-memory pipes.
    struct Phone {
        key: SigningKey,
        tx: mpsc::Sender<Value>,
        rx: mpsc::Receiver<Out>,
        sealer: Option<Sealer>,
        opener: Option<Opener>,
    }

    impl Phone {
        fn connect(hub: &Arc<Hub>, key: SigningKey, kind: ChannelKind) -> Self {
            let (tx, incoming) = mpsc::channel(64);
            let (outgoing, rx) = mpsc::channel(64);
            tokio::spawn(run(hub.clone(), Transport { incoming, outgoing }, kind));
            Self { key, tx, rx, sealer: None, opener: None }
        }

        fn direct(hub: &Arc<Hub>, key: SigningKey) -> Self {
            Self::connect(hub, key, ChannelKind::Direct { peer: IpAddr::from([10, 0, 0, 2]) })
        }

        fn dk(&self) -> String {
            crypto::b64(self.key.verifying_key().as_bytes())
        }

        async fn next(&mut self) -> Out {
            timeout(Duration::from_secs(5), self.rx.recv()).await.expect("host answers in time").expect("still open")
        }

        async fn hello(&mut self, hub: &Hub, pair: bool) -> Out {
            let ek = StaticSecret::from(crypto::random::<32>());
            let ek_pub = crypto::x25519_pub(&ek);
            let n = crypto::random::<32>();
            let dk = self.key.verifying_key().to_bytes();
            let cid = hub.identity.cid_raw();
            let sig = crypto::sign(&self.key, &crypto::hs1_input(&cid, &dk, &ek_pub, &n));
            let hello = json!({"t": "hello", "v": 1, "dk": self.dk(), "ek": crypto::b64(&ek_pub),
                "n": crypto::b64(&n), "sig": crypto::b64(&sig), "pair": pair});
            self.tx.send(hello).await.unwrap();
            let out = self.next().await;
            if let Out::Msg(w) = &out
                && w["t"] == "welcome"
            {
                let ek_h: [u8; 32] = crypto::unb64_n(w["ek"].as_str().unwrap()).unwrap();
                let th = crypto::transcript_hash(&cid, &dk, &ek_pub, &n, &ek_h);
                crypto::verify(&hub.identity.sign_pub(), &th, &crypto::unb64_n(w["sig"].as_str().unwrap()).unwrap())
                    .expect("welcome is signed by the host");
                let keys = crypto::channel_keys(&crypto::x25519(&ek, &ek_h).unwrap(), &th);
                self.sealer = Some(Sealer::new(keys.d2h));
                self.opener = Some(Opener::new(keys.h2d, 16 << 20));
            }
            out
        }

        async fn send(&mut self, v: Value) {
            for f in self.sealer.as_mut().unwrap().seal(v.to_string().as_bytes()).unwrap() {
                self.tx.send(f).await.unwrap();
            }
        }

        /// The next inner message, or the close that ended the channel.
        async fn recv(&mut self) -> Result<Value, (u16, Value)> {
            let mut last_plain = Value::Null;
            loop {
                match self.next().await {
                    Out::Msg(f) if f["t"] == "f" => {
                        if let Some(bytes) = self.opener.as_mut().unwrap().open(&f).unwrap() {
                            return Ok(serde_json::from_slice(&bytes).unwrap());
                        }
                    }
                    Out::Msg(plain) => last_plain = plain,
                    Out::Close(code, _) => return Err((code, last_plain)),
                }
            }
        }

        async fn call(&mut self, id: u64, m: &str, b: Value) -> Value {
            self.send(json!({"id": id, "m": m, "b": b})).await;
            loop {
                let v = self.recv().await.expect("channel stays open");
                if v["id"] == id && (v.get("ok").is_some() || v.get("err").is_some()) {
                    return v;
                }
            }
        }
    }

    fn closed(out: &Out) -> u16 {
        match out {
            Out::Close(code, _) => *code,
            Out::Msg(v) => panic!("expected a close, got {v}"),
        }
    }

    async fn paired_phone(hub: &Arc<Hub>) -> SigningKey {
        let key = SigningKey::from_bytes(&crypto::random());
        let code = hub.pairing.locked().issue().code;
        let mut p = Phone::direct(hub, key.clone());
        assert!(matches!(p.hello(hub, true).await, Out::Msg(v) if v["t"] == "welcome"));
        p.send(json!({"id": 1, "m": "pair", "b": {"code": code, "name": "Kevin's iPhone", "platform": "ios"}})).await;
        let reply = p.recv().await.unwrap();
        assert_eq!(reply["ok"]["computerId"], hub.identity.computer_id());
        assert_eq!(p.recv().await.unwrap_err().0, close::PAIRED);
        key
    }

    #[tokio::test]
    async fn unknown_device_is_rejected() {
        let hub = temp_hub();
        let mut p = Phone::direct(&hub, SigningKey::from_bytes(&crypto::random()));
        let Out::Msg(reject) = p.hello(&hub, false).await else { panic!("expected reject") };
        assert_eq!(reject["code"], "unauthorized");
        assert_eq!(closed(&p.next().await), 4001);
    }

    #[tokio::test]
    async fn pairing_then_normal_connection() {
        let hub = temp_hub();
        let key = paired_phone(&hub).await;
        let d = hub.store.device(&crypto::b64(key.verifying_key().as_bytes())).unwrap();
        assert_eq!((d.source, d.name.as_str()), (DeviceSource::Local, "Kevin's iPhone"));
        assert!(!hub.pairing.locked().open(), "the code is used up");

        let mut p = Phone::direct(&hub, key);
        assert!(matches!(p.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));
        let hello = p.call(1, "hello", json!({})).await;
        assert_eq!(hello["ok"]["computerId"], hub.identity.computer_id());
        assert_eq!(hello["ok"]["signKey"], hub.identity.sign_pub_b64());
        assert_eq!(hello["ok"]["protocol"], 1);
        assert_eq!(hub.connected.locked().get(&p.dk()), Some(&1));
        assert!(hub.store.device(&p.dk()).unwrap().last_seen_at.is_some());

        // Loopback-only methods are refused over the channel.
        for m in ["setScreenEnabled", "pairing", "devices", "revokeDevice", "claimSign", "computerCall"] {
            assert_eq!(p.call(2, m, json!({})).await["err"]["status"], 403, "{m}");
        }
        assert_eq!(p.call(3, "pair", json!({})).await["err"]["status"], 403);
    }

    #[tokio::test]
    async fn pair_hello_without_a_code_is_refused_before_any_crypto() {
        let hub = temp_hub();
        let (tx, incoming) = mpsc::channel(4);
        let (outgoing, mut rx) = mpsc::channel(4);
        tokio::spawn(run(
            hub.clone(),
            Transport { incoming, outgoing },
            ChannelKind::Direct { peer: [10, 0, 0, 2].into() },
        ));
        let dk = crypto::b64(SigningKey::from_bytes(&[1; 32]).verifying_key().as_bytes());
        // A garbage key and signature: only a pre-crypto check can answer pairingClosed.
        tx.send(json!({"t": "hello", "v": 1, "dk": dk, "ek": "x", "n": "x", "sig": "x", "pair": true})).await.unwrap();
        let Some(Out::Msg(reject)) = rx.recv().await else { panic!("expected reject") };
        assert_eq!(reject["code"], "pairingClosed");
        assert_eq!(closed(&rx.recv().await.unwrap()), 4410);
    }

    #[tokio::test]
    async fn pairing_channel_only_takes_pair_and_codes_are_single_use() {
        let hub = temp_hub();
        let code = hub.pairing.locked().issue().code;
        hub.pairing.locked().issue();
        let mut p = Phone::direct(&hub, SigningKey::from_bytes(&crypto::random()));
        p.hello(&hub, true).await;
        p.send(json!({"id": 1, "m": "hello", "b": {}})).await;
        assert_eq!(p.recv().await.unwrap_err().0, close::PROTOCOL);

        let mut p = Phone::direct(&hub, SigningKey::from_bytes(&crypto::random()));
        p.hello(&hub, true).await;
        p.send(json!({"id": 1, "m": "pair", "b": {"code": code, "name": "A", "platform": "ios"}})).await;
        assert!(p.recv().await.unwrap()["ok"].is_object());

        let mut p = Phone::direct(&hub, SigningKey::from_bytes(&crypto::random()));
        p.hello(&hub, true).await;
        p.send(json!({"id": 1, "m": "pair", "b": {"code": code, "name": "B", "platform": "ios"}})).await;
        assert_eq!(p.recv().await.unwrap()["err"]["status"], 403, "a used code doesn't work twice");
        assert_eq!(p.recv().await.unwrap_err().0, close::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revoking_closes_the_channel_and_its_subscription() {
        let hub = temp_hub();
        let key = paired_phone(&hub).await;
        let dk = crypto::b64(key.verifying_key().as_bytes());
        let mut p = Phone::direct(&hub, key);
        p.hello(&hub, false).await;
        assert!(p.call(1, "registerDevice", json!({"ticket": "t1", "name": "iPhone"})).await["ok"].is_object());
        assert!(p.call(2, "registerActivity", json!({"ticket": "a1", "botId": "b1"})).await["ok"].is_object());
        p.send(json!({"id": 3, "sub": "events", "b": {"since": 0, "client": "ios"}})).await;
        let hello = p.recv().await.unwrap();
        assert_eq!((hello["id"].as_u64(), hello["ev"]["type"].as_str()), (Some(3), Some("hello")));
        assert!(hub.ios_connected());

        assert!(hub.revoke_device(&dk).unwrap());
        let (code, reject) = p.recv().await.unwrap_err();
        assert_eq!(code, 4003);
        assert_eq!(reject["code"], "revoked");
        assert!(hub.store.push_tickets().is_empty());
        assert!(hub.store.activity_tickets("b1").is_empty());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!hub.ios_connected(), "the events subscription ended with the channel");
        assert!(hub.connected.locked().is_empty());
    }

    #[tokio::test]
    async fn account_devices_need_a_synced_live_lease() {
        let hub = temp_hub();
        let key = SigningKey::from_bytes(&crypto::random());
        let dk = crypto::b64(key.verifying_key().as_bytes());
        let device = Device {
            key: dk.clone(),
            name: "Phone".into(),
            platform: "ios".into(),
            source: DeviceSource::Account,
            grant_id: Some("grt_x".into()),
            scopes: vec![Scope::Control],
            lease_until: Some(now_ms() + 300),
            created_at: now_ms(),
            last_seen_at: None,
        };
        hub.store.put_device(&device).unwrap();
        let mut p = Phone::direct(&hub, key.clone());
        let Out::Msg(reject) = p.hello(&hub, false).await else { panic!("expected reject") };
        assert_eq!(reject["code"], "leaseExpired", "refused until the cloud state was applied");

        hub.cloud_synced.store(true, std::sync::atomic::Ordering::Release);
        let mut p = Phone::direct(&hub, key.clone());
        assert!(matches!(p.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));
        assert_eq!(p.call(1, "screenOffer", json!({})).await["err"]["status"], 403, "no screen scope");
        let (code, reject) = p.recv().await.unwrap_err();
        assert_eq!((code, reject["code"].as_str()), (4001, Some("leaseExpired")), "closed when the lease ran out");
    }

    #[tokio::test]
    async fn relay_link_hello_must_match_open() {
        let hub = temp_hub();
        let key = paired_phone(&hub).await;
        let other = crypto::b64(SigningKey::from_bytes(&crypto::random()).verifying_key().as_bytes());
        let kind = ChannelKind::Relay { link: "l1".into(), dk: other, pair: false };
        let mut p = Phone::connect(&hub, key.clone(), kind);
        assert_eq!(closed(&p.hello(&hub, false).await), close::PROTOCOL);

        let dk = crypto::b64(key.verifying_key().as_bytes());
        let mut p = Phone::connect(&hub, key, ChannelKind::Relay { link: "l2".into(), dk, pair: false });
        assert!(matches!(p.hello(&hub, false).await, Out::Msg(v) if v["t"] == "welcome"));
    }

    #[tokio::test]
    async fn replayed_or_bad_frames_close_the_channel() {
        let hub = temp_hub();
        let key = paired_phone(&hub).await;
        let mut p = Phone::direct(&hub, key);
        p.hello(&hub, false).await;
        let frames = p.sealer.as_mut().unwrap().seal(br#"{"id":1,"m":"hello","b":{}}"#).unwrap();
        p.tx.send(frames[0].clone()).await.unwrap();
        assert!(p.recv().await.unwrap()["ok"].is_object());
        p.tx.send(frames[0].clone()).await.unwrap();
        assert_eq!(p.recv().await.unwrap_err().0, close::PROTOCOL, "replay");
    }

    #[tokio::test]
    async fn cancelled_subscription_ends() {
        let hub = temp_hub();
        let key = paired_phone(&hub).await;
        let mut p = Phone::direct(&hub, key);
        p.hello(&hub, false).await;
        p.send(json!({"id": 7, "sub": "events", "b": {"since": 0}})).await;
        assert_eq!(p.recv().await.unwrap()["ev"]["type"], "hello");
        p.send(json!({"id": 7, "cancel": true})).await;
        assert_eq!(p.recv().await.unwrap(), json!({"id": 7, "end": true}));
        assert_eq!(p.call(8, "hello", json!({})).await["id"], 8, "the channel goes on");
        p.send(json!({"id": 9, "sub": "term", "b": {"term": "nope"}})).await;
        assert_eq!(p.recv().await.unwrap()["err"]["status"], 404);
    }

    #[tokio::test]
    async fn device_table_changes_only_after_the_pairing_close_is_queued() {
        let hub = temp_hub();
        let code = hub.pairing.locked().issue().code;
        let key = SigningKey::from_bytes(&crypto::random());
        let dk = crypto::b64(key.verifying_key().as_bytes());
        let mut auth = hub.auth.subscribe();
        auth.mark_unchanged();
        let mut p = Phone::connect(&hub, key, ChannelKind::Relay { link: "p".into(), dk, pair: true });
        p.hello(&hub, true).await;
        p.send(json!({"id": 1, "m": "pair", "b": {"code": code, "name": "A", "platform": "ios"}})).await;
        auth.changed().await.unwrap();
        // The relay republishes the ACL on this change: the answer and 4100 must already be queued.
        let mut last = None;
        while let Ok(out) = p.rx.try_recv() {
            last = Some(out);
        }
        assert!(matches!(last, Some(Out::Close(close::PAIRED, _))), "got {last:?}");
    }
}
