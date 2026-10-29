//! Standalone host updates. App bundles and package managers retain ownership
//! of their binaries; a host the desktop app bundles hands an update request to
//! the app. A signed manifest is required before executing new code.
mod install;
mod release;
#[cfg(test)]
mod tests;

use crate::{LockExt as _, hub::Hub, service};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

#[derive(Deserialize, Serialize)]
struct Config {
    /// On unless the owner turned it off; only a host that can update itself does.
    #[serde(default = "on")]
    automatic: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { automatic: on() }
    }
}

const fn on() -> bool {
    true
}

/// A release build of a standalone binary that the installed service runs: the hosts that
/// update themselves (app bundles and Homebrew own their binaries).
fn require_self_updating(executable: &Path) -> Result<()> {
    crate::environment::Environment::current().require_release_updates()?;
    install::require_standalone(executable)?;
    ensure!(
        service::installed(),
        "background updates require an installed host service; run `codync-host update` in a terminal"
    );
    service::require_executable(executable)
}

/// When the desktop app that bundles this host last polled `/health` (ms; 0: never).
static APP_POLLED: AtomicI64 = AtomicI64::new(0);

/// The app polls every 5 s: quiet for longer, it isn't open to carry out a request.
const APP_POLL_TTL_MS: i64 = 15_000;

/// When an update was last asked of that app (ms; 0: never). Its main process sees it in
/// `/health` and runs its own updater.
static APP_REQUESTED: AtomicI64 = AtomicI64::new(0);

/// How long a request waits for the app: one it never saw doesn't update the app days later.
const APP_REQUEST_TTL_MS: i64 = 10 * 60 * 1000;

/// `at` (ms; 0: never) when it was less than `ttl` ms before `now`.
fn recent(at: i64, now: i64, ttl: i64) -> Option<i64> {
    (at > 0 && now - at < ttl).then_some(at)
}

/// The desktop app polled `/health`: it's open to carry out an update request.
pub fn note_app_poll() {
    APP_POLLED.store(crate::store::now_ms(), Ordering::Release);
}

/// Who carries out `installHostUpdate` on this computer.
enum Route {
    /// The desktop app that bundles this host, through `/health`.
    App,
    /// The independent updater (`spawn_worker`).
    Worker,
}

/// How `installHostUpdate` updates this computer, or why it can't: both what `hello.canUpdate`
/// promises and what the call does. Homebrew and development builds update through their owners.
fn route(executable: &Path) -> Result<Route> {
    if install::method(executable) != install::Method::AppBundle {
        require_self_updating(executable)?;
        release::platform()?;
        return Ok(Route::Worker);
    }
    crate::environment::Environment::current().require_release_updates()?;
    ensure!(
        recent(APP_POLLED.load(Ordering::Acquire), crate::store::now_ms(), APP_POLL_TTL_MS).is_some(),
        "Codync isn't open on this computer. Open it, then try again."
    );
    Ok(Route::App)
}

/// The latest release a lookup found, whatever installed this host (`latest_version`).
static LATEST: Mutex<Option<String>> = Mutex::new(None);

fn note_latest(version: &str) {
    *LATEST.locked() = Some(version.to_owned());
}

/// The latest release for this platform (`hello.latestVersion`), so a phone can say whether this computer is
/// current and offer the update. None until a lookup succeeds and on development builds; also on a host
/// Homebrew owns, which `brew` updates.
pub fn latest_version() -> Option<String> {
    let homebrew = service::current_exe().is_ok_and(|exe| install::method(&exe) == install::Method::Homebrew);
    LATEST.locked().clone().filter(|_| !homebrew)
}

/// Looks up the latest release at start and every 6 hours, for `latest_version`; a failed lookup (offline
/// at login, GitHub unreachable) is tried again sooner, from a minute up to every 30 minutes. Release builds
/// on platforms with releases only: a development host never offers one.
pub async fn release_watch() {
    if crate::environment::Environment::current() != crate::environment::Environment::Main
        || release::platform().is_err()
    {
        return;
    }
    let mut retry = Duration::from_mins(1);
    loop {
        let wait = match release::latest().await {
            Ok(release) => {
                note_latest(&release.version);
                retry = Duration::from_mins(1);
                Duration::from_hours(6)
            }
            Err(error) => {
                tracing::debug!(error = format!("{error:#}"), "latest release lookup failed");
                let wait = retry;
                retry = (retry * 5).min(Duration::from_mins(30));
                wait
            }
        };
        tokio::time::sleep(wait).await;
    }
}

