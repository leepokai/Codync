//! n8n: the n8n instance this computer runs under the host's control. The desktop app
//! embeds its editor; its data lives next to the host's.
//!
//! The installation is the user's own folder (a built n8n monorepo). The host finds
//! Node.js on the computer, starts `packages/cli/bin/automan` in it, and waits for the
//! instance's `/healthz` endpoint.

use crate::LockExt;
use crate::agent::backends;
use crate::agent::process::Process;
use crate::service;
use crate::store::Store;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::broadcast;

/// The oldest Node.js major that runs an n8n install.
const NODE_MAJOR: u32 = 24;
/// The first start runs database setup and migrations, so the wait is patient.
const READY_TIMEOUT: Duration = Duration::from_secs(180);
const DEFAULT_PORT: u16 = 5678;
const LOG_CAP: usize = 16 * 1024;

const KV_PATH: &str = "n8n.path";
const KV_PORT: &str = "n8n.port";
const KV_ENABLED: &str = "n8n.enabled";
/// The settings' keys under the pre-rename name, read once and moved to the new keys.
const LEGACY_KV: [(&str, &str); 3] =
    [("automan.path", KV_PATH), ("automan.port", KV_PORT), ("automan.enabled", KV_ENABLED)];

#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Stopped,
    Starting,
    Running,
    Error(String),
}

impl State {
    fn name(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Error(_) => "error",
        }
    }
}

struct Settings {
    path: Option<PathBuf>,
    port: u16,
    enabled: bool,
}

/// The n8n service: its settings, the child process, and the state clients display.
pub struct N8n {
    settings: Mutex<Settings>,
    state: Mutex<State>,
    process: tokio::sync::Mutex<Option<Process>>,
    /// The tail of the instance's output, for error messages.
    log: Mutex<Vec<u8>>,
    /// Bumped by every stop and start, so a stale run task can't overwrite the state
    /// a newer stop or start already set.
    generation: AtomicU64,
    events: broadcast::Sender<Value>,
}

