//! `codync-screen`: Remote screen for Linux. The host (`codync-host`) starts it
//! while Remote screen is on; it connects to `~/.codync/screen.sock` and serves
//! the same JSON-RPC as the macOS helper (see `host/src/screen.rs`):
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
    portal: portal::Portal,
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
    // One portal session for the life of the process: the approval dialog appears once,
    // and later runs restore it silently from the saved token.
    let portal = portal::Portal::start(&data_dir()).await?;
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

async fn serve(sock: UnixStream, portal: &portal::Portal) -> Result<()> {
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
        sessions: Mutex::default(),
        out: out.clone(),
    });
    let _ =
        out.send(json!({"jsonrpc": "2.0", "method": "status", "params": helper.portal.status()}));
    let mut lines = BufReader::new(rd).lines();
    while let Some(line) = lines.next_line().await? {
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
    }
    writer.abort();
    for (_, s) in helper.sessions.lock().await.drain() {
        s.close();
    }
    Ok(())
}

async fn handle(h: &Arc<Helper>, method: &str, p: &Value) -> Result<Value> {
    let display = || h.portal.display(p["display"].as_u64());
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
            let fd = h.portal.pipewire_fd().await?;
            let session = stream::Session::new(id.clone(), &d, fd, h.clone())?;
            let answer = session.answer(sdp).await?;
            h.sessions.lock().await.insert(id, session);
            json!({"sdp": answer})
        }
        "close" => {
            if let Some(s) = p["session"]
                .as_str()
                .and_then(|id| h.sessions.try_lock().ok()?.remove(id))
            {
                s.close();
            }
            json!({})
        }
        "closeAll" => {
            for (_, s) in h.sessions.lock().await.drain() {
                s.close();
            }
            json!({})
        }
        "screenshot" => {
            let (w, h2) = (
                p["width"].as_u64().unwrap_or(1280),
                p["height"].as_u64().unwrap_or(800),
            );
            let d = display()?;
            let fd = h.portal.pipewire_fd().await?;
            let jpeg = stream::screenshot(fd, &d, u32::try_from(w)?, u32::try_from(h2)?).await?;
            json!({"data": jpeg})
        }
        "input" => {
            let d = display()?;
            input::perform(&h.portal, &d, &p["event"]).await?;
            json!({})
        }
        "uiTree" => a11y::frontmost(&display()?).await?,
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
