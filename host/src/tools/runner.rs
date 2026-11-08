//! The runner every bundled tool shares: one child process from the app's resources, its own
//! working folder and a narrow environment, a free port, a rotated log, and a state clients show.
//! The process is owned through [`Process`] (a Job Object on Windows, a process group elsewhere),
//! so stopping the runner stops everything the tool started, not just its first PID.

use super::Engine;
use super::health;
use super::log::Log;
use crate::LockExt;
use crate::agent::process::Process;
use anyhow::{Context, Result, anyhow};
use std::ffi::OsString;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::broadcast;

/// Variables a tool inherits from the host; everything else (PATH, tokens, proxies) stays out and
/// anything the tool needs comes from [`Launch::env`]. The user-profile ones are what Python and torch
/// use to find their caches.
const INHERITED_ENV: [&str; 13] = [
    "SystemRoot",
    "windir",
    "TEMP",
    "TMP",
    "TMPDIR",
    "HOME",
    "LANG",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "HOMEDRIVE",
    "HOMEPATH",
    "NUMBER_OF_PROCESSORS",
];
const WATCH_EVERY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Stopped,
    Starting,
    Running,
    Error(String),
}

impl State {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Error(_) => "error",
        }
    }
}

/// How to start one tool: the program inside the app's resources, its arguments and folder.
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

pub struct Runner {
    engine: &'static Engine,
    state: Mutex<State>,
    port: AtomicU16,
    process: tokio::sync::Mutex<Option<Process>>,
    /// Bumped by every start and stop, so a stale start or watcher can't overwrite a newer state.
    generation: AtomicU64,
    log: Arc<Log>,
    events: broadcast::Sender<serde_json::Value>,
}

impl Runner {
    pub fn new(engine: &'static Engine, log: Log, events: broadcast::Sender<serde_json::Value>) -> Self {
        Self {
            engine,
            state: Mutex::new(State::Stopped),
            port: AtomicU16::new(engine.default_port),
            process: tokio::sync::Mutex::default(),
            generation: AtomicU64::new(0),
            log: Arc::new(log),
            events,
        }
    }

    pub fn engine(&self) -> &'static Engine {
        self.engine
    }

    pub fn state(&self) -> State {
        self.state.locked().clone()
    }

    pub fn port(&self) -> u16 {
        self.port.load(Ordering::SeqCst)
    }

    pub fn log(&self) -> &Log {
        &self.log
    }

    pub fn active(&self) -> bool {
        matches!(self.state(), State::Starting | State::Running)
    }

    /// Sets the state and tells the clients, which then ask `toolStatus` for the whole picture.
    fn set_state(&self, state: State) {
        *self.state.locked() = state;
        let _ = self.events.send(serde_json::json!({ "type": "tool", "id": self.engine.id }));
    }

    /// Starts the tool and waits until its health endpoint answers. `build` receives the port the
    /// runner picked. The state ends `Running`, or `Error` with the reason and the log's last lines;
    /// a stop during the wait leaves `Stopped`.
    pub async fn start(self: &Arc<Self>, build: fn(u16) -> Result<Launch>) -> Result<()> {
        if self.active() {
            return Ok(());
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.set_state(State::Starting);
        match self.launch(generation, build).await {
            Ok(true) => {
                self.set_state(State::Running);
                self.watch(generation);
                Ok(())
            }
            Ok(false) => Ok(()), // a stop or a new start took over
            Err(e) => {
                let reason = format!("{e:#}");
                tracing::warn!(tool = self.engine.id, error = reason, "tool failed to start");
                if self.generation.load(Ordering::SeqCst) == generation {
                    self.kill_process().await;
                    let tail = self.log.tail();
                    self.set_state(State::Error(if tail.is_empty() { reason } else { format!("{reason}\n{tail}") }));
                }
                Err(e)
            }
        }
    }

    async fn launch(self: &Arc<Self>, generation: u64, build: fn(u16) -> Result<Launch>) -> Result<bool> {
        let port = free_port(self.engine.default_port)?;
        if port != self.engine.default_port {
            self.log
                .note(&format!("port {} is taken; {} runs on {port}", self.engine.default_port, self.engine.label))
                .await;
        }
        self.port.store(port, Ordering::SeqCst);
        // Building a launch checks disks and GPUs: blocking work.
        let launch = tokio::task::spawn_blocking(move || build(port)).await.context("preparing the launch")??;
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(&launch.cwd)
            .env_clear()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        for key in INHERITED_ENV {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.envs(launch.env.iter().map(|(k, v)| (k, v)));
        Process::prepare(&mut command);
        // After prepare, which sets the creation flags: a console program started by the windowless host
        // would open a console window of its own.
        #[cfg(windows)]
        command.creation_flags(
            windows_sys::Win32::System::Threading::CREATE_SUSPENDED
                | windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
        );
        let mut child = command.spawn().with_context(|| format!("starting {}", launch.program.display()))?;
        self.log.pump(child.stdout.take());
        self.log.pump(child.stderr.take());
        *self.process.lock().await = Some(Process::new(child)?);

        let url = format!("http://127.0.0.1:{port}{}", self.engine.health_path);
        health::wait_ready(
            &url,
            self.engine.ready_timeout,
            || self.generation.load(Ordering::SeqCst) == generation,
            || self.process_alive(),
        )
        .await
        .with_context(|| format!("{} on port {port}", self.engine.label))
    }

    async fn process_alive(&self) -> Result<bool> {
        let guard = self.process.lock().await;
        let process = guard.as_ref().ok_or_else(|| anyhow!("the process is gone"))?;
        Ok(!process.exited()?)
    }

    /// Notices a tool that dies after it was ready, so the card shows `error` and Retry.
    fn watch(self: &Arc<Self>, generation: u64) {
        let runner = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(WATCH_EVERY).await;
                if runner.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                if matches!(runner.process_alive().await, Ok(true)) {
                    continue;
                }
                runner.kill_process().await;
                let tail = runner.log.tail();
                runner.set_state(State::Error(format!("{} stopped unexpectedly\n{tail}", runner.engine.label)));
                return;
            }
        });
    }

    /// Stops the tool and everything it started, waiting for them to go.
    pub async fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.kill_process().await;
        self.set_state(State::Stopped);
    }

    async fn kill_process(&self) {
        if let Some(mut process) = self.process.lock().await.take() {
            process.kill().await;
        }
    }
}

/// `preferred` when it is free on loopback, else any free port.
fn free_port(preferred: u16) -> Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, preferred))
        .or_else(|_| TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))
        .context("finding a free port")?;
    Ok(listener.local_addr()?.port())
}