impl N8n {
    pub fn new(store: &Store, events: broadcast::Sender<Value>) -> Self {
        let settings = Settings {
            path: kv(store, KV_PATH).map(PathBuf::from),
            port: kv(store, KV_PORT).and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_PORT),
            enabled: kv(store, KV_ENABLED).as_deref() == Some("1"),
        };
        Self {
            settings: Mutex::new(settings),
            state: Mutex::new(State::Stopped),
            process: tokio::sync::Mutex::default(),
            log: Mutex::default(),
            generation: AtomicU64::new(0),
            events,
        }
    }

    /// What clients show: the settings, the state, and where the editor runs.
    pub fn state(&self) -> Value {
        let s = self.settings.locked();
        let st = self.state.locked();
        json!({
            "enabled": s.enabled,
            "state": st.name(),
            "error": match &*st { State::Error(e) => Some(e.clone()), _ => None },
            "url": s.path.is_some().then(|| format!("http://127.0.0.1:{}", s.port)),
            "port": s.port,
            "path": s.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        })
    }

    pub fn enabled(&self) -> bool {
        self.settings.locked().enabled
    }

    fn set_state(&self, state: State) {
        if *self.state.locked() == state {
            return;
        }
        *self.state.locked() = state;
        self.broadcast();
    }

    /// The clients' view of the service: the settings, the state, and where the editor runs.
    fn broadcast(&self) {
        let _ = self.events.send(json!({ "type": "n8n", "n8n": self.state() }));
    }

    fn active(&self) -> bool {
        matches!(*self.state.locked(), State::Starting | State::Running)
    }

    fn persist(&self, store: &Store) -> Result<()> {
        let s = self.settings.locked();
        if let Some(path) = &s.path {
            store.kv_set(KV_PATH, &path.to_string_lossy())?;
        }
        if s.port != DEFAULT_PORT {
            store.kv_set(KV_PORT, &s.port.to_string())?;
        }
        store.kv_set(KV_ENABLED, if s.enabled { "1" } else { "0" })?;
        Ok(())
    }

    /// Kicks off a start when it isn't already under way; the run happens in the background.
    /// Only starts when the service is on.
    pub fn start(self: &Arc<Self>) {
        if !self.enabled() || self.active() {
            return;
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.set_state(State::Starting);
        tokio::spawn(self.clone().run(generation));
    }

    /// Turns the service on (it starts with the host from now on) and, if it isn't running, starts it.
    pub fn enable(self: &Arc<Self>, store: &Store) -> Result<()> {
        self.settings.locked().enabled = true;
        self.persist(store)?;
        self.start();
        Ok(())
    }

    /// Stops the instance (it stays on for the next host start), waiting for it to go.
    pub async fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.stop_process().await;
        self.set_state(State::Stopped);
    }

    /// Stops the instance and turns the service off.
    pub async fn disable(self: &Arc<Self>, store: &Store) -> Result<()> {
        self.settings.locked().enabled = false;
        self.persist(store)?;
        self.stop().await;
        // The state may not have moved (already stopped), but the switch did.
        self.broadcast();
        Ok(())
    }

    pub async fn set_path(&self, store: &Store, path: &str) -> Result<()> {
        let path = PathBuf::from(path.trim());
        check_install(&path)?;
        self.settings.locked().path = Some(path);
        self.persist(store)?;
        self.after_settings_change().await;
        Ok(())
    }

    pub async fn set_port(&self, store: &Store, port: u16) -> Result<()> {
        ensure!(port >= 1024, "that port is reserved; pick one of 1024 or higher");
        self.settings.locked().port = port;
        self.persist(store)?;
        self.after_settings_change().await;
        Ok(())
    }

    /// A setting changed: a running instance comes back with the new value, and the clients
    /// get the new snapshot even when the state name didn't move.
    async fn after_settings_change(&self) {
        if self.active() {
            self.stop().await;
        } else if matches!(*self.state.locked(), State::Error(_)) {
            // The error belongs to the old settings.
            self.set_state(State::Stopped);
        }
        self.broadcast();
    }

    /// Kills the owned process if there is one.
    async fn stop_process(&self) {
        if let Some(mut process) = self.process.lock().await.take() {
            process.kill().await;
        }
    }

    /// The start's whole life: launch, wait for readiness, then watch for a crash.
    async fn run(self: Arc<Self>, generation: u64) {
        match self.launch(generation).await {
            Ok(()) => self.watch_exit(generation),
            Err(e) if self.generation.load(Ordering::SeqCst) == generation => {
                let tail = self.log_tail();
                self.set_state(State::Error(with_tail(e.to_string(), &tail)));
            }
            Err(_) => {}
        }
    }

    async fn launch(self: &Arc<Self>, generation: u64) -> Result<()> {
        let (path, port) = {
            let s = self.settings.locked();
            let path = s.path.clone().context("no n8n folder set yet")?;
            (path, s.port)
        };
        let bin = path.join("packages/cli/bin/automan");
        ensure!(bin.exists(), "that folder isn't an n8n install (missing packages/cli/bin/automan)");
        let node =
            tokio::task::spawn_blocking(|| node_at_least(NODE_MAJOR)).await?.context("Node.js for the n8n instance")?;
        let user_folder = service::data_dir().join("n8n");
        migrate_data_dir(&user_folder).await?;
        tokio::fs::create_dir_all(&user_folder).await.context("preparing the n8n data folder")?;

        let mut command = Command::new(&node);
        command
            .arg(&bin)
            .current_dir(&path)
            .env("AUTOMAN_USER_FOLDER", &user_folder)
            .env("AUTOMAN_PORT", port.to_string())
            .env("AUTOMAN_HOST", "127.0.0.1")
            .env("AUTOMAN_PROTOCOL", "http")
            .env("AUTOMAN_DIAGNOSTICS_ENABLED", "false")
            .env("NODE_ENV", "production")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        Process::prepare(&mut command);
        let mut child = command.spawn().with_context(|| format!("starting n8n on port {port}"))?;
        self.pump(child.stdout.take());
        self.pump(child.stderr.take());
        let process = Process::new(child)?;
        *self.process.lock().await = Some(process);

        // Ready when /healthz answers, watching for the instance dying before that.
        let url = format!("http://127.0.0.1:{port}/healthz");
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        loop {
            if self.generation.load(Ordering::SeqCst) != generation {
                return Ok(()); // a stop or new start took over
            }
            if self.exited().await? {
                bail!("n8n stopped before it was ready{}", tail_hint(&self.log_tail()));
            }
            if health(&url).await {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                bail!("n8n didn't come up within three minutes{}", tail_hint(&self.log_tail()));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        self.set_state(State::Running);
        Ok(())
    }

    /// A watchdog: the instance died on its own, so the state says what it last printed.
    fn watch_exit(self: Arc<Self>, generation: u64) {
        tokio::spawn(async move {
            loop {
                match self.exited().await {
                    Ok(false) => {}
                    Ok(true) | Err(_) => break,
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            // A stop already set the state; only a crash needs a message.
            if self.generation.load(Ordering::SeqCst) == generation && *self.state.locked() == State::Running {
                self.set_state(State::Error(format!("n8n stopped unexpectedly{}", tail_hint(&self.log_tail()))));
            }
        });
    }

    /// Whether the owned process has exited (and whether there is one).
    async fn exited(&self) -> Result<bool> {
        let guard = self.process.lock().await;
        match &*guard {
            Some(p) => Ok(p.exited()?),
            None => Ok(true),
        }
    }

    /// Keeps the tail of the instance's output for error messages.
    fn pump(self: &Arc<Self>, stream: Option<impl tokio::io::AsyncRead + Unpin + Send + 'static>) {
        let Some(mut stream) = stream else { return };
        let this = self.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut log = this.log.locked();
                        log.extend_from_slice(&buf[..n]);
                        let overflow = log.len().saturating_sub(LOG_CAP);
                        if overflow > 0 {
                            log.drain(..overflow);
                        }
                    }
                }
            }
        });
    }

    /// The end of the instance's output, for error messages.
    fn log_tail(&self) -> String {
        let log = self.log.locked();
        let text = String::from_utf8_lossy(&log);
        let mut lines: Vec<&str> = text.lines().rev().take(3).collect();
        lines.reverse();
        lines.join(" | ")
    }
}

