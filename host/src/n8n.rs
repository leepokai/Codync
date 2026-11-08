//! n8n: the n8n instance this computer runs under the host's control. The desktop app
//! embeds its editor; its data lives next to the host's.
//!
//! The installation is the user's own folder (a built n8n monorepo). The host finds
//! Node.js on the computer, starts `packages/cli/bin/automan` in it, and waits for the
//! instance's `/healthz` endpoint.

use crate::LockExt;
use crate::agent::backends;
use crate::agent::process::Process;
use crate::hub::Hub;
use crate::service;
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;
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
/// The instance's owner and MCP API key, which the host creates on a first start and keeps.
const KV_OWNER_EMAIL: &str = "n8n.owner.email";
const KV_OWNER_PASSWORD: &str = "n8n.owner.password";
const KV_MCP_KEY: &str = "n8n.mcpApiKey";
/// The owner the host creates for an instance that doesn't have one yet.
const OWNER_EMAIL: &str = "owner@codync.local";
/// The instance's session cookie; the desktop's editor logs in with it.
const AUTH_COOKIE: &str = "automan-auth";
/// The instance's MCP server in the bots' MCP list.
const CONNECTOR_ID: &str = "n8n";

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
    /// What the instance's Codync node reaches back to: this host's loopback URL.
    host_url: String,
    /// This host's loopback bearer token, for the same node.
    host_token: String,
    /// The instance's session cookie (`automan-auth=<jwt>`): the desktop's editor's auto-login.
    session: Mutex<Option<String>>,
    /// Whether the instance's MCP server is registered as a connector for the bots.
    mcp: Mutex<bool>,
}

impl N8n {
    pub fn new(store: &Store, events: broadcast::Sender<Value>, host_url: String, host_token: String) -> Self {
        let settings = Settings {
            path: initial_path(store),
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
            host_url,
            host_token,
            session: Mutex::default(),
            mcp: Mutex::default(),
        }
    }

