//! Remote screen: the phone views and controls this computer over WebRTC, and
//! bots operate it through the built-in `computer` MCP server (`mcp.rs`).
//!
//! Capture, encoding and input injection live in a separate *screen helper*
//! that connects to `~/.codync/screen.sock` — on macOS `CodyncScreen.app`
//! (a signed bundle, so Screen Recording / Accessibility grants stick), on
//! Linux `codync-screen` (xdg portals + `GStreamer`). The host stays the only
//! network-facing process: it relays WebRTC signaling, gates access (off by
//! default, enabled only from this computer) and arbitrates control between
//! the user and bots.
//!
//! Helper protocol: newline-delimited JSON-RPC 2.0 over the socket.
//! - helper → host notifications
//!   - `status` [`HelperStatus`]
//!   - `session {session, state: "connected" | "closed"}`
//! - host → helper requests
//!   - `answer {session, sdp, display?}` → `{sdp}`: non-trickle; the offer and
//!     the answer carry every ICE candidate (host + TCP, no STUN/TURN: any
//!     network that reaches this API reaches the helper).
//!   - `close {session}`, `closeAll {}`
//!   - `screenshot {display, width, height}` → `{data}` (base64 JPEG, exactly
//!     that size)
//!   - `input {display, event}` → `{}`; `event` is an [`InputEvent`] in display points
//!   - `uiTree {display}` → `{app, tree}`: accessibility tree of the frontmost
//!     app; nodes are `{role, title?, value?, frame: [x, y, w, h], children?}`
//!     with frames in display points
//!   - `openApp {name}` → `{}`
//!
//! The phone's input travels straight to the helper over WebRTC data channels
//! (`input-fast` unordered for moves, `input` reliable) using the same
//! [`InputEvent`] JSON plus `down`/`up` for touch drags and `clipboard {text}`.

use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, oneshot};

/// Long edge of screenshots handed to agents: sharp enough to read UI text,
/// small enough to keep every turn cheap.
pub const SHOT_MAX_EDGE: f64 = 1280.0;
/// How long after its last computer call a bot still counts as "using the computer".
const AGENT_HOLD: Duration = Duration::from_secs(20);
/// Lets the UI react to an action before the follow-up screenshot.
const SETTLE: Duration = Duration::from_millis(400);
const HELPER_TIMEOUT: Duration = Duration::from_secs(15);
const KV_ENABLED: &str = "screen_enabled";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Size in points (the coordinate space of [`InputEvent`]).
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub main: bool,
}

impl Display {
    /// Screenshot scale: points → agent image pixels.
    fn shot_scale(&self) -> f64 {
        (SHOT_MAX_EDGE / self.width.max(self.height)).min(1.0)
    }

    /// Agent image size for this display.
    fn shot_size(&self) -> (u32, u32) {
        let s = self.shot_scale();
        // Display sizes are a few thousand points: always in range.
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        ((self.width * s).round() as u32, (self.height * s).round() as u32)
    }

    /// Agent image pixels → display points, rejecting points off the image.
    fn to_points(&self, x: f64, y: f64) -> Result<(f64, f64)> {
        let (width, height) = self.shot_size();
        if !(0.0..=f64::from(width)).contains(&x) || !(0.0..=f64::from(height)).contains(&y) {
            bail!("({x}, {y}) is outside the {width}×{height} screenshot");
        }
        // Per axis: the image size is rounded, so its aspect differs slightly from the display's.
        Ok((x * self.width / f64::from(width), y * self.height / f64::from(height)))
    }
}

/// What the helper reports about itself.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperStatus {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub displays: Vec<Display>,
    /// Screen capture is permitted.
    #[serde(default)]
    pub capture: bool,
    /// Input injection (and the accessibility tree) is permitted.
    #[serde(default)]
    pub input: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    /// ⌘ on macOS, Super on Linux.
    Cmd,
    Option,
    Ctrl,
    Shift,
}

/// One input action for the helper, in display points.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum InputEvent {
    Move {
        x: f64,
        y: f64,
    },
    Click {
        x: f64,
        y: f64,
        button: Button,
        count: u8,
        modifiers: Vec<Modifier>,
    },
    Drag {
        x: f64,
        y: f64,
        to_x: f64,
        to_y: f64,
    },
    /// Scroll by `dx`/`dy` lines (positive = right/down) with the pointer at `x`,`y`.
    Scroll {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    Text {
        text: String,
    },
    Key {
        key: String,
        modifiers: Vec<Modifier>,
    },
}

