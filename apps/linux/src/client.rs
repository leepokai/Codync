//! Talks to the local codync-host: JSON commands and the SSE event stream,
//! run on a tokio runtime and handed to the GTK main loop through channels.

use futures::StreamExt;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    })
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("CODYNC_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".codync"))
}

pub fn port() -> u16 {
    std::env::var("CODYNC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(19222)
}

/// `CODYNC_URL` overrides the host address (e.g. a host outside a container).
pub fn base() -> String {
    std::env::var("CODYNC_URL").unwrap_or_else(|_| format!("http://127.0.0.1:{}", port()))
}

pub fn token() -> Option<String> {
    std::fs::read_to_string(data_dir().join("token"))
        .ok()
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

fn http() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(reqwest::Client::new)
}

async fn call_async(method: String, body: Value) -> Result<Value, String> {
    let token = token().ok_or("The Codync host isn't set up on this computer.")?;
    let res = http()
        .post(format!("{}/api/{method}", base()))
        .bearer_auth(token)
        .json(&body)
        .timeout(Duration::from_secs(
            if matches!(
                method.as_str(),
                "agentAuth"
                    | "agentAuthenticate"
                    | "setAgentEnv"
                    | "installSkill"
                    | "installConnector"
            ) {
                660
            } else {
                120
            },
        ))
        .send()
        .await
        .map_err(|_| "Can't reach the Codync host.".to_owned())?;
    let ok = res.status().is_success();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    if ok {
        Ok(v)
    } else {
        Err(v["error"].as_str().unwrap_or("Host error").to_owned())
    }
}

/// Runs a host command off the main thread and hands the result back on it.
pub fn call(method: &str, body: Value, done: impl FnOnce(Result<Value, String>) + 'static) {
    let (tx, rx) = async_channel::bounded(1);
    let method = method.to_owned();
    runtime().spawn(async move {
        let _ = tx.send(call_async(method, body).await).await;
    });
    gtk::glib::spawn_future_local(async move {
        if let Ok(r) = rx.recv().await {
            done(r);
        }
    });
}

/// Uploads `files` in 384 KiB chunks (under the channel's message limit, like the apps), then
/// sends `body` with them attached; the result comes back on the main thread.
pub fn send_files(
    mut body: Value,
    files: Vec<PathBuf>,
    done: impl FnOnce(Result<Value, String>) + 'static,
) {
    const CHUNK: usize = 384 * 1024;
    const MAX: usize = 100 * 1024 * 1024;
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let r = async {
            let mut ids = Vec::new();
            for path in &files {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let data = tokio::fs::read(path)
                    .await
                    .map_err(|e| format!("Can't read {name}: {e}"))?;
                if data.len() > MAX {
                    return Err(format!("{name} is over 100 MB."));
                }
                let id = uuid::Uuid::new_v4().to_string();
                let mut offset = 0;
                loop {
                    let end = (offset + CHUNK).min(data.len());
                    let chunk = gtk::glib::base64_encode(&data[offset..end]).to_string();
                    let done = end == data.len();
                    let chunk_body = json!({"botId": body["botId"], "uploadId": id, "name": name,
                                            "offset": offset, "data": chunk, "done": done});
                    call_async("upload".into(), chunk_body).await?;
                    if done {
                        break;
                    }
                    offset = end;
                }
                ids.push(id);
            }
            body["attachments"] = ids.into();
            call_async("send".into(), body).await
        };
        let _ = tx.send(r.await).await;
    });
    gtk::glib::spawn_future_local(async move {
        if let Ok(r) = rx.recv().await {
            done(r);
        }
    });
}

pub enum Event {
    Online(bool),
    Message(Value),
}

static REV: AtomicI64 = AtomicI64::new(0);

pub fn reset_rev() {
    REV.store(0, Ordering::Relaxed);
}