    /// What clients show: the settings, the state, where the editor runs, and its auto-login.
    pub fn state(&self) -> Value {
        let s = self.settings.locked();
        let st = self.state.locked();
        let auth = self.session.locked().clone();
        let mcp = *self.mcp.locked();
        json!({
            "enabled": s.enabled,
            "state": st.name(),
            "error": match &*st { State::Error(e) => Some(e.clone()), _ => None },
            "url": s.path.is_some().then(|| format!("http://127.0.0.1:{}", s.port)),
            "port": s.port,
            "path": s.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "mcp": mcp,
            "auth": auth,
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
    pub fn start(self: &Arc<Self>, hub: &Arc<Hub>) {
        if !self.enabled() || self.active() {
            return;
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.set_state(State::Starting);
        tokio::spawn(self.clone().run(generation, hub.clone()));
    }

    /// Turns the service on (it starts with the host from now on) and, if it isn't running, starts it.
    pub fn enable(self: &Arc<Self>, hub: &Arc<Hub>) -> Result<()> {
        self.settings.locked().enabled = true;
        self.persist(&hub.store)?;
        self.start(hub);
        Ok(())
    }

    /// Stops the instance (it stays on for the next host start), waiting for it to go.
    pub async fn stop(&self, store: &Store) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.stop_process().await;
        // A stopped instance has no MCP server: the bots lose the connector with it.
        if let Err(e) = crate::market::remove_connector(store, CONNECTOR_ID) {
            tracing::warn!(error = format!("{e:#}"), "removing the n8n connector");
        }
        *self.session.locked() = None;
        *self.mcp.locked() = false;
        self.set_state(State::Stopped);
    }

    /// Stops the instance and turns the service off.
    pub async fn disable(self: &Arc<Self>, store: &Store) -> Result<()> {
        self.settings.locked().enabled = false;
        self.persist(store)?;
        self.stop(store).await;
        // The state may not have moved (already stopped), but the switch did.
        self.broadcast();
        Ok(())
    }

    pub async fn set_path(&self, store: &Store, path: &str) -> Result<()> {
        let path = PathBuf::from(path.trim());
        check_install(&path)?;
        self.settings.locked().path = Some(path);
        self.persist(store)?;
        self.after_settings_change(store).await;
        Ok(())
    }

    pub async fn set_port(&self, store: &Store, port: u16) -> Result<()> {
        ensure!(port >= 1024, "that port is reserved; pick one of 1024 or higher");
        self.settings.locked().port = port;
        self.persist(store)?;
        self.after_settings_change(store).await;
        Ok(())
    }

    /// A setting changed: a running instance comes back with the new value, and the clients
    /// get the new snapshot even when the state name didn't move.
    async fn after_settings_change(&self, store: &Store) {
        if self.active() {
            self.stop(store).await;
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

    /// The start's whole life: launch, wait for readiness, wire the MCP connector, then watch
    /// for a crash.
    async fn run(self: Arc<Self>, generation: u64, hub: Arc<Hub>) {
        match self.launch(generation, &hub).await {
            Ok(()) => self.watch_exit(generation),
            Err(e) if self.generation.load(Ordering::SeqCst) == generation => {
                let tail = self.log_tail();
                self.set_state(State::Error(with_tail(e.to_string(), &tail)));
            }
            Err(_) => {}
        }
    }

    async fn launch(self: &Arc<Self>, generation: u64, hub: &Arc<Hub>) -> Result<()> {
        let (path, port) = {
            let s = self.settings.locked();
            let path = s.path.clone().context(
                "n8n doesn't know its install folder yet. Choose the folder that holds packages/cli/bin/automan in Settings â†’ n8n.",
            )?;
            (path, s.port)
        };
        let bin = path.join("packages/cli/bin/automan");
        ensure!(bin.exists(), "that folder isn't an n8n install (missing packages/cli/bin/automan)");
        let node =
            tokio::task::spawn_blocking(|| node_at_least(NODE_MAJOR)).await?.context("Node.js for the n8n instance")?;
        let user_folder = service::data_dir().join("n8n");
        migrate_data_dir(&user_folder).await?;
        tokio::fs::create_dir_all(&user_folder).await.context("preparing the n8n data folder")?;
        install_node_package(&user_folder).await?;

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
            // The Codync node in this instance's editor: the host it talks to, with
            // its loopback token, so workflows need no configured credential.
            .env("CODYNC_HOST_URL", &self.host_url)
            .env("CODYNC_HOST_TOKEN", &self.host_token)
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
        // The instance is up: the bots get its MCP server, the editor its auto-login.
        self.mcp_setup(generation, hub).await;
        Ok(())
    }

    /// The instance is ready: its owner, session, MCP access and the bots' connector.
    /// Best effort: a failure shows in the state and the logs, never in the instance.
    async fn mcp_setup(&self, generation: u64, hub: &Arc<Hub>) {
        if !self.rest_ready(generation).await {
            tracing::warn!("the n8n instance's REST API was not ready yet; its MCP server waits for the next start");
            return;
        }
        let store = &hub.store;
        let (cookie, key) = match self.mcp_setup_inner(store).await {
            Ok(session) => session,
            Err(e) => {
                tracing::warn!(error = format!("{e:#}"), "setting up the n8n instance's MCP server");
                return;
            }
        };
        // A stop in the meantime: the session and the connector go away with the instance.
        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        // The editor's auto-login only needs the session: it is published even when the
        // connector below cannot be written, as its vault may be locked.
        *self.session.locked() = Some(cookie);
        // The connector's Authorization header is a secret: unlock the vault the first time
        // the connector is written, not only when someone opens the credentials page.
        if let Err(e) = crate::market::vault::unlock(hub.clone()).await {
            tracing::warn!(error = format!("{e:#}"), "unlocking credential storage for the n8n connector");
        }
        *self.mcp.locked() = match self.connect_bots(store, hub, &key) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = format!("{e:#}"), "registering the n8n connector for the bots");
                false
            }
        };
        self.broadcast();
    }

    /// The REST API mounts a while after `/healthz` answers (it runs migrations first);
    /// wait until one of its routes answers anything but a 404: the unauthenticated
    /// `GET /rest/login` answers 401 once the router is up.
    async fn rest_ready(&self, generation: u64) -> bool {
        let port = self.settings.locked().port;
        let url = format!("http://127.0.0.1:{port}/rest/login");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            if self.generation.load(Ordering::SeqCst) != generation {
                return false; // a stop or new start took over
            }
            let mounted = matches!(
                crate::http().get(&url).timeout(Duration::from_secs(2)).send().await,
                Ok(r) if r.status().as_u16() != 404
            );
            if mounted {
                return true;
            }
            if tokio::time::Instant::now() > deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Owner, login, MCP access and the API key; returns the editor's session cookie and
    /// the key the bots' connector authenticates with.
    async fn mcp_setup_inner(&self, store: &Store) -> Result<(String, String)> {
        let port = self.settings.locked().port;
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::builder().cookie_store(true).build().context("building the n8n client")?;

        // The instance's owner: this host's, created on the first start and kept from then on.
        let (email, password) =
            if let (Some(email), Some(password)) = (store.kv_get(KV_OWNER_EMAIL), store.kv_get(KV_OWNER_PASSWORD)) {
                (email, password)
            } else {
                let email = OWNER_EMAIL.to_owned();
                let password = owner_password();
                Self::owner_setup(&client, &base, &email, &password).await?;
                store.kv_set(KV_OWNER_EMAIL, &email)?;
                store.kv_set(KV_OWNER_PASSWORD, &password)?;
                (email, password)
            };

        // The session cookie the desktop's editor logs in with.
        let mut resp = client
            .post(format!("{base}/rest/login"))
            .json(&json!({ "emailOrLdapLoginId": email, "password": password }))
            .send()
            .await
            .context("logging into the n8n instance")?;
        // The saved credentials can belong to a run that raced the REST router: the
        // instance still says "set up the owner". Create it now and log in again.
        if resp.status().as_u16() == 401 && Self::owner_setup(&client, &base, &email, &password).await.is_ok() {
            resp = client
                .post(format!("{base}/rest/login"))
                .json(&json!({ "emailOrLdapLoginId": email, "password": password }))
                .send()
                .await
                .context("logging into the n8n instance")?;
        }
        let status = resp.status();
        let cookie = cookie_value(resp.headers(), AUTH_COOKIE);
        resp.error_for_status().with_context(|| format!("logging into the n8n instance ({status})"))?;
        let cookie = cookie.ok_or_else(|| anyhow!("the n8n login set no {AUTH_COOKIE} cookie"))?;

        // MCP access is off by default.
        let resp = client
            .patch(format!("{base}/rest/mcp/settings"))
            .json(&json!({ "mcpAccessEnabled": true }))
            .send()
            .await
            .context("turning the n8n MCP server on")?;
        let status = resp.status();
        resp.error_for_status().with_context(|| format!("turning the n8n MCP server on ({status})"))?;

        // The API key: raw only at creation, or after a rotation.
        let key = if let Some(key) = store.kv_get(KV_MCP_KEY) {
            key
        } else {
            let body = get_json(&client, &format!("{base}/rest/mcp/api-key"), "reading the n8n MCP API key").await?;
            let mut key =
                api_key_field(&body).map(str::to_owned).ok_or_else(|| anyhow!("no MCP API key in the n8n reply"))?;
            // A redacted reply (`******â€¦`) means a key exists and we don't have it: rotate.
            if key.starts_with('*') {
                let body =
                    post_json(&client, &format!("{base}/rest/mcp/api-key/rotate"), "rotating the n8n MCP API key")
                        .await?;
                key = api_key_field(&body)
                    .filter(|k| !k.starts_with('*'))
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow!("no raw MCP API key in the n8n rotation reply"))?;
            }
            store.kv_set(KV_MCP_KEY, &key)?;
            key
        };
        Ok((cookie, key))
    }

    /// Creates the instance's owner. A 400 means an owner already exists with other
    /// credentials; the caller reports that as the login failure it is.
    async fn owner_setup(client: &reqwest::Client, base: &str, email: &str, password: &str) -> Result<()> {
        let resp = client
            .post(format!("{base}/rest/owner/setup"))
            .json(&json!({
                "email": email,
                "firstName": "Codync",
                "lastName": "Host",
                "password": password,
            }))
            .send()
            .await
            .context("creating the n8n instance's owner")?;
        let status = resp.status();
        ensure!(
            status.as_u16() != 400,
            "the n8n instance already has an owner with another sign-in, so its MCP server stays out of the bots"
        );
        resp.error_for_status().with_context(|| format!("creating the n8n instance's owner ({status})"))?;
        Ok(())
    }

    /// The bots' connector: one for this instance, upserted so port and key changes follow.
    fn connect_bots(&self, store: &Store, hub: &Arc<Hub>, key: &str) -> Result<()> {
        let port = self.settings.locked().port;
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_owned(), format!("Bearer {key}"));
        let connector = crate::market::Connector {
            id: CONNECTOR_ID.to_owned(),
            name: CONNECTOR_ID.to_owned(),
            description: "The n8n instance on this computer".to_owned(),
            registry_name: None,
            catalog: None,
            command: None,
            args: vec![],
            login: vec![],
            env: BTreeMap::new(),
            url: Some(format!("http://127.0.0.1:{port}/mcp-server/http")),
            headers,
            oauth: None,
        };
        crate::market::update_connectors(store, |all| {
            match all.iter_mut().find(|c| c.id == CONNECTOR_ID) {
                Some(existing) => *existing = connector.clone(),
                None => all.push(connector.clone()),
            }
            Ok(())
        })?;
        crate::market::turn_on(hub, crate::market::Installed::Connectors, &[CONNECTOR_ID.to_owned()], None)?;
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

/// The Codync node's package: what the host writes into the instance's custom folder
/// (always scanned) so the editor offers the Codync Bot node with no install step.
const NODE_PACKAGE: [(&str, &str); 3] = [
    ("package.json", include_str!("../n8n-nodes-codync/package.json")),
    ("dist/CodyncBot.node.js", include_str!("../n8n-nodes-codync/dist/CodyncBot.node.js")),
    ("dist/CodyncApi.credentials.js", include_str!("../n8n-nodes-codync/dist/CodyncApi.credentials.js")),
];

/// Writes the package over itself, so it tracks this host's version on every start.
async fn install_node_package(user_folder: &Path) -> Result<()> {
    for (rel, contents) in NODE_PACKAGE {
        let file = user_folder.join("custom/node_modules/n8n-nodes-codync").join(rel);
        if let Some(parent) = file.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| format!("preparing {}", parent.display()))?;
        }
        tokio::fs::write(&file, contents).await.with_context(|| format!("writing the Codync node ({rel})"))?;
    }
    Ok(())
}

/// `message - what the instance last printed`, when there is a print to cite.
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

/// The install folder saved for this computer, or one found in the usual place on a
/// first run. A folder the person chose later always wins.
fn initial_path(store: &Store) -> Option<PathBuf> {
    let saved = kv(store, KV_PATH).map(PathBuf::from);
    #[cfg(not(test))]
    {
        if saved.is_none()
            && let Some(found) = discover_install()
        {
            let _ = store.kv_set(KV_PATH, &found.to_string_lossy());
            tracing::info!(path = %found.display(), "found the n8n install");
            return Some(found);
        }
    }
    saved
}

/// The places an n8n monorepo is commonly cloned or unpacked, newest layouts first.
#[cfg(not(test))]
fn discover_install() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    ["Documents/automan", "automan", "Documents/n8n", "n8n", "Documents/n8n-monorepo", "n8n-monorepo"]
        .into_iter()
        .map(|name| home.join(name))
        .find(|path| check_install(path).is_ok())
}