/// A renamed settings key: the new value if stored, else the pre-rename one, which is
/// moved to the new key so this only happens once.
fn kv(store: &Store, key: &str) -> Option<String> {
    if let Some(v) = store.kv_get(key) {
        return Some(v);
    }
    let legacy = LEGACY_KV.iter().find(|(_, new)| *new == key)?.0;
    let value = store.kv_get(legacy)?;
    if store.kv_set(key, &value).is_ok() {
        let _ = store.kv_delete(legacy);
    }
    Some(value)
}

/// The instance's data folder under its pre-rename name is moved to the new one, once.
async fn migrate_data_dir(dir: &Path) -> Result<()> {
    let legacy = dir.with_file_name("automan");
    if legacy.is_dir() && !dir.exists() {
        tokio::fs::rename(&legacy, dir)
            .await
            .with_context(|| format!("moving {} to {}", legacy.display(), dir.display()))?;
    }
    Ok(())
}

/// `message — what the instance last printed`, when there is a print to cite.
fn with_tail(message: String, tail: &str) -> String {
    if tail.is_empty() { message } else { format!("{message} — {tail}") }
}

/// `: what the instance last printed`, for messages that end on the tail.
fn tail_hint(tail: &str) -> String {
    if tail.is_empty() { ".".into() } else { format!(": {tail}") }
}

/// Node.js on the computer, at least `min_major`.
fn node_at_least(min_major: u32) -> Result<PathBuf> {
    let node = backends::which("node").context("Node.js isn't on this computer's PATH")?;
    let out = std::process::Command::new(&node).arg("--version").output().context("checking Node.js's version")?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let major: u32 = version.trim_start_matches('v').split('.').next().unwrap_or_default().parse().unwrap_or(0);
    ensure!(major >= min_major, "n8n needs Node.js {min_major} or newer; this computer has {version}");
    Ok(node)
}

/// Whether the install folder has the entry point the host starts.
fn check_install(path: &Path) -> Result<()> {
    ensure!(
        path.join("packages/cli/bin/automan").exists(),
        "that folder isn't an n8n install (missing packages/cli/bin/automan)"
    );
    Ok(())
}