/// Streams host events forever, reconnecting with `since = last rev`.
pub fn stream(tx: async_channel::Sender<Event>) {
    runtime().spawn(async move {
        loop {
            if let Some(token) = token() {
                let url = format!(
                    "{}/events?since={}&client=linux",
                    base(),
                    REV.load(Ordering::Relaxed)
                );
                if let Ok(res) = http().get(url).bearer_auth(token).send().await
                    && res.status().is_success()
                {
                    let _ = tx.send(Event::Online(true)).await;
                    let mut body = res.bytes_stream();
                    let mut buf = String::new();
                    while let Some(Ok(chunk)) = body.next().await {
                        buf.push_str(&String::from_utf8_lossy(&chunk));
                        while let Some(i) = buf.find('\n') {
                            let line: String = buf.drain(..=i).collect();
                            if let Some(data) = line.trim_end().strip_prefix("data:")
                                && let Ok(v) = serde_json::from_str::<Value>(data.trim_start())
                            {
                                if let Some(rev) = v["rev"].as_i64() {
                                    REV.fetch_max(rev, Ordering::Relaxed);
                                }
                                if tx.send(Event::Message(v)).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            if tx.send(Event::Online(false)).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

/// `codync-host install`, for when the host isn't running yet.
pub fn install_host(done: impl FnOnce(Result<(), String>) + 'static) {
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let bin = ["codync-host", "/usr/local/bin/codync-host", "/home/linuxbrew/.linuxbrew/bin/codync-host"]
            .iter()
            .map(PathBuf::from)
            .chain(dirs::home_dir().map(|h| h.join(".cargo/bin/codync-host")))
            .chain(dirs::home_dir().map(|h| h.join(".local/bin/codync-host")))
            .find(|p| p.components().count() == 1 || p.exists());
        let r = match bin {
            Some(bin) => match tokio::process::Command::new(bin).arg("install").output().await {
                Ok(o) if o.status.success() => Ok(()),
                Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_owned()),
                Err(_) => Err("codync-host isn't installed. Install it with Homebrew or from a release, then try again.".into()),
            },
            None => Err("codync-host isn't installed.".into()),
        };
        let _ = tx.send(r).await;
    });
    gtk::glib::spawn_future_local(async move {
        if let Ok(r) = rx.recv().await {
            done(r);
        }
    });
}

pub fn empty() -> Value {
    json!({})
}

/// Save an attachment without overwriting an existing download or trusting a remote path.
pub fn save_file(
    bot: String,
    id: String,
    name: String,
    done: impl FnOnce(Result<PathBuf, String>) + 'static,
) {
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let result = async {
            use tokio::io::AsyncWriteExt as _;
            let dir = dirs::download_dir()
                .or_else(dirs::home_dir)
                .ok_or("No Downloads folder")?;
            tokio::fs::create_dir_all(&dir)
                .await
                .map_err(|e| e.to_string())?;
            let name = std::path::Path::new(&name)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| !n.is_empty() && *n != "." && *n != "..")
                .unwrap_or("attachment");
            let mut suffix = 0;
            let (path, mut file) = loop {
                let path = dir.join(if suffix == 0 {
                    name.to_owned()
                } else {
                    format!("{suffix}-{name}")
                });
                match tokio::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .await
                {
                    Ok(file) => break (path, file),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => suffix += 1,
                    Err(e) => return Err(e.to_string()),
                }
            };
            let download = async {
                let mut offset = 0_u64;
                loop {
                    let v = call_async(
                        "readUpload".into(),
                        json!({"botId":bot,"uploadId":id,"offset":offset}),
                    )
                    .await?;
                    let chunk =
                        gtk::glib::base64_decode(v["data"].as_str().ok_or("Missing file data")?);
                    let size = v["size"].as_u64().ok_or("Missing file size")?;
                    if size > 100 * 1024 * 1024 || offset + chunk.len() as u64 > size {
                        return Err("Invalid attachment size".to_owned());
                    }
                    if chunk.is_empty() && offset < size {
                        return Err("Incomplete attachment".to_owned());
                    }
                    file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    offset += chunk.len() as u64;
                    if offset == size {
                        return Ok(());
                    }
                }
            }
            .await;
            drop(file);
            if let Err(e) = download {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
            Ok(path)
        }
        .await;
        let _ = tx.send(result).await;
    });
    gtk::glib::spawn_future_local(async move {
        if let Ok(r) = rx.recv().await {
            done(r);
        }
    });
}

/// One ordered input queue and cancellable output stream for a host setup terminal.
pub fn terminal(
    id: String,
    events: async_channel::Sender<Result<Value, String>>,
) -> (async_channel::Sender<Vec<u8>>, tokio::task::AbortHandle) {
    let (keys, input) = async_channel::unbounded::<Vec<u8>>();
    let term = id.clone();
    let errors = events.clone();
    runtime().spawn(async move {
        while let Ok(bytes) = input.recv().await {
            let data = gtk::glib::base64_encode(&bytes).to_string();
            if let Err(e) = call_async("termInput".into(), json!({"term":term,"data":data})).await {
                let _ = errors.send(Err(e)).await;
            }
        }
    });
    let output = runtime().spawn(async move {
        let read = async {
            let token = token().ok_or("Host token is missing")?;
            let response = http()
                .get(format!("{}/term/{id}", base()))
                .bearer_auth(token)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| e.to_string())?;
            let mut stream = response.bytes_stream();
            let mut buffer = Vec::new();
            while let Some(chunk) = stream.next().await {
                buffer.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
                while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                    let line: Vec<_> = buffer.drain(..=end).collect();
                    let line = String::from_utf8_lossy(&line);
                    if let Some(data) = line.trim_end().strip_prefix("data:") {
                        let event: Value =
                            serde_json::from_str(data.trim()).map_err(|e| e.to_string())?;
                        let exited = event["type"] == "exit";
                        if events.send(Ok(event)).await.is_err() || exited {
                            return Ok(());
                        }
                    }
                }
            }
            Err("Terminal disconnected. Close it and retry setup.".to_owned())
        }
        .await;
        if let Err(e) = read {
            let _ = events.send(Err(e)).await;
        }
    });
    (keys, output.abort_handle())
}