/// Whether `installHostUpdate` can update this computer (`hello.canUpdate`).
pub fn installable() -> bool {
    service::current_exe().is_ok_and(|executable| route(&executable).is_ok())
}

/// Installs the latest release now (`installHostUpdate`).
pub fn request_install(port: u16, force: bool) -> Result<()> {
    match route(&service::current_exe()?)? {
        Route::App => {
            APP_REQUESTED.store(crate::store::now_ms(), Ordering::Release);
            Ok(())
        }
        Route::Worker => spawn_worker(port, force),
    }
}

/// The update asked of the app that bundles this host and not yet expired (when, in ms).
pub fn app_requested() -> Option<i64> {
    recent(APP_REQUESTED.load(Ordering::Acquire), crate::store::now_ms(), APP_REQUEST_TTL_MS)
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct State {
    phase: String,
    available_version: Option<String>,
    checked_at: Option<i64>,
    error: Option<String>,
}

fn read<T: serde::de::DeserializeOwned + Default>(name: &str) -> Result<T> {
    match std::fs::read(service::data_dir().join(name)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error.into()),
    }
}

fn write(name: &str, value: &impl Serialize) -> Result<()> {
    let path = service::data_dir().join(name);
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(service::data_dir())?;
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn lock() -> Result<std::fs::File> {
    std::fs::create_dir_all(service::data_dir())?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(service::data_dir().join("update.lock"))?;
    fs2::FileExt::try_lock_exclusive(&file).context("another host update is already running")?;
    Ok(file)
}

pub fn status() -> Result<Value> {
    let executable = service::current_exe()?;
    status_at(&executable)
}

fn status_at(executable: &Path) -> Result<Value> {
    let state: State = read("update-state.json")?;
    let config: Config = read("update-config.json")?;
    Ok(json!({
        "currentVersion": env!("CARGO_PKG_VERSION"), "method": install::method(executable),
        "automatic": config.automatic && require_self_updating(executable).is_ok(), "state": state,
    }))
}

pub fn set_automatic(enabled: bool) -> Result<Value> {
    let executable = service::current_exe()?;
    if enabled {
        crate::environment::Environment::current().require_release_updates()?;
        install::require_standalone(&executable)?;
        ensure!(service::installed(), "install the background host service before enabling automatic updates");
        service::require_executable(&executable)?;
    }
    write("update-config.json", &Config { automatic: enabled })?;
    status()
}

pub async fn check() -> Result<Value> {
    crate::environment::Environment::current().require_release_updates()?;
    let _lock = lock()?;
    let mut state: State = read("update-state.json")?;
    ensure!(!awaiting_worker(&state), "a host update is starting");
    state.checked_at = Some(crate::store::now_ms());
    match release::latest().await {
        Ok(release) => {
            note_latest(&release.version);
            let available = (semver::Version::parse(&release.version)?
                > semver::Version::parse(env!("CARGO_PKG_VERSION"))?)
            .then_some(release.version);
            state.phase = if available.is_some() { "available" } else { "upToDate" }.into();
            state.available_version = available;
            state.error = None;
        }
        Err(error) => {
            state.phase = "failed".into();
            state.error = Some(format!("{error:#}"));
            write("update-state.json", &state)?;
            return Err(error);
        }
    }
    write("update-state.json", &state)?;
    status()
}

pub async fn apply(port: u16, force: bool, worker: bool) -> Result<Value> {
    crate::environment::Environment::current().require_release_updates()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let _lock = loop {
        match lock() {
            Ok(lock) => break lock,
            Err(_) if worker && tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error),
        }
    };
    let target = service::current_exe()?;
    install::require_standalone(&target)?;
    let mut state = State { phase: "checking".into(), checked_at: Some(crate::store::now_ms()), ..State::default() };
    write("update-state.json", &state)?;
    let result = async {
        let release = release::latest().await?;
        if semver::Version::parse(&release.version)? <= semver::Version::parse(env!("CARGO_PKG_VERSION"))? {
            state.phase = "upToDate".into();
            return Ok(());
        }
        state.available_version = Some(release.version.clone());
        state.phase = "installing".into();
        write("update-state.json", &state)?;
        install::apply(&release, &target, port, force).await?;
        state.phase = "complete".into();
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(error) = &result {
        state.phase = "failed".into();
        state.error = Some(format!("{error:#}"));
    }
    write("update-state.json", &state)?;
    result?;
    // On Linux /proc/self/exe refers to the unlinked old inode after replacement.
    // Reuse the path captured before installing rather than resolving it again.
    let mut status = status_at(&target)?;
    if state.phase == "complete" {
        status["currentVersion"] = json!(state.available_version);
    }
    Ok(status)
}

/// The updater must live outside the daemon's job/cgroup. Otherwise stopping
/// the host would also kill the process responsible for starting its replacement.
pub fn spawn_worker(port: u16, force: bool) -> Result<()> {
    crate::environment::Environment::current().require_release_updates()?;
    let _lock = lock()?;
    let previous: State = read("update-state.json")?;
    ensure!(!awaiting_worker(&previous), "a host update is already starting");
    let executable = service::current_exe()?;
    require_self_updating(&executable)?;
    let mut args = vec!["update".to_owned(), "--worker".to_owned(), "--port".to_owned(), port.to_string()];
    if force {
        args.push("--force".to_owned());
    }
    let mut state = State { phase: "scheduled".into(), checked_at: Some(crate::store::now_ms()), ..previous };
    state.error = None;
    write("update-state.json", &state)?;
    if let Err(error) = spawn_job(&executable, &args) {
        state.phase = "failed".into();
        state.error = Some(format!("{error:#}"));
        write("update-state.json", &state)?;
        return Err(error);
    }
    Ok(())
}

fn awaiting_worker(state: &State) -> bool {
    state.phase == "scheduled" && state.checked_at.is_some_and(|at| crate::store::now_ms().saturating_sub(at) < 60_000)
}

fn spawn_job(executable: &Path, args: &[String]) -> Result<()> {
    let mut command;
    if cfg!(target_os = "linux") {
        command = std::process::Command::new("systemd-run");
        command
            .args(["--user", "--collect", "--unit=codync-host-update", "--property=Type=exec"])
            .arg(format!("--setenv=CODYNC_HOME={}", service::data_dir().display()))
            .arg("--")
            .arg(executable)
            .args(args);
    } else {
        // submit jobs do not restart on exit. Remove only a completed old job;
        // an active worker holds update.lock, acquired by our caller above.
        let _ = std::process::Command::new("launchctl").args(["remove", "com.pokai.codync.update"]).output();
        command = std::process::Command::new("launchctl");
        command
            .args(["submit", "-l", "com.pokai.codync.update", "-o"])
            .arg(service::data_dir().join("update.log"))
            .arg("-e")
            .arg(service::data_dir().join("update.log"))
            .args(["--", "/usr/bin/env"])
            .arg(format!("CODYNC_HOME={}", service::data_dir().display()))
            .arg(executable)
            .args(args);
    }
    let output = command.output().context("launching the independent host updater")?;
    ensure!(output.status.success(), "could not launch updater: {}", String::from_utf8_lossy(&output.stderr).trim());
    Ok(())
}

pub async fn automatic_loop(hub: Arc<Hub>) {
    if crate::environment::Environment::current() != crate::environment::Environment::Main {
        return;
    }
    let mut last_check = None;
    loop {
        tokio::time::sleep(Duration::from_mins(1)).await;
        let enabled = read::<Config>("update-config.json").is_ok_and(|c| c.automatic);
        if !enabled || hub.busy() {
            continue;
        }
        if last_check.is_some_and(|at: tokio::time::Instant| at.elapsed() < Duration::from_hours(24)) {
            continue;
        }
        last_check = Some(tokio::time::Instant::now());
        if !service::current_exe().is_ok_and(|exe| require_self_updating(&exe).is_ok()) {
            continue;
        }
        match check().await {
            Ok(status) if status["state"]["availableVersion"].is_string() => {
                let port = hub.port;
                match tokio::task::spawn_blocking(move || spawn_worker(port, false)).await {
                    Ok(Ok(())) => {}
                    result => tracing::warn!(?result, "could not start automatic host update"),
                }
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "host update check failed"),
        }
    }
}