/// One readiness probe; connection errors mean "not up yet".
async fn health(url: &str) -> bool {
    matches!(crate::http().get(url).timeout(Duration::from_secs(2)).send().await.ok(), Some(r) if r.status().is_success())
}

/// A GET whose JSON reply is the answer; `what` is the error's context.
async fn get_json(client: &reqwest::Client, url: &str, what: &str) -> Result<Value> {
    let resp = client.get(url).send().await.with_context(|| what.to_string())?;
    resp.error_for_status()?.json().await.with_context(|| what.to_string())
}

/// A POST with no body, for the same shape of reply.
async fn post_json(client: &reqwest::Client, url: &str, what: &str) -> Result<Value> {
    let resp = client.post(url).send().await.with_context(|| what.to_string())?;
    resp.error_for_status()?.json().await.with_context(|| what.to_string())
}

/// The `apiKey` of an n8n MCP reply: the instance's REST layer nests the key's
/// entity under `data`, with the bare top-level field as the fallback.
fn api_key_field(body: &Value) -> Option<&str> {
    let nested = body.get("data").and_then(|data| data.get("apiKey"));
    nested.or_else(|| body.get("apiKey"))?.as_str().filter(|k| !k.is_empty())
}

/// The instance owner's password: random, with the required digit and upper.
fn owner_password() -> String {
    let raw = uuid::Uuid::new_v4().simple().to_string();
    let mut out = String::with_capacity(24);
    out.push('C');
    out.push('0');
    out.extend(raw.bytes().take(22).map(char::from));
    out
}