/// Named keys the helpers understand; any other key is a single character.
const NAMED_KEYS: &[&str] = &[
    "return",
    "tab",
    "space",
    "escape",
    "delete",
    "forwardDelete",
    "left",
    "right",
    "up",
    "down",
    "home",
    "end",
    "pageUp",
    "pageDown",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

/// Modifier names with common aliases, deduplicated.
fn parse_modifiers(names: &[impl AsRef<str>]) -> Result<Vec<Modifier>> {
    let mut out = vec![];
    for m in names {
        let m = match m.as_ref().to_lowercase().as_str() {
            "cmd" | "command" | "meta" | "super" | "win" => Modifier::Cmd,
            "option" | "opt" | "alt" => Modifier::Option,
            "ctrl" | "control" => Modifier::Ctrl,
            "shift" => Modifier::Shift,
            other => bail!("unknown modifier `{other}` (use cmd, option, ctrl, shift)"),
        };
        if !out.contains(&m) {
            out.push(m);
        }
    }
    Ok(out)
}

/// `"cmd+shift+t"` → key `t` with ⌘⇧. Accepts common aliases (`ctrl`, `alt`, `enter`, `esc`, …).
pub fn parse_keys(combo: &str) -> Result<(String, Vec<Modifier>)> {
    let parts: Vec<&str> = combo.split('+').map(str::trim).collect();
    let (key, mods) =
        parts.split_last().filter(|(k, _)| !k.is_empty()).ok_or_else(|| anyhow!("no key in `{combo}`"))?;
    let modifiers = parse_modifiers(mods)?;
    let lower = key.to_lowercase();
    let key = match lower.as_str() {
        "enter" => "return".to_owned(),
        "esc" => "escape".to_owned(),
        "backspace" => "delete".to_owned(),
        "del" | "forwarddelete" => "forwardDelete".to_owned(),
        "pageup" | "pgup" => "pageUp".to_owned(),
        "pagedown" | "pgdn" => "pageDown".to_owned(),
        "arrowleft" => "left".to_owned(),
        "arrowright" => "right".to_owned(),
        "arrowup" => "up".to_owned(),
        "arrowdown" => "down".to_owned(),
        _ if NAMED_KEYS.contains(&lower.as_str()) => lower,
        _ if key.chars().count() == 1 => lower,
        _ => bail!("unknown key `{key}`"),
    };
    Ok((key, modifiers))
}

/// A `computer` MCP tool call, as the MCP server forwards it (`{name, arguments}`).
#[derive(Debug, Deserialize)]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
pub enum ComputerTool {
    Screenshot {
        display: Option<u32>,
    },
    Click {
        x: f64,
        y: f64,
        #[serde(default)]
        button: Button,
        count: Option<u8>,
        #[serde(default)]
        modifiers: Vec<String>,
        display: Option<u32>,
    },
    Move {
        x: f64,
        y: f64,
        display: Option<u32>,
    },
    Drag {
        x: f64,
        y: f64,
        to_x: f64,
        to_y: f64,
        display: Option<u32>,
    },
    Scroll {
        x: f64,
        y: f64,
        #[serde(default)]
        dx: f64,
        #[serde(default)]
        dy: f64,
        display: Option<u32>,
    },
    Type {
        text: String,
    },
    Key {
        keys: String,
    },
    UiTree {
        display: Option<u32>,
    },
    OpenApp {
        name: String,
    },
}

impl ComputerTool {
    /// Tools that only look: allowed while the user has control.
    fn read_only(&self) -> bool {
        matches!(self, Self::Screenshot { .. } | Self::UiTree { .. })
    }
}

type Pending = Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>;

/// One connected screen helper.
struct Link {
    id: u64,
    tx: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicI64,
}

impl Link {
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.locked().insert(id, tx);
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        if self.tx.send(line).is_err() {
            self.pending.locked().remove(&id);
            bail!("the screen helper disconnected");
        }
        match tokio::time::timeout(HELPER_TIMEOUT, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(message))) => Err(anyhow!(message)),
            Ok(Err(_)) => bail!("the screen helper disconnected"),
            Err(_) => {
                self.pending.locked().remove(&id);
                bail!("the screen helper didn't answer `{method}` in time")
            }
        }
    }
}

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

