//! `codync-screen`: Remote screen for Linux. The host (`codync-host`) starts it
//! while Remote screen is on; it connects to `~/.codync/screen.sock` and serves
//! the same JSON-RPC as the macOS helper (see `host/src/screen/mod.rs`):
//! capture and input through the xdg `RemoteDesktop` + `ScreenCast` portals
//! (approved once, then restored from a saved token), video as H.264 over
//! GStreamer `webrtcbin`, and the accessibility tree over AT-SPI.

mod a11y;
mod candidates;
mod input;
mod link;
mod negotiation;
mod portal;
mod sessions;
mod stream;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

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
    sessions: sessions::Sessions,
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
    let (out, outbox) = mpsc::unbounded_channel::<Value>();
    let helper = Arc::new(Helper {
        portal: portal.clone(),
        sessions: sessions::Sessions::default(),
        out: out.clone(),
    });
    let _ =
        out.send(json!({"jsonrpc": "2.0", "method": "status", "params": helper.portal.status()}));
    let serving = helper.clone();
    let result = link::serve(sock, out, outbox, move |method, params| {
        let helper = serving.clone();
        async move { handle(&helper, &method, &params).await }
    })
    .await;
    helper.sessions.close_all().await;
    result
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
            let creating = id.clone();
            let answer = h
                .sessions
                .answer(id, sdp, async {
                    let d = display()?;
                    let fd = h.portal.pipewire_fd().await?;
                    let session = stream::Session::new(creating, &d, fd, h.clone(), p)?;
                    let answer = session.answer(sdp).await?;
                    Ok((session, answer))
                })
                .await?;
            json!({"sdp": answer})
        }
        "candidate" => {
            let id = p["session"].as_str().context("missing session")?;
            h.sessions.candidate(id, &p["candidate"]).await?;
            json!({})
        }
        "close" => {
            if let Some(id) = p["session"].as_str() {
                h.sessions.close(id).await;
            }
            json!({})
        }
        "closeAll" => {
            h.sessions.close_all().await;
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
    /// A session's WebRTC connection ended on its own.
    pub fn ended(self: &Arc<Self>, id: &str) {
        let this = self.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            this.sessions.close(&id).await;
            let _ = this.out.send(json!({"jsonrpc": "2.0", "method": "session", "params": {"session": id, "state": "closed"}}));
        });
    }
}
