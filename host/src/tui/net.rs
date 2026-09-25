//! Host API over HTTP: JSON commands and the SSE event stream (same wire as the apps).

use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use super::app::{After, Msg};

pub const NOT_INSTALLED: &str = "The Codync host isn't set up on this computer yet.";

#[derive(Clone)]
pub struct Client {
    base: Arc<str>,
    /// `None`: this computer's token file, read on every use so an install mid-session is picked up.
    token: Option<Arc<str>>,
    /// Highest rev seen; the stream reconnects from here (0 = full catch-up).
    pub rev: Arc<AtomicI64>,
}

impl Client {
    pub fn new(base: &str, token: Option<&str>) -> Self {
        Self { base: base.trim_end_matches('/').into(), token: token.map(Into::into), rev: Arc::default() }
    }

    pub fn token(&self) -> Option<String> {
        match &self.token {
            Some(t) => Some(t.to_string()),
            None => std::fs::read_to_string(crate::service::data_dir().join("token"))
                .ok()
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty()),
        }
    }

    pub async fn call(&self, method: &str, body: &Value) -> Result<Value, String> {
        let token = self.token().ok_or(NOT_INSTALLED)?;
        let res = crate::http()
            .post(format!("{}/api/{method}", self.base))
            .bearer_auth(token)
            .json(body)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|_| format!("Can't reach codync-host at {}. Is it running?", self.base))?;
        let ok = res.status().is_success();
        let v: Value = res.json().await.unwrap_or(Value::Null);
        if ok { Ok(v) } else { Err(v["error"].as_str().unwrap_or("The host refused that.").to_owned()) }
    }

    /// Runs a command in the background; the reply comes back as `Msg::Reply`.
    pub fn spawn_call(&self, method: &'static str, body: Value, after: After, tx: UnboundedSender<Msg>) {
        let c = self.clone();
        tokio::spawn(async move {
            let r = c.call(method, &body).await;
            let _ = tx.send(Msg::Reply(after, r));
        });
    }

    /// Streams host events forever, reconnecting with `since = last rev`.
    pub fn spawn_stream(&self, tx: UnboundedSender<Msg>) {
        let c = self.clone();
        tokio::spawn(async move {
            loop {
                let url = format!("{}/events?since={}&client=tui", c.base, c.rev.load(Ordering::Relaxed));
                if let Some(token) = c.token()
                    && let Ok(res) = crate::http().get(url).bearer_auth(token).send().await
                    && res.status().is_success()
                {
                    if tx.send(Msg::Online(true)).is_err() {
                        return;
                    }
                    let mut body = res.bytes_stream();
                    let mut buf = Vec::new();
                    'read: while let Some(Ok(chunk)) = body.next().await {
                        buf.extend_from_slice(&chunk);
                        while let Some(i) = buf.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = buf.drain(..=i).collect();
                            let line = String::from_utf8_lossy(&line);
                            let Some(data) = line.trim_end().strip_prefix("data:") else { continue };
                            let Ok(v) = serde_json::from_str::<Value>(data.trim_start()) else {
                                // Never skip what we can't read: start over from scratch.
                                c.rev.store(0, Ordering::Relaxed);
                                let _ = tx.send(Msg::Rewind);
                                break 'read;
                            };
                            if v["type"] == "resync" {
                                break 'read;
                            }
                            if let Some(rev) = v["rev"].as_i64() {
                                c.rev.fetch_max(rev, Ordering::Relaxed);
                            }
                            if tx.send(Msg::Event(v)).is_err() {
                                return;
                            }
                        }
                    }
                }
                if tx.send(Msg::Online(false)).is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }
}