pub struct Screen {
    enabled: AtomicBool,
    events: broadcast::Sender<Value>,
    link: Mutex<Option<Arc<Link>>>,
    status: Mutex<HelperStatus>,
    control: Mutex<Control>,
    sessions: Mutex<HashSet<String>>,
    next_link: AtomicU64,
}

impl Screen {
    pub fn new(enabled: bool, events: broadcast::Sender<Value>) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            events,
            link: Mutex::default(),
            status: Mutex::default(),
            control: Mutex::default(),
            sessions: Mutex::default(),
            next_link: AtomicU64::new(1),
        }
    }

    pub fn load_enabled(store: &crate::store::Store) -> bool {
        store.kv_get(KV_ENABLED).as_deref() == Some("1")
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// What clients show: availability, permissions, displays, who's in control.
    pub fn state(&self) -> Value {
        let st = self.status.locked().clone();
        let connected = self.link.locked().is_some();
        let c = self.control.locked();
        json!({
            "enabled": self.enabled(),
            "connected": connected,
            "platform": st.platform,
            "capture": st.capture,
            "input": st.input,
            "displays": st.displays,
            "userControl": c.user,
            "agentBot": c.active_agent(),
            "viewers": self.sessions.locked().len(),
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

    fn display(&self, id: Option<u32>) -> Result<Display> {
        let st = self.status.locked();
        let found = match id {
            Some(id) => st.displays.iter().find(|d| d.id == id),
            None => st.displays.iter().find(|d| d.main).or_else(|| st.displays.first()),
        };
        found.cloned().ok_or_else(|| anyhow!("no such display"))
    }

    // MARK: phone

    /// Answers a WebRTC offer from the phone (a new session, or an ICE restart of `session`).
    pub async fn offer(&self, sdp: &str, session: Option<&str>, display: Option<u32>) -> Result<Value> {
        let link = self.link()?;
        if !self.status.locked().capture {
            bail!("Codync isn't allowed to record this computer's screen yet. Allow it in System Settings there.");
        }
        let session = session.map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned);
        let res = link.request("answer", json!({"session": session, "sdp": sdp, "display": display})).await?;
        let answer = res["sdp"].as_str().ok_or_else(|| anyhow!("the screen helper sent no answer"))?;
        if self.sessions.locked().insert(session.clone()) {
            self.emit();
        }
        Ok(json!({"session": session, "sdp": answer}))
    }

    pub async fn close(&self, session: &str) -> Result<()> {
        let link = self.link.locked().clone();
        if let Some(link) = link {
            link.request("close", json!({"session": session})).await?;
        }
        self.session_closed(session);
        Ok(())
    }

    fn session_closed(&self, session: &str) {
        let removed = {
            let mut sessions = self.sessions.locked();
            let removed = sessions.remove(session);
            if sessions.is_empty() {
                // Nobody is watching: hand control back to the bots.
                self.control.locked().user = false;
            }
            removed
        };
        if removed {
            self.emit();
        }
    }

    pub fn takeover(&self, on: bool) {
        self.control.locked().user = on;
        self.emit();
    }

    /// Only callable from this computer (see `api.rs`).
    pub async fn set_enabled(&self, store: &crate::store::Store, on: bool) -> Result<()> {
        store.kv_set(KV_ENABLED, if on { "1" } else { "0" })?;
        self.enabled.store(on, Ordering::Relaxed);
        if !on {
            let link = self.link.locked().clone();
            if let Some(link) = link
                && let Err(error) = link.request("closeAll", json!({})).await
            {
                tracing::warn!(error = format!("{error:#}"), "couldn't close screen sessions");
            }
            self.sessions.locked().clear();
            *self.control.locked() = Control::default();
        }
        self.emit();
        Ok(())
    }

    // MARK: bots

    /// Marks `bot` as the one using the computer. Returns whether that's news.
    fn claim(&self, bot: &str, read_only: bool) -> Result<bool> {
        let mut c = self.control.locked();
        if c.user && !read_only {
            bail!(
                "The user has taken over the screen from their phone. Wait until they hand it back, or ask them what to do."
            );
        }
        if let Some(other) = c.active_agent()
            && other != bot
        {
            bail!("Another bot is using the computer right now. Try again in a minute.");
        }
        let fresh = c.active_agent().is_none();
        c.agent = Some((bot.to_owned(), Instant::now()));
        Ok(fresh)
    }
}

/// Runs one `computer` tool for `bot`: MCP `CallToolResult` content.
pub async fn computer(hub: &Arc<Hub>, bot: &str, tool: ComputerTool) -> Result<Value> {
    let row = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    if !row.config.computer {
        bail!("Computer use is turned off for this bot.");
    }
    let screen = &hub.screen;
    let link = screen.link()?;
    if screen.claim(bot, tool.read_only())? {
        screen.emit();
        let screen = screen.clone();
        tokio::spawn(async move {
            // Say when the bot stops, so phones drop the live chip.
            loop {
                tokio::time::sleep(AGENT_HOLD).await;
                if screen.control.locked().active_agent().is_none() {
                    screen.emit();
                    return;
                }
            }
        });
    }
    let display = |id| screen.display(id);
    let act = |id: Option<u32>, event: InputEvent| {
        let link = link.clone();
        async move {
            let d = display(id)?;
            link.request("input", json!({"display": d.id, "event": event})).await?;
            tokio::time::sleep(SETTLE).await;
            shot(&link, &d).await
        }
    };
    match tool {
        ComputerTool::Screenshot { display: id } => shot(&link, &display(id)?).await,
        ComputerTool::Click { x, y, button, count, modifiers, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            let modifiers = parse_modifiers(&modifiers)?;
            let count = count.unwrap_or(1).clamp(1, 3);
            act(Some(d.id), InputEvent::Click { x, y, button, count, modifiers }).await
        }
        ComputerTool::Move { x, y, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            act(Some(d.id), InputEvent::Move { x, y }).await
        }
        ComputerTool::Drag { x, y, to_x, to_y, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            let (to_x, to_y) = d.to_points(to_x, to_y)?;
            act(Some(d.id), InputEvent::Drag { x, y, to_x, to_y }).await
        }
        ComputerTool::Scroll { x, y, dx, dy, display: id } => {
            let d = display(id)?;
            let (x, y) = d.to_points(x, y)?;
            act(Some(d.id), InputEvent::Scroll { x, y, dx, dy }).await
        }
        ComputerTool::Type { text } => act(None, InputEvent::Text { text }).await,
        ComputerTool::Key { keys } => {
            let (key, modifiers) = parse_keys(&keys)?;
            act(None, InputEvent::Key { key, modifiers }).await
        }
        ComputerTool::UiTree { display: id } => {
            let d = display(id)?;
            let mut res = link.request("uiTree", json!({"display": d.id})).await?;
            scale_frames(&mut res["tree"], d.shot_scale());
            let text = format!(
                "Accessibility tree of {} (frames are [x, y, width, height] in screenshot pixels):\n{}",
                res["app"].as_str().unwrap_or("the frontmost app"),
                res["tree"]
            );
            Ok(json!([{"type": "text", "text": text}]))
        }
        ComputerTool::OpenApp { name } => {
            link.request("openApp", json!({"name": name})).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            shot(&link, &display(None)?).await
        }
    }
}

async fn shot(link: &Link, d: &Display) -> Result<Value> {
    let (width, height) = d.shot_size();
    let res = link.request("screenshot", json!({"display": d.id, "width": width, "height": height})).await?;
    let data = res["data"].as_str().ok_or_else(|| anyhow!("the screen helper sent no image"))?;
    Ok(json!([
        {"type": "image", "data": data, "mimeType": "image/jpeg"},
        {"type": "text", "text": format!(
            "Display {} \"{}\", {width}×{height}. Coordinates for click/move/drag/scroll are pixels in this image.",
            d.id, d.name
        )},
    ]))
}

/// Display points → screenshot pixels, for every `frame` in an accessibility tree.
fn scale_frames(node: &mut Value, s: f64) {
    if let Some(frame) = node.get_mut("frame").and_then(Value::as_array_mut) {
        for v in frame.iter_mut() {
            if let Some(f) = v.as_f64() {
                *v = json!((f * s).round());
            }
        }
    }
    if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
        for c in children {
            scale_frames(c, s);
        }
    }
}

// MARK: helper socket

/// Accepts screen helpers on `~/.codync/screen.sock`. The newest connection wins.
pub async fn serve_helpers(screen: Arc<Screen>) {
    let path = crate::service::data_dir().join("screen.sock");
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "can't listen for the screen helper");
            return;
        }
    };
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(%error, "couldn't restrict the screen helper socket");
        }
    }
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(run_link(screen.clone(), stream));
            }
            Err(error) => {
                tracing::warn!(%error, "screen helper accept failed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// Linux: the host runs `codync-screen` itself while Remote screen is on (on macOS
/// launchd runs Codync Screen for the app). Restarts it if it exits; stops it when turned off.
#[cfg(target_os = "linux")]
pub async fn supervise_linux_helper(screen: Arc<Screen>) {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("codync-screen")));
    let program = exe.filter(|p| p.is_file()).unwrap_or_else(|| "codync-screen".into());
    let mut child: Option<tokio::process::Child> = None;
    let mut backoff = Duration::from_secs(1);
    loop {
        let running = child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
        if screen.enabled() && !running {
            match tokio::process::Command::new(&program).kill_on_drop(true).spawn() {
                Ok(c) => child = Some(c),
                Err(error) => tracing::warn!(%error, program = %program.display(), "can't start codync-screen"),
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }
        if !screen.enabled()
            && let Some(mut c) = child.take()
        {
            let _ = c.kill().await;
        }
        if running && screen.link.locked().is_some() {
            backoff = Duration::from_secs(1);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn run_link(screen: Arc<Screen>, stream: UnixStream) {
    let (rd, mut wr) = stream.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let link = Arc::new(Link {
        id: screen.next_link.fetch_add(1, Ordering::Relaxed),
        tx,
        pending: Mutex::default(),
        next_id: AtomicI64::new(1),
    });
    *screen.link.locked() = Some(link.clone());
    tracing::info!("screen helper connected");
    let writer = tokio::spawn(async move {
        while let Some(mut line) = rx.recv().await {
            line.push('\n');
            if wr.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let mut lines = BufReader::new(rd).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            tracing::debug!(line, "ignoring non-JSON from the screen helper");
            continue;
        };
        match (msg["method"].as_str(), msg["id"].as_i64()) {
            (None, Some(id)) => {
                if let Some(waiter) = link.pending.locked().remove(&id) {
                    let res = match msg.get("error") {
                        Some(e) => Err(e["message"].as_str().map_or_else(|| e.to_string(), str::to_owned)),
                        None => Ok(msg["result"].clone()),
                    };
                    let _ = waiter.send(res);
                }
            }
            (Some("status"), _) => match serde_json::from_value::<HelperStatus>(msg["params"].clone()) {
                Ok(st) => {
                    *screen.status.locked() = st;
                    screen.emit();
                }
                Err(error) => tracing::warn!(%error, "bad status from the screen helper"),
            },
            (Some("session"), _) => {
                if msg["params"]["state"] == "closed"
                    && let Some(s) = msg["params"]["session"].as_str()
                {
                    screen.session_closed(s);
                }
            }
            (Some(method), _) => tracing::debug!(method, "unknown screen helper notification"),
            _ => {}
        }
    }
    writer.abort();
    link.pending.locked().clear();
    let current = {
        let mut slot = screen.link.locked();
        let current = slot.as_ref().is_some_and(|l| l.id == link.id);
        if current {
            *slot = None;
        }
        current
    };
    if current {
        *screen.status.locked() = HelperStatus::default();
        screen.sessions.locked().clear();
        screen.control.locked().user = false;
        screen.emit();
    }
    tracing::info!("screen helper disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retina() -> Display {
        Display { id: 1, name: "Built-in".into(), width: 1512.0, height: 982.0, main: true }
    }

    #[test]
    fn screenshot_coordinates_map_to_points() {
        let d = retina();
        assert_eq!(d.shot_size(), (1280, 831));
        assert_eq!(d.to_points(1280.0, 831.0).unwrap(), (1512.0, 982.0));
        let (x, y) = d.to_points(640.0, 415.5).unwrap();
        assert!((x - 756.0).abs() < 0.01 && (y - 491.0).abs() < 0.01);
        assert!(d.to_points(1281.0, 10.0).is_err());
        // Small displays aren't upscaled.
        let small = Display { width: 800.0, height: 600.0, ..retina() };
        assert_eq!(small.shot_size(), (800, 600));
    }

    #[test]
    fn key_combos_parse() {
        assert_eq!(parse_keys("cmd+shift+T").unwrap(), ("t".into(), vec![Modifier::Cmd, Modifier::Shift]));
        assert_eq!(parse_keys("Enter").unwrap(), ("return".into(), vec![]));
        assert_eq!(parse_keys("ctrl+alt+Delete").unwrap(), ("delete".into(), vec![Modifier::Ctrl, Modifier::Option]));
        assert_eq!(parse_keys("cmd++").unwrap_err().to_string(), "no key in `cmd++`");
        assert!(parse_keys("hyper+x").is_err());
        assert!(parse_keys("cmd+banana").is_err());
    }

    #[test]
    fn tools_parse_from_mcp_calls() {
        let t: ComputerTool =
            serde_json::from_value(json!({"name": "click", "arguments": {"x": 1, "y": 2, "button": "right"}})).unwrap();
        assert!(matches!(t, ComputerTool::Click { button: Button::Right, count: None, .. }));
        let t: ComputerTool = serde_json::from_value(json!({"name": "screenshot", "arguments": {}})).unwrap();
        assert!(t.read_only());
    }

    #[test]
    fn input_events_serialize_for_helpers() {
        let e = InputEvent::Drag { x: 1.0, y: 2.0, to_x: 3.0, to_y: 4.0 };
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({"type": "drag", "x": 1.0, "y": 2.0, "toX": 3.0, "toY": 4.0})
        );
    }

    #[test]
    fn user_takeover_blocks_bot_actions_but_not_looking() {
        let (tx, _) = broadcast::channel(8);
        let s = Screen::new(true, tx);
        assert!(s.claim("a", false).unwrap());
        assert!(!s.claim("a", false).unwrap());
        assert!(s.claim("b", true).is_err(), "one bot at a time");
        s.takeover(true);
        assert!(s.claim("a", false).is_err());
        assert!(s.claim("a", true).is_ok());
    }

    #[test]
    fn frames_scale_recursively() {
        let mut t = json!({"frame": [100, 50, 200, 20], "children": [{"frame": [10.0, 10.0, 5.0, 5.0]}]});
        scale_frames(&mut t, 0.5);
        assert_eq!(t["frame"], json!([50.0, 25.0, 100.0, 10.0]));
        assert_eq!(t["children"][0]["frame"], json!([5.0, 5.0, 3.0, 3.0]));
    }

    #[tokio::test]
    async fn helper_roundtrip_over_the_socket() {
        let (tx, _) = broadcast::channel(8);
        let screen = Arc::new(Screen::new(true, tx));
        let (a, b) = UnixStream::pair().unwrap();
        // Fake helper: reports a display, then answers every request with its method name.
        tokio::spawn(async move {
            let (rd, mut wr) = b.into_split();
            let status = json!({"jsonrpc": "2.0", "method": "status", "params": {"platform": "test", "capture": true, "displays": [retina()]}});
            wr.write_all(format!("{status}\n").as_bytes()).await.unwrap();
            let mut lines = BufReader::new(rd).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let m: Value = serde_json::from_str(&l).unwrap();
                let res = json!({"jsonrpc": "2.0", "id": m["id"], "result": {"sdp": format!("answer to {}", m["params"]["sdp"].as_str().unwrap_or_default())}});
                wr.write_all(format!("{res}\n").as_bytes()).await.unwrap();
            }
        });
        tokio::spawn(run_link(screen.clone(), a));
        for _ in 0..50 {
            if screen.status.locked().capture {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let res = screen.offer("offer", None, None).await.unwrap();
        assert_eq!(res["sdp"], "answer to offer");
        assert_eq!(screen.state()["viewers"], 1);
        assert_eq!(screen.state()["displays"][0]["name"], "Built-in");
    }
}