/// The value of `name` among a reply's `Set-Cookie` headers.
fn cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    headers
        .get_all(axum::http::header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find_map(|h| h.split(';').next()?.trim().strip_prefix(&prefix).map(str::to_owned))
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
        let a = N8n::new(&store, events, "http://127.0.0.1:19306".into(), "test".into());
        let s = a.state();
        assert_eq!(s["enabled"], false);
        assert_eq!(s["state"], "stopped");
        assert_eq!(s["port"], DEFAULT_PORT);
        assert!(s["url"].is_null());
    }

    #[test]
    fn the_api_key_reads_the_wrapped_and_bare_reply_shapes() {
        // The instance's REST layer nests the key's entity under `data`.
        let wrapped = json!({
            "data": {
                "id": "FfkIiFUdqNaIryui",
                "label": "MCP Server API Key",
                "apiKey": "mcp_raw_key_1234",
                "audience": "mcp-server-api"
            }
        });
        assert_eq!(api_key_field(&wrapped), Some("mcp_raw_key_1234"));
        assert_eq!(api_key_field(&json!({ "apiKey": "mcp_raw_key_5678" })), Some("mcp_raw_key_5678"));
        // A redacted reply is still the key's field; the caller decides what to do with it.
        assert_eq!(api_key_field(&json!({ "data": { "apiKey": "******T73M" } })), Some("******T73M"));
        assert_eq!(api_key_field(&json!({ "data": {} })), None);
        assert_eq!(api_key_field(&json!({ "data": { "apiKey": "" } })), None);
    }

    #[test]
    fn the_path_must_be_an_install() {
        let store = temp_store();
        let (events, _) = broadcast::channel(8);
        let a = Arc::new(N8n::new(&store, events, "http://127.0.0.1:19306".into(), "test".into()));
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
        let a = Arc::new(N8n::new(&store, events, "http://127.0.0.1:19306".into(), "test".into()));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(a.set_port(&store, 80)).unwrap_err();
        rt.block_on(a.set_port(&store, 5679)).unwrap();
        assert_eq!(a.state()["port"], 5679);
    }

    #[test]
    fn a_settings_change_reaches_the_clients() {
        let store = temp_store();
        let (events, mut rx) = broadcast::channel(8);
        let a = Arc::new(N8n::new(&store, events, "http://127.0.0.1:19306".into(), "test".into()));
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
        let dir = std::env::temp_dir().join(format!("n8n-enable-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("codync.db")).unwrap();
        // Keep connector defaults independent of the person's credential manager.
        *store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![0; 32]));
        let (events, _) = broadcast::channel(8);
        let a = Arc::new(N8n::new(&store, events, "http://127.0.0.1:19306".into(), "test".into()));
        let hub = Hub::new(
            store,
            "n8n-enable".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "test".into(),
            0,
        );
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            a.enable(&hub).unwrap();
            assert_eq!(hub.store.kv_get(KV_ENABLED).as_deref(), Some("1"));
            a.disable(&hub.store).await.unwrap();
            assert_eq!(hub.store.kv_get(KV_ENABLED).as_deref(), Some("0"));
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_under_the_old_name_come_along_and_are_moved() {
        let store = temp_store();
        store.kv_set("automan.path", "C:\\n8n-install").unwrap();
        store.kv_set("automan.port", "5679").unwrap();
        store.kv_set("automan.enabled", "1").unwrap();
        let (events, _) = broadcast::channel(8);
        let a = N8n::new(&store, events, "http://127.0.0.1:19306".into(), "test".into());
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

    #[tokio::test]
    async fn the_codync_node_package_is_written_where_the_instance_scans_it() {
        let dir = std::env::temp_dir().join(format!("n8n-pkg-{}", uuid::Uuid::new_v4()));
        install_node_package(&dir).await.unwrap();
        let pkg = dir.join("custom/node_modules/n8n-nodes-codync");
        assert_eq!(
            std::fs::read_to_string(pkg.join("package.json")).unwrap(),
            include_str!("../n8n-nodes-codync/package.json")
        );
        let node = std::fs::read_to_string(pkg.join("dist/CodyncBot.node.js")).unwrap();
        assert!(node.contains("module.exports = { CodyncBot }"), "the loader instantiates by class name");
        let cred = std::fs::read_to_string(pkg.join("dist/CodyncApi.credentials.js")).unwrap();
        assert!(cred.contains("name = 'codyncApi'"));
        // A start over an existing folder keeps the package at this host's version.
        install_node_package(&dir).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_session_cookie_comes_out_of_the_set_cookie_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.append(axum::http::header::SET_COOKIE, axum::http::HeaderValue::from_static("other=xyz; Path=/"));
        headers.append(
            axum::http::header::SET_COOKIE,
            axum::http::HeaderValue::from_static("automan-auth=abc123; Path=/; HttpOnly"),
        );
        assert_eq!(cookie_value(&headers, AUTH_COOKIE).as_deref(), Some("abc123"));
        assert_eq!(cookie_value(&headers, "missing").as_deref(), None);
    }

    #[test]
    fn the_owner_password_meets_the_instances_rules() {
        for _ in 0..50 {
            let p = owner_password();
            assert!((8..=64).contains(&p.len()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()), "needs an upper: {p}");
            assert!(p.chars().any(|c| c.is_ascii_digit()), "needs a digit: {p}");
        }
    }
}
