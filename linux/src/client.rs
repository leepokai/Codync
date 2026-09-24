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
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap())
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("CODYNC_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".codync"))
}

pub fn port() -> u16 {
    std::env::var("CODYNC_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(19222)
}

/// `CODYNC_URL` overrides the host address (e.g. a host outside a container).
pub fn base() -> String {
    std::env::var("CODYNC_URL").unwrap_or_else(|_| format!("http://127.0.0.1:{}", port()))
}

pub fn token() -> Option<String> {
    std::fs::read_to_string(data_dir().join("token")).ok().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
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
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| "Can't reach the Codync host.".to_owned())?;
    let ok = res.status().is_success();
    let v: Value = res.json().await.unwrap_or(Value::Null);
    if ok { Ok(v) } else { Err(v["error"].as_str().unwrap_or("Host error").to_owned()) }
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
                let url = format!("{}/events?since={}&client=linux", base(), REV.load(Ordering::Relaxed));
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
