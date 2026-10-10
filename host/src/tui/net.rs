//! Host API over HTTP: JSON commands and the SSE event stream (same wire as the apps).

pub mod files;

use futures::StreamExt;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use super::app::{After, Msg};
use super::manage::Reply;
mod memory_transfer;

/// Methods that start an agent or download something: they can take minutes.
const SLOW: [&str; 8] = [
    "agentAuth",
    "agentAuthenticate",
    "agentModels",
    "setAgentEnv",
    "installConnector",
    "installSkill",
    "importConnectors",
    "memory",
];

pub const NOT_INSTALLED: &str = "The Codync host isn't set up on this computer yet.";

/// How long a command keeps trying while the host can't be reached (it is restarting, say).
const PATIENCE: Duration = Duration::from_secs(20);

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
        let started = std::time::Instant::now();
        let res = loop {
            let sent = crate::http()
                .post(format!("{}/api/{method}", self.base))
                .bearer_auth(&token)
                .json(body)
                .timeout(Duration::from_secs(
                    if SLOW.contains(&method) || method.starts_with("memory") || method.ends_with("Memory") {
                        11 * 60
                    } else {
                        60
                    },
                ))
                .send()
                .await;
            match sent {
                Ok(res) => break res,
                // Nothing was sent, so any command is safe to repeat while the host comes back.
                // `hello` is the probe that says why it can't be reached, so it answers at once.
                Err(e) if e.is_connect() && method != "hello" && started.elapsed() < PATIENCE => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Err(e) if e.is_timeout() => return Err("The host took too long to answer.".to_owned()),
                Err(_) => return Err(format!("Can't reach codync-host at {}. Is it running?", self.base)),
            }
        };
        let ok = res.status().is_success();
        let v: Value = res.json().await.unwrap_or(Value::Null);
        if ok { Ok(v) } else { Err(v["error"].as_str().unwrap_or("The host refused that.").to_owned()) }
    }

    /// Runs a command in the background; the reply comes back as `Msg::Reply`.
    pub fn spawn_call(&self, method: &'static str, body: Value, after: After, tx: UnboundedSender<Msg>) {
        let c = self.clone();
        tokio::spawn(async move {
            let r = if matches!(method, "exportMemory" | "importMemory") {
                memory_transfer::call(&c, method, &body).await
            } else {
                c.call(method, &body).await
            };
            let _ = tx.send(Msg::Reply(after, r));
        });
    }

    /// Uploads `files` in 384 KiB chunks (under the channel's message limit, like the apps), then sends
    /// `body` with them attached. The reply comes back as `Msg::Reply(after, …)`.
    pub fn spawn_send_files(&self, mut body: Value, files: Vec<PathBuf>, after: After, tx: UnboundedSender<Msg>) {
        use base64::Engine as _;
        const CHUNK: usize = 384 * 1024;
        const MAX: u64 = 100 * 1024 * 1024;
        let c = self.clone();
        tokio::spawn(async move {
            let upload = async {
                let mut ids = Vec::new();
                for path in &files {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let data = tokio::fs::read(path).await.map_err(|e| format!("Can't read {name}: {e}"))?;
                    if data.len() as u64 > MAX {
                        return Err(format!("{name} is over 100 MB."));
                    }
                    let id = uuid::Uuid::new_v4().to_string();
                    let mut offset = 0;
                    loop {
                        let end = (offset + CHUNK).min(data.len());
                        let chunk = base64::engine::general_purpose::STANDARD.encode(&data[offset..end]);
                        let done = end == data.len();
                        c.call(
                            "upload",
                            &json!({"botId": body["botId"], "uploadId": id, "name": name,
                                    "offset": offset, "data": chunk, "done": done}),
                        )
                        .await?;
                        if done {
                            break;
                        }
                        offset = end;
                    }
                    ids.push(id);
                }
                body["attachments"] = ids.into();
                c.call("send", &body).await
            };
            let _ = tx.send(Msg::Reply(after, upload.await));
        });
    }

    /// Streams a setup terminal's output (scrollback first) until it exits; returns where its
    /// keys go, sent in order with whatever piled up meanwhile.
    pub fn spawn_term(&self, id: String, tx: UnboundedSender<Msg>) -> UnboundedSender<Vec<u8>> {
        let (keys, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let c = self.clone();
        let term = id.clone();
        tokio::spawn(async move {
            use base64::Engine as _;
            while let Some(mut bytes) = rx.recv().await {
                while let Ok(more) = rx.try_recv() {
                    bytes.extend(more);
                }
                let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let _ = c.call("termInput", &json!({"term": term, "data": data})).await;
            }
        });
        let c = self.clone();
        tokio::spawn(async move {
            let Some(token) = c.token() else { return };
            let res = crate::http().get(format!("{}/term/{id}", c.base)).bearer_auth(token).send().await;
            let Ok(res) = res.and_then(reqwest::Response::error_for_status) else {
                let _ = tx.send(Msg::Term(id, json!({"type": "exit", "code": -1})));
                return;
            };
            let mut body = res.bytes_stream();
            let mut buf = Vec::new();
            while let Some(Ok(chunk)) = body.next().await {
                buf.extend_from_slice(&chunk);
                while let Some(i) = buf.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = buf.drain(..=i).collect();
                    let line = String::from_utf8_lossy(&line);
                    let Some(data) = line.trim_end().strip_prefix("data:") else { continue };
                    if let Ok(v) = serde_json::from_str::<Value>(data.trim_start())
                        && tx.send(Msg::Term(id.clone(), v)).is_err()
                    {
                        return;
                    }
                }
            }
        });
        keys
    }

    /// Saves sent files (`(upload id, name)`) to Downloads, 384 KiB at a time.
    pub fn spawn_download(&self, bot: String, files: Vec<(String, String)>, tx: UnboundedSender<Msg>) {
        use base64::Engine as _;
        let c = self.clone();
        tokio::spawn(async move {
            let save = async {
                let dir = dirs::download_dir().or_else(dirs::home_dir).ok_or("No Downloads folder")?;
                let mut paths = vec![];
                for (id, name) in files {
                    let mut data = Vec::new();
                    loop {
                        let r =
                            c.call("readUpload", &json!({"botId": bot, "uploadId": id, "offset": data.len()})).await?;
                        let chunk = base64::engine::general_purpose::STANDARD
                            .decode(r["data"].as_str().unwrap_or_default())
                            .map_err(|e| e.to_string())?;
                        data.extend_from_slice(&chunk);
                        if chunk.is_empty() || data.len() as u64 >= r["size"].as_u64().unwrap_or(0) {
                            break;
                        }
                    }
                    let path = free_path(&dir, &name);
                    tokio::fs::write(&path, data).await.map_err(|e| format!("Can't save {name}: {e}"))?;
                    paths.push(path.to_string_lossy().into_owned());
                }
                Ok(json!({"paths": paths}))
            };
            let _ = tx.send(Msg::Reply(After::Sheet(Reply::Downloaded), save.await));
        });
    }

    /// Streams host events forever, reconnecting with `since = last rev`.
    pub fn spawn_stream(&self, tx: UnboundedSender<Msg>) {
        let c = self.clone();
        tokio::spawn(async move {
            loop {
                let since = c.rev.load(Ordering::Relaxed);
                let url = format!("{}/events?since={since}&client=tui", c.base);
                if let Some(token) = c.token()
                    && let Ok(res) = crate::http().get(url).bearer_auth(token).send().await
                    && res.status().is_success()
                {
                    if tx.send(Msg::Online(true)).is_err() || tx.send(Msg::Connected { since }).is_err() {
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

/// `dir/name`, or `name (2)` … when that's taken. The name is only its last part (no `../`).
fn free_path(dir: &std::path::Path, name: &str) -> PathBuf {
    // Only the last plain component: a drive prefix ("C:x") or parent can't escape `dir`.
    let name = std::path::Path::new(name)
        .components()
        .rev()
        .find_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .unwrap_or_else(|| "file".into());
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    (1..10_000)
        .map(|i| dir.join(if i == 1 { name.clone() } else { format!("{stem} ({i}){ext}") }))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(&name))
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn call_waits_for_a_restarting_host() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // A free port with nothing listening yet: the host is restarting.
        let addr = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let client = super::Client::new(&format!("http://{addr}"), Some("t"));
        let call = tokio::spawn(async move { client.call("stop", &serde_json::json!({})).await });
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        assert!(!call.is_finished());
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let (mut socket, _) = listener.accept().await.unwrap();
        let _ = socket.read(&mut [0; 4096]).await.unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}").await.unwrap();
        assert_eq!(call.await.unwrap(), Ok(serde_json::json!({})));
    }

    #[test]
    fn saved_files_never_overwrite_or_escape() {
        let dir = std::env::temp_dir().join(format!("codync-dl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(super::free_path(&dir, "../a.txt"), dir.join("a.txt"));
        assert_eq!(super::free_path(&dir, "/etc/b.txt"), dir.join("b.txt"));
        std::fs::write(dir.join("a.txt"), "").unwrap();
        assert_eq!(super::free_path(&dir, "a.txt"), dir.join("a (2).txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