/// One readiness probe; connection errors mean "not up yet".
async fn health(url: &str) -> bool {
    matches!(crate::http().get(url).timeout(Duration::from_secs(2)).send().await.ok(), Some(r) if r.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn temp_store() -> Store {
        let dir = std::env::temp_dir().join(format!("n8n-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open(&dir.join("codync.db")).unwrap()
    }

    #[test]
    fn settings_default_to_off_and_the_usual_port() {
        let store = temp_store();
        let (events, _) = broadcast::channel(8);
        let a = N8n::new(&store, events);
        let s = a.state();
        assert_eq!(s["enabled"], false);
        assert_eq!(s["state"], "stopped");
        assert_eq!(s["port"], DEFAULT_PORT);
        assert!(s["url"].is_null());
    }

    #[test]
    fn the_path_must_be_an_install() {
        let store = temp_store();
        let (events, _) = broadcast::channel(8);
        let a = Arc::new(N8n::new(&store, events));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let bad = std::env::temp_dir().join(format!("n8n-not-{}", uuid::Uuid::new_v4()));
        rt.block_on(a.set_path(&store, &bad.to_string_lossy())).unwrap_err();
        std::fs::create_dir_all(bad.join("packages/cli/bin")).unwrap();
        std::fs::write(bad.join("packages/cli/bin/automan"), "").unwrap();
        rt.block_on(a.set_path(&store, &bad.to_string_lossy())).unwrap();
        assert_eq!(a.state()["path"], bad.to_string_lossy().to_string());
        assert_eq!(a.state()["url"], format!("http://127.0.0.1:{DEFAULT_PORT}"));
    }

    #[test]
    fn the_port_stays_in_the_user_range() {
        let store = temp_store();
        let (events, _) = broadcast::channel(8);
        let a = Arc::new(N8n::new(&store, events));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(a.set_port(&store, 80)).unwrap_err();
        rt.block_on(a.set_port(&store, 5679)).unwrap();
        assert_eq!(a.state()["port"], 5679);
    }

    #[test]
    fn a_settings_change_reaches_the_clients() {
        let store = temp_store();
        let (events, mut rx) = broadcast::channel(8);
        let a = Arc::new(N8n::new(&store, events));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let good = std::env::temp_dir().join(format!("n8n-good-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(good.join("packages/cli/bin")).unwrap();
        std::fs::write(good.join("packages/cli/bin/automan"), "").unwrap();
        rt.block_on(async {
            a.set_path(&store, &good.to_string_lossy()).await.unwrap();
            let ev = rx.recv().await.unwrap();
            assert_eq!(ev["type"], "n8n");
            assert_eq!(ev["n8n"]["path"], good.to_string_lossy().to_string());
            assert_eq!(ev["n8n"]["url"], format!("http://127.0.0.1:{DEFAULT_PORT}"));
            a.set_port(&store, 5679).await.unwrap();
            let ev = rx.recv().await.unwrap();
            assert_eq!(ev["n8n"]["port"], 5679);
            assert_eq!(ev["n8n"]["url"], "http://127.0.0.1:5679");
        });
    }

    #[test]
    fn enabling_persists_and_reflects() {
        let store = temp_store();
        let (events, _) = broadcast::channel(8);
        let a = Arc::new(N8n::new(&store, events));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            a.enable(&store).unwrap();
            assert_eq!(store.kv_get(KV_ENABLED).as_deref(), Some("1"));
            a.disable(&store).await.unwrap();
            assert_eq!(store.kv_get(KV_ENABLED).as_deref(), Some("0"));
        });
    }

    #[test]
    fn settings_under_the_old_name_come_along_and_are_moved() {
        let store = temp_store();
        store.kv_set("automan.path", "C:\\n8n-install").unwrap();
        store.kv_set("automan.port", "5679").unwrap();
        store.kv_set("automan.enabled", "1").unwrap();
        let (events, _) = broadcast::channel(8);
        let a = N8n::new(&store, events);
        let s = a.state();
        assert_eq!(s["path"], "C:\\n8n-install");
        assert_eq!(s["port"], 5679);
        assert_eq!(s["enabled"], true);
        // The move is one-time: the new keys hold the values, the old ones are gone.
        assert_eq!(store.kv_get(KV_PATH).as_deref(), Some("C:\\n8n-install"));
        assert_eq!(store.kv_get(KV_PORT).as_deref(), Some("5679"));
        assert_eq!(store.kv_get(KV_ENABLED).as_deref(), Some("1"));
        assert!(store.kv_get("automan.path").is_none());
        assert!(store.kv_get("automan.port").is_none());
        assert!(store.kv_get("automan.enabled").is_none());
    }
}
