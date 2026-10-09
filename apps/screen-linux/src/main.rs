//! `codync-screen`: Remote screen for Linux. The host (`codync-host`) starts it
//! while Remote screen is on; it connects to `~/.codync/screen.sock` and serves
//! the same JSON-RPC as the macOS helper (see `host/src/screen/mod.rs`):
//! capture and input through the xdg `RemoteDesktop` + `ScreenCast` portals
//! (approved once, then restored from a saved token), video as H.264 over
//! GStreamer `webrtcbin`, and the accessibility tree over AT-SPI.

mod a11y;
mod input;
mod portal;
mod stream;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{Mutex, mpsc};

fn data_dir() -> PathBuf {
    std::env::var_os("CODYNC_HOME").map_or_else(
        || dirs::home_dir().expect("HOME must be set").join(".codync"),
        PathBuf::from,
    )
}

/// Messages to the host, one JSON line each.
pub type Outbox = mpsc::UnboundedSender<Value>;

pub struct Helper {
    portal: Arc<Mutex<Option<portal::Portal>>>,
    last_status: Mutex<Option<Value>>,
    sessions: Mutex<HashMap<String, stream::Session>>,
    out: Outbox,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "codync_screen=info".into()),
        )
        .init();
    gst::init().context("starting GStreamer")?;
    // Starting the helper must not display a system dialog. Setup explicitly opens a session.
    let portal = Arc::new(Mutex::new(None));
    loop {
        match UnixStream::connect(data_dir().join("screen.sock")).await {
            Ok(sock) => {
                tracing::info!("connected to codync-host");
                if let Err(error) = serve(sock, &portal).await {
                    tracing::warn!(error = format!("{error:#}"), "host link ended");
                }
            }
            Err(error) => tracing::debug!(%error, "codync-host isn't listening yet"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn serve(sock: UnixStream, portal: &Arc<Mutex<Option<portal::Portal>>>) -> Result<()> {
    let (rd, mut wr) = sock.into_split();
    let (out, mut outbox) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = outbox.recv().await {
            let mut line = msg.to_string();
            line.push('\n');
            if wr.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let helper = Arc::new(Helper {
        portal: portal.clone(),
        last_status: Mutex::new(None),
        sessions: Mutex::default(),
        out: out.clone(),
    });
    helper.send_status().await;
    let watched = helper.clone();
    let status_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            watched.send_status().await;
        }
    });
    let mut lines = BufReader::new(rd).lines();
    let result = loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break Ok(()),
            Err(error) => break Err(error.into()),
        };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let (Some(method), Some(id)) = (
            msg["method"].as_str().map(str::to_owned),
            msg.get("id").cloned(),
        ) else {
            continue;
        };
        let helper = helper.clone();
        tokio::spawn(async move {
            let reply = match handle(&helper, &method, &msg["params"]).await {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err(error) => {
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": format!("{error:#}")}})
                }
            };
            let _ = helper.out.send(reply);
        });
    };
    writer.abort();
    status_task.abort();
    for (_, s) in helper.sessions.lock().await.drain() {
        s.close();
    }
    result
}

async fn handle(h: &Arc<Helper>, method: &str, p: &Value) -> Result<Value> {
    if method == "requestPermission" {
        if p["permission"] != "portal" {
            return Err(anyhow!("Linux uses the screen sharing portal"));
        }
        let mut portal = h.portal.lock().await;
        if portal.as_ref().is_none_or(|p| !p.has_input()) {
            let restore = portal.as_ref().is_none_or(|p| !p.is_active());
            let next = portal::Portal::start(&data_dir(), restore).await?;
            if let Some(previous) = portal.as_ref() {
                if previous.is_active() {
                    previous.session().close().await?;
                }
                for (_, session) in h.sessions.lock().await.drain() {
                    session.close();
                }
            }
            *portal = Some(next);
        }
        drop(portal);
        h.send_status().await;
        return Ok(json!({}));
    }
    // Close requests remain valid before setup; all other methods require a granted session.
    if method == "closeAll" {
        for (_, s) in h.sessions.lock().await.drain() {
            s.close();
        }
        return Ok(json!({}));
    }
    if method == "close" {
        if let Some(id) = p["session"].as_str()
            && let Some(s) = h.sessions.lock().await.remove(id)
        {
            s.close();
        }
        return Ok(json!({}));
    }
    let portal = h.portal().await?;
    let display = || portal.display(p["display"].as_u64());
    Ok(match method {
        "answer" => {
            let id = p["session"]
                .as_str()
                .ok_or_else(|| anyhow!("session is required"))?
                .to_owned();
            let sdp = p["sdp"]
                .as_str()
                .ok_or_else(|| anyhow!("sdp is required"))?;
            if let Some(s) = h.sessions.lock().await.get(&id) {
                return Ok(json!({"sdp": s.answer(sdp).await?}));
            }
            let d = display()?;
            let fd = portal.pipewire_fd().await?;
            let session = stream::Session::new(id.clone(), &d, fd, h.clone(), p)?;
            let answer = session.answer(sdp).await?;
            h.sessions.lock().await.insert(id, session);
            json!({"sdp": answer})
        }
        "screenshot" => {
            let (w, h2) = (
                p["width"].as_u64().unwrap_or(1280),
                p["height"].as_u64().unwrap_or(800),
            );
            let d = display()?;
            let fd = portal.pipewire_fd().await?;
            let jpeg = stream::screenshot(fd, &d, u32::try_from(w)?, u32::try_from(h2)?).await?;
            json!({"data": jpeg})
        }
        "input" => {
            let d = display()?;
            input::perform(&portal, &d, &p["event"]).await?;
            json!({})
        }
        "uiTree" => a11y::frontmost(&display()?).await?,
        "focusedField" => a11y::focused_field().await?,
        "openApp" => {
            input::open_app(
                p["name"]
                    .as_str()
                    .ok_or_else(|| anyhow!("name is required"))?,
            )
            .await?;
            json!({})
        }
        other => return Err(anyhow!("unknown method {other}")),
    })
}

impl Helper {
    async fn portal(&self) -> Result<portal::Portal> {
        self.portal
            .lock()
            .await
            .clone()
            .filter(portal::Portal::is_active)
            .ok_or_else(|| anyhow!("Set up Computer access in Codync on this computer first."))
    }

    async fn send_status(&self) {
        let status = self.portal.lock().await.as_ref().map_or_else(
            || json!({"platform": "linux", "permissionApp": "Codync Screen", "capture": false, "input": false, "displays": []}),
            portal::Portal::status,
        );
        let mut last = self.last_status.lock().await;
        if last.as_ref() == Some(&status) {
            return;
        }
        *last = Some(status.clone());
        let _ = self
            .out
            .send(json!({"jsonrpc": "2.0", "method": "status", "params": status}));
    }

    /// A session's WebRTC connection ended on its own.
    pub fn ended(self: &Arc<Self>, id: &str) {
        let this = self.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            if let Some(s) = this.sessions.lock().await.remove(&id) {
                s.close();
            }
            let _ = this.out.send(json!({"jsonrpc": "2.0", "method": "session", "params": {"session": id, "state": "closed"}}));
        });
    }
}
