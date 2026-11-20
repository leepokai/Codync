//! A phone's side of the direct end-to-end channel, shared by browser tests.
//!
//! The host's wire crypto doubles as this test device's implementation. The module is included only by
//! test binaries that also include `host/src/remote/crypto.rs` as `crypto`.

#![allow(dead_code)]

use crate::crypto;
use ed25519_dalek::SigningKey;
use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub struct Phone {
    ws: Ws,
    sealer: crypto::Sealer,
    opener: crypto::Opener,
}

pub enum Frame {
    Inner(Value),
    Closed(u16),
}

impl Phone {
    /// Connects and completes the handshake, verifying the host against the pinned `sk`.
    pub async fn connect(base: &str, key: &SigningKey, sk: &[u8; 32], pair: bool) -> Self {
        let url = format!("{}/channel?v=1", base.replace("http://", "ws://"));
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let cid = crypto::computer_id_raw(sk);
        let device_key = key.verifying_key().to_bytes();
        let ephemeral = x25519_dalek::StaticSecret::from(crypto::random::<32>());
        let ephemeral_public = crypto::x25519_pub(&ephemeral);
        let nonce = crypto::random::<32>();
        let signature = crypto::sign(key, &crypto::hs1_input(&cid, &device_key, &ephemeral_public, &nonce));
        let hello = json!({
            "t": "hello",
            "v": 1,
            "dk": crypto::b64(&device_key),
            "ek": crypto::b64(&ephemeral_public),
            "n": crypto::b64(&nonce),
            "sig": crypto::b64(&signature),
            "pair": pair,
        });
        ws.send(hello.to_string().into()).await.unwrap();
        let welcome: Value = serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(welcome["t"], "welcome", "{welcome}");
        let host_ephemeral: [u8; 32] = crypto::unb64_n(welcome["ek"].as_str().unwrap()).unwrap();
        let transcript = crypto::transcript_hash(&cid, &device_key, &ephemeral_public, &nonce, &host_ephemeral);
        crypto::verify(sk, &transcript, &crypto::unb64_n(welcome["sig"].as_str().unwrap()).unwrap()).unwrap();
        let keys = crypto::channel_keys(&crypto::x25519(&ephemeral, &host_ephemeral).unwrap(), &transcript);
        Self { ws, sealer: crypto::Sealer::new(keys.d2h), opener: crypto::Opener::new(keys.h2d, 16 << 20) }
    }

    pub async fn send(&mut self, value: Value) {
        for frame in self.sealer.seal(value.to_string().as_bytes()).unwrap() {
            self.ws.send(Message::Text(frame.to_string().into())).await.unwrap();
        }
    }

    pub async fn recv(&mut self) -> Frame {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(60), self.ws.next()).await.expect("host answers");
            match message.unwrap().unwrap() {
                Message::Text(text) => {
                    let frame: Value = serde_json::from_str(text.as_str()).unwrap();
                    if let Some(bytes) = self.opener.open(&frame).unwrap() {
                        return Frame::Inner(serde_json::from_slice(&bytes).unwrap());
                    }
                }
                Message::Close(close) => return Frame::Closed(close.map_or(1005, |close| close.code.into())),
                _ => {}
            }
        }
    }

    pub async fn inner(&mut self) -> Value {
        match self.recv().await {
            Frame::Inner(value) => value,
            Frame::Closed(code) => panic!("closed with {code}"),
        }
    }

    pub async fn call(&mut self, id: u64, method: &str, body: Value) -> Value {
        self.send(json!({"id": id, "m": method, "b": body})).await;
        loop {
            let value = self.inner().await;
            if value["id"] == id && value.get("ev").is_none() {
                assert!(value["ok"].is_object(), "{method}: {value}");
                return value["ok"].clone();
            }
        }
    }
}

pub fn query_param(url: &str, name: &str) -> String {
    url.split(['?', '&']).find_map(|part| part.strip_prefix(&format!("{name}="))).unwrap().to_owned()
}

/// A device certificate as the cloud signs it, with the test host's trusted key.
pub fn test_certificate(device_key: &str, user_id: &str) -> Value {
    use base64::Engine as _;
    use ed25519_dalek::Signer as _;

    let expires = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() + 60_000;
    let payload =
        json!({"v": 1, "deviceKey": device_key, "userId": user_id, "email": "kevin@example.com", "exp": expires})
            .to_string();
    let key = SigningKey::from_bytes(&crate::common::TEST_CLOUD_SEED);
    let signature = key.sign(format!("codync/device-cert/v1\n{payload}").as_bytes());
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes());
    json!({"payload": payload, "sig": signature})
}
