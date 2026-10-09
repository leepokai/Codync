//! Remote screen: the phone views and controls this computer over WebRTC, and
//! bots operate it through the built-in `computer` MCP server (`mcp.rs`, `computer.rs`).
//!
//! Capture, encoding and the phone's input live in a separate *screen helper*
//! that connects to `~/.codync/screen.sock` — on macOS `CodyncScreen.app`
//! (a signed bundle, so Screen Recording / Accessibility grants stick), on
//! Linux `codync-screen` (xdg portals + `GStreamer`). The host stays the only
//! network-facing process: it relays WebRTC signaling, gates access (on by
//! default where there is a desktop, toggled only from this computer) and arbitrates control between
//! the user and bots. Bots act through the computer-use driver (`cua.rs`), which Codync Screen
//! starts on macOS and the host starts on Windows and Linux.
//!
//! Helper protocol: newline-delimited JSON-RPC 2.0 over the socket.
//! - helper → host notifications
//!   - `status` [`HelperStatus`]
//!   - `session {session, state: "connected" | "closed"}`
//! - host → helper requests
//!   - `answer {session, sdp, display?, iceServers, maxBitrateBps, maxFramerate}` → `{sdp}`:
//!     non-trickle ICE, with short-lived STUN/TURN credentials for remote connections.
//!   - `close {session}`, `closeAll {}`
//!   - `focusedField {pid?}` → `{app, pid, url?, secure}`: the focused control of that app (the
//!     frontmost one by default), for `type_login`
//!   - macOS: `driver {}` → `{socket}`: starts the computer-use driver if it isn't running
//!   - Linux, for bots on desktops the driver doesn't support (`helper_tools.rs`):
//!     `screenshot {display, width, height}` → `{data}` (base64 JPEG, exactly that size);
//!     `input {display, event}` → `{}` (display points); `uiTree {display}` → `{app, tree}`;
//!     `openApp {name}` → `{}`
//!
//! The phone's input travels straight to the helper over WebRTC data channels
//! (`input-fast` unordered for moves, `input` reliable): `move`, `click`, `drag`, `scroll`,
//! `text`, `key` in display points, plus `down`/`up` for touch drags and `clipboard {text}`.

mod computer;
mod cua;
mod helper;
#[cfg(target_os = "linux")]
mod helper_tools;
mod permissions;
mod protocol;
mod viewer;

pub use computer::{computer, computer_tools};
pub use helper::serve_helpers;
#[cfg(target_os = "linux")]
pub use helper::supervise_linux_helper;
pub use viewer::{IceConfig, watch_viewer};

use crate::LockExt;
use anyhow::{Result, anyhow, bail};
use helper::{Link, graphical_session};
use protocol::HelperStatus;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// How long after its last computer call a bot still counts as "using the computer".
const AGENT_HOLD: Duration = Duration::from_secs(20);
/// Lets the UI react to an action before the follow-up screenshot.
#[cfg(target_os = "linux")]
const SETTLE: Duration = Duration::from_millis(400);
const HELPER_TIMEOUT: Duration = Duration::from_secs(15);
const KV_ENABLED: &str = "screen_enabled";

#[derive(Default)]
struct Control {
    /// The phone took over: bots may only look.
    user: bool,
    /// The bot using the computer, and when it last did.
    agent: Option<(String, Instant)>,
}

impl Control {
    fn active_agent(&self) -> Option<&str> {
        self.agent.as_ref().filter(|(_, t)| t.elapsed() < AGENT_HOLD).map(|(b, _)| b.as_str())
    }
}

struct Viewer {
    owner: String,
    ice: IceConfig,
    started: bool,
    pending_until: i64,
}

pub struct Screen {
    /// The user's choice; `None` until they make one (see [`Screen::enabled`]).
    enabled: Mutex<Option<bool>>,
    events: broadcast::Sender<Value>,
    link: Mutex<Option<Arc<Link>>>,
    status: Mutex<HelperStatus>,
    control: Mutex<Control>,
    sessions: Mutex<HashMap<String, Viewer>>,
    driver: cua::Driver,
    #[cfg_attr(windows, expect(dead_code, reason = "Windows has no screen helper yet"))]
    next_link: AtomicU64,
}

impl Screen {
    pub fn new(enabled: Option<bool>, events: broadcast::Sender<Value>) -> Self {
        Self {
            enabled: Mutex::new(enabled),
            events,
            link: Mutex::default(),
            status: Mutex::default(),
            control: Mutex::default(),
            sessions: Mutex::default(),
            driver: cua::Driver::default(),
            next_link: AtomicU64::new(1),
        }
    }

    pub fn load_enabled(store: &crate::store::Store) -> Option<bool> {
        match store.kv_get(KV_ENABLED).as_deref() {
            Some("1") => Some(true),
            Some("0") => Some(false),
            _ => None,
        }
    }

    /// On by default wherever there is a screen to share: always on macOS, and on Linux
    /// only while a graphical session is up (checked live, so a headless server stays off
    /// and a desktop that logs in after the host started turns on).
    pub fn enabled(&self) -> bool {
        (*self.enabled.locked()).unwrap_or_else(graphical_session)
    }

    /// What clients show: availability, permissions, displays, who's in control.
    pub fn state(&self) -> Value {
        let st = self.status.locked().clone();
        let connected = self.link.locked().is_some();
        let viewers = self.sessions.locked().values().filter(|v| v.started).count();
        let c = self.control.locked();
        json!({
            "enabled": self.enabled(),
            "connected": connected,
            "platform": st.platform,
            "permissionApp": st.permission_app,
            "capture": st.capture,
            "input": st.input,
            "displays": st.displays,
            "userControl": c.user,
            "agentBot": c.active_agent(),
            "computerUse": self.computer_use(),
            "viewers": viewers,
        })
    }

    fn emit(&self) {
        let _ = self.events.send(json!({"type": "screen", "screen": self.state()}));
    }

    fn link(&self) -> Result<Arc<Link>> {
        if !self.enabled() {
            bail!("Remote screen is turned off on this computer. Turn it on in Codync's menu there.");
        }
        self.link.locked().clone().ok_or_else(|| anyhow!("The screen helper isn't running on this computer."))
    }

    #[cfg(target_os = "linux")]
    fn display(&self, id: Option<u32>) -> Result<protocol::Display> {
        let st = self.status.locked();
        let found = match id {
            Some(id) => st.displays.iter().find(|d| d.id == id),
            None => st.displays.iter().find(|d| d.main).or_else(|| st.displays.first()),
        };
        found.cloned().ok_or_else(|| anyhow!("no such display"))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    pub(super) fn retina() -> protocol::Display {
        protocol::Display { id: 1, name: "Built-in".into(), width: 1512.0, height: 982.0, main: true }
    }
}
