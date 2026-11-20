//! `codync-screen`: Remote screen for Linux. The host (`codync-host`) starts it
//! while Remote screen is on; it connects to `~/.codync/screen.sock` and serves
//! the same JSON-RPC as the macOS helper (see `host/src/screen/mod.rs`):
//! capture and input through the xdg `RemoteDesktop` + `ScreenCast` portals
//! (approved once, then restored from a saved token) or, without them, straight
//! from the X display (`x11/`), video as H.264 over GStreamer `webrtcbin`, and
//! the accessibility tree over AT-SPI.

mod a11y;
mod backend;
mod candidates;
mod connection;
mod input;
mod link;
mod negotiation;
mod portal;
mod portal_input;
mod sessions;
mod stream;
mod video;
mod x11;

use anyhow::{Context, Result, anyhow};
use backend::{Backend, Display};
use input::{Event, Press};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, PoisonError};
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

/// The press held down, and the viewer session that pressed it (`None`: a bot's `input`).
#[derive(Default)]
struct Held {
    owner: Option<String>,
    press: Option<(Display, Press)>,
}

pub struct Helper {
    /// `None` until the portal is set up, or while no X display is reachable (`current_backend`).
    backend: Arc<Mutex<Option<Backend>>>,
    /// The session has screen sharing portals (once seen, they stay): use them, never the X display.
    portals: AtomicBool,
    /// A viewer's `Down` on its display, until its `Up`.
    held: std::sync::Mutex<Held>,
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
    // Starting the helper must not display a system dialog. Setup explicitly opens a session.
    let backend = Arc::new(Mutex::new(None));
    loop {
        match UnixStream::connect(data_dir().join("screen.sock")).await {
            Ok(sock) => {
                tracing::info!("connected to codync-host");
                if let Err(error) = serve(sock, &backend).await {
                    tracing::warn!(error = format!("{error:#}"), "host link ended");
                }
            }
            Err(error) => tracing::debug!(%error, "codync-host isn't listening yet"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn serve(sock: UnixStream, backend: &Arc<Mutex<Option<Backend>>>) -> Result<()> {
    let (out, outbox) = mpsc::unbounded_channel::<Value>();
    let helper = Arc::new(Helper {
        backend: backend.clone(),
        portals: AtomicBool::default(),
        held: std::sync::Mutex::default(),
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
    helper.release_held(None).await;
    result
}

async fn handle(h: &Arc<Helper>, method: &str, p: &Value) -> Result<Value> {
    if method == "requestPermission" {
        if p["permission"] != "portal" {
            return Err(anyhow!("Linux uses the screen sharing portal"));
        }
        h.request_permission().await?;
        h.send_status().await;
        return Ok(json!({}));
    }
    // Close requests remain valid before setup; all other methods require a granted session.
    if method == "closeAll" {
        h.sessions.close_all().await;
        h.release_held(None).await;
        return Ok(json!({}));
    }
    if method == "close" {
        if let Some(id) = p["session"].as_str() {
            h.sessions.close(id).await;
            h.release_held(Some(id)).await;
        }
        return Ok(json!({}));
    }
    let backend = h.backend().await?;
    let display = || backend.display(p["display"].as_u64());
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
                    let source = backend.source(&d).await?;
                    let session = stream::Session::new(creating, &d, &source, h, p)?;
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
            let source = backend.source(&display()?).await?;
            let jpeg = stream::screenshot(source, u32::try_from(w)?, u32::try_from(h2)?).await?;
            json!({"data": jpeg})
        }
        "input" => {
            h.input(&backend, &display()?, None, Event::parse(&p["event"])?)
                .await?;
            json!({})
        }
        "uiTree" => a11y::frontmost(&display()?).await?,
        "focusedField" => a11y::focused_field().await?,
        "openApp" => {
            input::open_app(
                &backend,
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
    async fn backend(&self) -> Result<Backend> {
        self.current_backend()
            .await
            .clone()
            .filter(Backend::is_active)
            .ok_or_else(|| anyhow!("Set up Computer access in Codync on this computer first."))
    }

    /// One input event from `owner`'s session. An `Up` lets go of the modifiers its `Down`
    /// pressed, not the ones it carries: the user may let go of a modifier before the button.
    async fn input(
        &self,
        backend: &Backend,
        d: &Display,
        owner: Option<&str>,
        event: Event,
    ) -> Result<()> {
        let event = {
            let mut held = self.held();
            if matches!(event, Event::Down(_)) {
                held.owner = owner.map(str::to_owned);
            }
            input::track_held(&mut held.press, d, event)
        };
        backend.input(d, &event).await
    }

    /// Lets go of a `Down` whose viewer left before its `Up`: `owner`'s only, so a viewer leaving
    /// never ends another one's drag; `None` lets go of any.
    async fn release_held(&self, owner: Option<&str>) {
        let Some((d, press)) = ({
            let mut held = self.held();
            if owner.is_some() && held.owner.as_deref() != owner {
                return;
            }
            held.press.take()
        }) else {
            return;
        };
        let released = match self.backend().await {
            Ok(backend) => backend.input(&d, &Event::Up(press)).await,
            Err(error) => Err(error),
        };
        if let Err(error) = released {
            tracing::debug!(
                error = format!("{error:#}"),
                "couldn't let go of a held button"
            );
        }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn send_status(&self) {
        let status = self
            .current_backend()
            .await
            .as_ref()
            .map_or_else(|| backend::status(&[], false, false), Backend::status);
        let mut last = self.last_status.lock().await;
        if last.as_ref() == Some(&status) {
            return;
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

    /// A session's WebRTC connection ended on its own.
    pub fn ended(self: &Arc<Self>, id: &str) {
        let this = self.clone();
        let id = id.to_owned();
        self.rt.spawn(async move {
            this.sessions.close(&id).await;
            this.notify_session(&id, "closed");
            this.release_held(Some(&id)).await;
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
        self.release_held(Some(id)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_viewer_leaving_lets_go_only_of_its_own_press() {
        let (out, _events) = mpsc::unbounded_channel();
        let helper = Helper {
            backend: Arc::new(Mutex::new(None)),
            portals: AtomicBool::new(false),
            held: std::sync::Mutex::default(),
            last_status: Mutex::new(None),
            sessions: sessions::Sessions::default(),
            out,
            rt: tokio::runtime::Handle::current(),
        };
        let display = Display {
            id: 0,
            x: 0,
            y: 0,
            width: 100.0,
            height: 100.0,
        };
        let press = Press {
            at: (1.0, 1.0),
            button: input::Button::Left,
            modifiers: vec![],
        };
        *helper.held() = Held {
            owner: Some("a".into()),
            press: Some((display, press)),
        };
        helper.release_held(Some("b")).await;
        assert!(
            helper.held().press.is_some(),
            "another viewer's drag goes on"
        );
        helper.release_held(Some("a")).await;
        assert!(
            helper.held().press.is_none(),
            "its own viewer leaving lets go"
        );
    }
}
