//! Where capture and input come from: the xdg portals when the desktop offers them, otherwise
//! X11 when an X server with XTest is reachable and someone set it up on this computer.

use crate::input::{self, PortalInput};
use crate::portal::{Display, Portal};
use crate::stream::Capture;
use crate::x11::{X11, X11Input, consent, display};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

pub type Slot = Arc<Mutex<Option<Backend>>>;

/// Failed reconnections to the same display before looking for another one: after a relogin
/// xrdp may give the new session `:11` instead of `:10`.
const REPICK_AFTER: u32 = 3;

#[derive(Clone)]
pub enum Backend {
    Portal(Portal),
    X11(X11),
}

/// The first candidate display that connects, or why none did. With `granted`, only the display
/// consent was given on or a later one (see `display::follows_consent`); never `besides`.
async fn find_x11(granted: Option<u32>, besides: Option<&str>) -> Result<X11> {
    connect_first(
        display::candidates()
            .into_iter()
            .filter(|name| granted.is_none_or(|g| display::follows_consent(name, g)))
            .filter(|name| besides.is_none_or(|b| display::number(name) != display::number(b))),
    )
    .await
}

async fn connect_first(candidates: impl IntoIterator<Item = String>) -> Result<X11> {
    let mut last = anyhow!("no screen sharing portal and no X11 session to share");
    for name in candidates {
        match X11::connect(name).await {
            Ok(x11) => return Ok(x11),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// X11 at startup, only when the portals are missing and X11 was set up here before.
/// Probing creates no portal session, so it never opens a dialog.
pub async fn x11_fallback(data: &Path) -> Option<Backend> {
    let granted = consent::granted(data)?;
    if Portal::available().await {
        return None;
    }
    match find_x11(Some(granted), None).await {
        Ok(x11) => {
            tracing::info!(
                display = x11.name(),
                "no screen sharing portal: using X11 (ximagesrc + XTest)"
            );
            Some(Backend::X11(x11))
        }
        Err(error) => {
            // `wait_for_x11` keeps looking and warns once if it still can't find one.
            tracing::debug!(
                error = format!("{error:#}"),
                "no screen sharing portal and no usable X display yet"
            );
            None
        }
    }
}

/// A service can start before its X session (xrdp creates it at boot, and `DISPLAY` reaches the
/// service manager only later): keep looking until X11 works, setup picks the portals, or the
/// desktop turns out to have them.
pub async fn wait_for_x11(slot: Slot, data: PathBuf) {
    let mut warned = false;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let Some(granted) = consent::granted(&data) else {
            return;
        };
        if slot.lock().await.is_some() || Portal::available().await {
            return;
        }
        match find_x11(Some(granted), None).await {
            Ok(x11) => {
                let mut current = slot.lock().await;
                if current.is_none() {
                    tracing::info!(
                        display = x11.name(),
                        "X display ready: using X11 (ximagesrc + XTest)"
                    );
                    *current = Some(Backend::X11(x11));
                }
                return;
            }
            Err(error) if !warned => {
                warned = true;
                tracing::warn!(error = format!("{error:#}"), "waiting for an X display");
            }
            Err(error) => tracing::debug!(error = format!("{error:#}"), "still no X display"),
        }
    }
}

/// Setup on a desktop without portals: records the local consent for the display it connects to.
/// Returns the backend it replaced, whose sessions the caller ends.
pub async fn approve_x11(slot: &Slot, data: &Path) -> Result<Option<Backend>> {
    let grant = |x11: &X11| {
        let number = display::number(x11.name())
            .with_context(|| format!("can't tell the number of X display {}", x11.name()))?;
        consent::grant(data, number)
    };
    let current = slot.lock().await.clone();
    match &current {
        Some(Backend::X11(x11)) if x11.is_active() => {
            grant(x11)?;
            return Ok(None);
        }
        // The portals were here, so they are only slow to answer: X11 would bypass their dialog.
        Some(Backend::Portal(_)) => {
            bail!("the screen sharing portal isn't answering; try again in a moment")
        }
        _ => {}
    }
    let x11 = connect_first(display::for_setup()?).await?;
    grant(&x11)?;
    tracing::info!(
        display = x11.name(),
        "X11 screen sharing set up on this computer"
    );
    Ok(slot.lock().await.replace(Backend::X11(x11)))
}

/// After an X connection broke (xrdp ended or restarted the session): a new connection to the
/// same display, or once that has failed a few times, to a later one the consent covers. Portal
/// sessions are restored only through setup, and nothing reconnects once the consent file is
/// gone. Puts the new backend in `slot` if `dead` is still there.
pub async fn reconnect(slot: &Slot, dead: &X11, data: &Path) -> bool {
    let Some(granted) = consent::granted(data) else {
        return false;
    };
    let next = match X11::connect(dead.name().to_owned()).await {
        Ok(x11) => Some(x11),
        Err(error) => {
            tracing::debug!(
                error = format!("{error:#}"),
                display = dead.name(),
                "X display still unreachable"
            );
            // It was just tried: look only at the other displays, so a frozen one can't hold up
            // the search.
            if dead.reconnect_failed() >= REPICK_AFTER {
                find_x11(Some(granted), Some(dead.name())).await.ok()
            } else {
                None
            }
        }
    };
    let Some(next) = next else { return false };
    // A connection that timed out mid-gesture may have left keys or buttons down.
    if let Err(error) = next.release_held().await {
        tracing::debug!(error = format!("{error:#}"), "couldn't release held keys");
    }
    let mut current = slot.lock().await;
    if !matches!(current.as_ref(), Some(Backend::X11(x)) if x.same(dead)) {
        return false;
    }
    if next.name() == dead.name() {
        tracing::info!(display = next.name(), "reconnected to the X display");
    } else {
        tracing::warn!(
            from = dead.name(),
            to = next.name(),
            "the X display moved: sharing the new one"
        );
    }
    *current = Some(Backend::X11(next));
    true
}

impl Backend {
    pub fn is_active(&self) -> bool {
        match self {
            Self::Portal(p) => p.is_active(),
            Self::X11(x) => x.is_active(),
        }
    }

    pub fn has_input(&self) -> bool {
        match self {
            Self::Portal(p) => p.has_input(),
            Self::X11(x) => x.is_active(),
        }
    }

    pub async fn status(&self) -> Value {
        match self {
            Self::Portal(p) => p.status(),
            Self::X11(x) => {
                let displays = x.displays().await;
                let active = x.is_active();
                crate::portal::status_json(active, active, &displays)
            }
        }
    }

    pub async fn display(&self, id: Option<u64>) -> Result<Display> {
        match self {
            Self::Portal(p) => p.display(id),
            Self::X11(x) => x.display(id).await,
        }
    }

    /// A fresh capture source for one pipeline.
    pub async fn capture(&self) -> Result<Capture> {
        Ok(match self {
            Self::Portal(p) => Capture::PipeWire(p.pipewire_fd().await?),
            Self::X11(x) => Capture::X11(x.name().to_owned()),
        })
    }

    pub async fn perform(&self, display: &Display, event: &Value) -> Result<()> {
        match self {
            Self::Portal(portal) => {
                input::perform(
                    &PortalInput {
                        portal,
                        node: display.node,
                    },
                    event,
                )
                .await
            }
            Self::X11(x11) => {
                // The X screen can change size under a stream; clamp to the size it has now.
                let display = x11.input_display(display.id).await?;
                input::perform(
                    &X11Input {
                        x11,
                        display: &display,
                    },
                    event,
                )
                .await
            }
        }
    }
}
