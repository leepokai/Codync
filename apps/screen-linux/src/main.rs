//! `codync-screen`: Remote screen for Linux. The host (`codync-host`) starts it
//! while Remote screen is on; it connects to `~/.codync/screen.sock` and serves
//! the same JSON-RPC as the macOS helper (see `host/src/screen/mod.rs`):
//! capture and input through the xdg `RemoteDesktop` + `ScreenCast` portals
//! (approved once, then restored from a saved token) or, on X11 desktops without
//! them, `ximagesrc` and XTest; video as H.264 over GStreamer `webrtcbin`, and the
//! accessibility tree over AT-SPI.

mod a11y;
mod backend;
mod candidates;
mod capture;
mod connection;
mod input;
mod link;
mod negotiation;
mod portal;
mod sessions;
mod stream;
mod video;
mod x11;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
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
    portal: backend::Slot,
    last_status: Mutex<Option<Value>>,
    sessions: sessions::Sessions,
    out: Outbox,
    /// GStreamer calls `ended` from its own threads, outside the Tokio runtime.
    rt: tokio::runtime::Handle,
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
    // Starting the helper must not display a system dialog. Setup explicitly opens a portal
    // session; X11 has no dialog, so that backend starts here only if setup already chose it.
    let data = data_dir();
    let portal: backend::Slot = Arc::new(Mutex::new(backend::x11_fallback(&data).await));
    if portal.lock().await.is_none() && x11::consent::granted(&data).is_some() {
        tokio::spawn(backend::wait_for_x11(portal.clone(), data));
    }
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

async fn serve(sock: UnixStream, portal: &backend::Slot) -> Result<()> {
    let (out, outbox) = mpsc::unbounded_channel::<Value>();
    let helper = Arc::new(Helper {
        portal: portal.clone(),
        last_status: Mutex::new(None),
        sessions: sessions::Sessions::default(),
        out: out.clone(),
        rt: tokio::runtime::Handle::current(),
    });
    helper.send_status().await;
    let watched = helper.clone();
    let status_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            watched.send_status().await;
        }
    });
    let serving = helper.clone();
    let result = link::serve(sock, out, outbox, move |method, params| {
        let helper = serving.clone();
        async move { handle(&helper, &method, &params).await }
    })
    .await;
    status_task.abort();
    helper.sessions.close_all().await;
    result
}

