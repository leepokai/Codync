//! The helper's connection to the host and the requests it serves.

use crate::candidates::Candidate;
use crate::displays::{self, Monitor};
use crate::link::{self, Outbox};
use crate::peer::Session;
use crate::sessions::Sessions;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::windows::named_pipe::ClientOptions;
use tokio::sync::{Mutex, mpsc};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

pub struct Helper {
    pub out: Outbox,
    sessions: Sessions,
    last_status: Mutex<Option<Value>>,
}

fn data_dir() -> PathBuf {
    std::env::var_os("CODYNC_HOME").map_or_else(
        || {
            dirs::home_dir()
                .expect("the user profile must exist")
                .join(".codync")
        },
        PathBuf::from,
    )
}

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "codync_screen=info".into()),
        )
        .init();
    // Monitor sizes and pointer positions are physical pixels, whatever the display scaling.
    // SAFETY: first calls of the process, before any window or monitor query.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            .context("making the helper DPI aware")?;
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .context("starting COM")?;
    }
    crate::encoder::startup()?;
    loop {
        // The host creates the pipe and names it in a file only this user can read.
        let pipe = std::fs::read_to_string(data_dir().join("screen.pipe"))
            .ok()
            .map(|name| name.trim().to_owned());
        match pipe.map(|name| ClientOptions::new().open(name)) {
            Some(Ok(pipe)) => {
                tracing::info!("connected to codync-host");
                if let Err(error) = serve(pipe).await {
                    tracing::warn!(error = format!("{error:#}"), "host link ended");
                }
            }
            Some(Err(error)) => tracing::debug!(%error, "codync-host isn't listening yet"),
            None => tracing::debug!("codync-host hasn't published its screen pipe yet"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn serve(pipe: tokio::net::windows::named_pipe::NamedPipeClient) -> Result<()> {
    let (out, outbox) = mpsc::unbounded_channel::<Value>();
    let helper = Arc::new(Helper {
        out: out.clone(),
        sessions: Sessions::default(),
        last_status: Mutex::new(None),
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
    let result = link::serve(pipe, out, outbox, move |method, params| {
        let helper = serving.clone();
        async move { Box::pin(helper.handle(&method, &params)).await }
    })
    .await;
    status_task.abort();
    helper.sessions.close_all().await;
    result
}

impl Helper {
    async fn handle(self: &Arc<Self>, method: &str, p: &Value) -> Result<Value> {
        match method {
            // Windows asks for no grants: capture and input work in the signed-in session.
            "requestPermission" => Ok(json!({})),
            "closeAll" => {
                self.sessions.close_all().await;
                Ok(json!({}))
            }
            "close" => {
                if let Some(id) = p["session"].as_str() {
                    self.sessions.close(id).await;
                }
                Ok(json!({}))
            }
            "answer" => {
                let id = p["session"]
                    .as_str()
                    .ok_or_else(|| anyhow!("session is required"))?
                    .to_owned();
                let monitor = find_monitor(p["display"].as_u64())?;
                let sdp = self
                    .sessions
                    .answer(id.clone(), Box::pin(Session::answer(id, self, monitor, p)))
                    .await?;
                Ok(json!({"sdp": sdp}))
            }
            "candidate" => {
                let id = p["session"].as_str().context("missing session")?;
                let candidate: Candidate = serde_json::from_value(p["candidate"].clone())?;
                self.sessions.candidate(id, &candidate).await?;
                Ok(json!({}))
            }
            other => Err(anyhow!("unknown method {other}")),
        }
    }

    async fn send_status(&self) {
        let monitors = blocking_displays().await;
        let ready = !monitors.is_empty();
        let status = json!({
            "platform": "windows",
            "permissionApp": "Codync Screen",
            "version": env!("CARGO_PKG_VERSION"),
            "trickle": true,
            "capture": ready,
            "input": ready,
            "displays": monitors.iter().map(Monitor::status).collect::<Vec<_>>(),
        });
        let mut last = self.last_status.lock().await;
        if last.as_ref() == Some(&status) {
            return;
        }
        *last = Some(status.clone());
        // Fails only once the host link is gone; the next link starts a new Helper that sends its status first.
        let _ = self.notify("status", status);
    }

    /// One notification to the host; fails once the host link is gone.
    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        let mut message = json!({"jsonrpc": "2.0", "method": method});
        message["params"] = params;
        self.out
            .send(message)
            .map_err(|_| anyhow!("the host link closed"))
    }

    /// A session's WebRTC connection ended on its own.
    pub async fn ended(&self, id: &str) {
        self.sessions.close(id).await;
        let _ = self.notify("session", json!({"session": id, "state": "closed"}));
    }
}

fn find_monitor(id: Option<u64>) -> Result<Monitor> {
    let monitors = displays::list()?;
    match id {
        Some(id) => monitors.into_iter().find(|m| u64::from(m.id) == id),
        None => monitors
            .iter()
            .find(|m| m.main())
            .or(monitors.first())
            .cloned(),
    }
    .ok_or_else(|| anyhow!("no such display"))
}

async fn blocking_displays() -> Vec<Monitor> {
    tokio::task::spawn_blocking(|| displays::list().unwrap_or_default())
        .await
        .unwrap_or_default()
}