async fn handle(h: &Arc<Helper>, method: &str, p: &Value) -> Result<Value> {
    if method == "requestPermission" {
        if p["permission"] != "portal" {
            return Err(anyhow!("Linux uses the screen sharing portal"));
        }
        setup(h).await?;
        h.send_status().await;
        return Ok(json!({}));
    }
    // Close requests remain valid before setup; all other methods require a granted session.
    if method == "closeAll" {
        h.sessions.close_all().await;
        return Ok(json!({}));
    }
    if method == "close" {
        if let Some(id) = p["session"].as_str() {
            h.sessions.close(id).await;
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
            let creating = id.clone();
            let answer = h
                .sessions
                .answer(id, sdp, async {
                    let d = display().await?;
                    let capture = portal.capture().await?;
                    let session = stream::Session::new(creating, &d, capture, h, p)?;
                    let answer = session.answer(sdp).await?;
                    Ok((session, answer))
                })
                .await?;
            json!({"sdp": answer})
        }
        "candidate" => {
            let id = p["session"].as_str().context("missing session")?;
            let candidate = serde_json::from_value(p["candidate"].clone())?;
            h.sessions.candidate(id, &candidate).await?;
            json!({})
        }
        "screenshot" => {
            let (w, h2) = (
                p["width"].as_u64().unwrap_or(1280),
                p["height"].as_u64().unwrap_or(800),
            );
            let d = display().await?;
            let capture = portal.capture().await?;
            let jpeg =
                stream::screenshot(capture, &d, u32::try_from(w)?, u32::try_from(h2)?).await?;
            json!({"data": jpeg})
        }
        "input" => {
            let d = display().await?;
            portal.perform(&d, &p["event"]).await?;
            json!({})
        }
        "uiTree" => a11y::frontmost(&display().await?).await?,
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

/// Setup on this computer (the host takes `requestPermission` only from loopback): the portals
/// when the desktop has them, otherwise X11.
async fn setup(h: &Arc<Helper>) -> Result<()> {
    if !portal::Portal::available().await {
        if backend::approve_x11(&h.portal, &data_dir())
            .await?
            .is_some()
        {
            h.end_sessions().await;
        }
        return Ok(());
    }
    let mut portal = h.portal.lock().await;
    // X11 was chosen because the portals were missing; now that they're here, setup uses them.
    let x11 = matches!(portal.as_ref(), Some(backend::Backend::X11(_)));
    if !x11 && portal.as_ref().is_some_and(backend::Backend::has_input) {
        return Ok(());
    }
    let restore = match portal.as_ref() {
        Some(backend::Backend::Portal(p)) => !p.is_active(),
        _ => true,
    };
    // On failure the current backend, X11 included, stays as it was.
    let next = portal::Portal::start(&data_dir(), restore).await?;
    if let Some(backend::Backend::Portal(previous)) = portal.as_ref() {
        if previous.is_active() {
            previous.session().close().await?;
        }
        h.sessions.close_all().await;
    }
    let previous = portal.replace(backend::Backend::Portal(next));
    drop(portal);
    if matches!(previous, Some(backend::Backend::X11(_))) {
        h.end_sessions().await;
    }
    Ok(())
}

impl Helper {
    async fn portal(&self) -> Result<backend::Backend> {
        let current = self.portal.lock().await.clone();
        match current {
            Some(backend) if backend.is_active() => Ok(backend),
            Some(backend::Backend::X11(x11)) => Err(anyhow!(
                "The X display {} isn't reachable; Codync reconnects when it's back.",
                x11.name()
            )),
            _ => Err(anyhow!(
                "Set up Computer access in Codync on this computer first."
            )),
        }
    }

    async fn send_status(&self) {
        let current = self.portal.lock().await.clone();
        if let Some(backend::Backend::X11(x11)) = &current
            && !x11.is_active()
        {
            // Video from a lost display would stay frozen: end it so viewers reconnect.
            self.end_sessions().await;
            backend::reconnect(&self.portal, x11, &data_dir()).await;
        }
        let current = self.portal.lock().await.clone();
        let x11 = matches!(current, Some(backend::Backend::X11(_)));
        let status = match &current {
            Some(backend) => backend.status().await,
            None => {
                json!({"platform": "linux", "permissionApp": "Codync Screen", "version": env!("CARGO_PKG_VERSION"), "trickle": true, "capture": false, "input": false, "displays": []})
            }
        };
        let mut last = self.last_status.lock().await;
        if last.as_ref() == Some(&status) {
            return;
        }
        // xrdp resized the X screen: streams keep the old size, so end them and let viewers
        // reconnect at the new one.
        if x11
            && last
                .as_ref()
                .is_some_and(|l| l["displays"] != status["displays"])
        {
            self.end_sessions().await;
        }
        *last = Some(status.clone());
        // Fails only once the host link is gone; the next link starts a new Helper that sends its status first.
        let _ = self.notify("status", status);
    }

    /// One notification to the host; fails once the host link is gone.
    fn notify(&self, method: &str, params: Value) -> Result<()> {
        let mut message = json!({"jsonrpc": "2.0", "method": method});
        message["params"] = params;
        self.out
            .send(message)
            .map_err(|_| anyhow!("the host link closed"))
    }

    /// Tells the host a session's state. It fails only once the host link is gone, and the host drops every session
    /// of a helper whose link ended, so a lost notice changes nothing there.
    fn notify_session(&self, id: &str, state: &str) {
        if let Err(error) = self.notify("session", json!({"session": id, "state": state})) {
            tracing::debug!(%error, session = id, state, "session notice not sent");
        }
    }

    /// Ends every session and tells the host, so viewers can reconnect.
    async fn end_sessions(&self) {
        for id in self.sessions.ids().await {
            self.sessions.close(&id).await;
            self.notify_session(&id, "closed");
        }
    }

    /// A session's WebRTC connection ended on its own.
    pub fn ended(self: &Arc<Self>, id: &str) {
        let this = self.clone();
        let id = id.to_owned();
        self.rt.spawn(async move {
            this.sessions.close(&id).await;
            this.notify_session(&id, "closed");
        });
    }

    async fn connected(&self, id: &str, peer: &gst::Element) {
        let live = self.sessions.is_live(id, peer).await;
        if live {
            self.notify_session(id, "connected");
        }
    }

    async fn ended_peer(&self, id: &str, peer: &gst::Element) {
        self.sessions
            .close_peer(id, peer, || {
                self.notify_session(id, "closed");
            })
            .await;
    }
}
