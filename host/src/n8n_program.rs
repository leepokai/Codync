//! n8n's program: the instance the host starts needs Node.js 24 and n8n's own packages. Like ComfyUI, they
//! don't travel inside the installer (n8n and its dependencies are several hundred MB); the first start gets
//! them once, under `<data>/n8n-program`, and a start after that goes straight to the editor:
//!
//! 1. the pinned Node.js portable from nodejs.org, checked by size and SHA-256 while it arrives (skipped when
//!    this computer already has Node.js 24 or newer, or after the first time);
//! 2. the pinned `n8n` release from the npm registry, installed with that Node's own npm.
//!
//! The pins live in `packaging/n8n/version.json`. A folder the person chose in Settings (their own n8n
//! checkout, started the way `n8n.rs` always has) still wins over all of this.

use crate::LockExt;
use crate::agent::backends;
use crate::service;
use crate::tools::comfy_install::{Source, already_downloaded, download, rename_patiently, seven_zip};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Whether this computer can fetch Node.js by itself: the pinned portable is the Windows x64 one.
const NODE_DOWNLOAD: bool = cfg!(all(windows, target_arch = "x86_64"));
/// The oldest Node.js major n8n runs on.
pub(crate) const NODE_MAJOR: u32 = 24;
/// The longest the npm install may take: n8n brings a few thousand packages.
const INSTALL_TIMEOUT: Duration = Duration::from_mins(25);
/// Free space the program wants: the Node.js archive and folder, npm's cache and n8n's packages.
const FREE_NEEDED: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Deserialize)]
struct Pin {
    n8n: String,
    node: NodePin,
}

#[derive(Deserialize)]
struct NodePin {
    version: String,
    url: String,
    size: u64,
    sha256: String,
}

fn pin() -> Pin {
    serde_json::from_str(include_str!("../../packaging/n8n/version.json")).expect("packaging/n8n/version.json is valid")
}

/// Which n8n build runs: the names of its environment variables, session cookie and API-key header differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// A checkout the person chose: the `automan` fork's `packages/cli/bin/automan`.
    Automan,
    /// The pinned official release this module downloads.
    Official,
}

impl Flavor {
    /// The environment variable `name` of this build (`USER_FOLDER`, `PORT`…).
    pub fn env(self, name: &str) -> String {
        format!("{}_{name}", if self == Self::Automan { "AUTOMAN" } else { "N8N" })
    }

    /// The session cookie the editor signs in with.
    pub fn cookie(self) -> &'static str {
        if self == Self::Automan { "automan-auth" } else { "n8n-auth" }
    }

    /// The header the public API reads its key from.
    pub fn key_header(self) -> &'static str {
        if self == Self::Automan { "X-AUTOMAN-API-KEY" } else { "X-N8N-API-KEY" }
    }
}

/// What a start runs: `node entry …` in `cwd`.
pub struct Program {
    pub node: PathBuf,
    pub entry: PathBuf,
    pub cwd: PathBuf,
    pub flavor: Flavor,
}

fn base_dir() -> PathBuf {
    service::data_dir().join("n8n-program")
}

fn node_dir(version: &str) -> PathBuf {
    base_dir().join("node").join(version)
}

fn app_dir(version: &str) -> PathBuf {
    base_dir().join("app").join(version)
}

fn entry_of(app: &Path) -> PathBuf {
    app.join("node_modules/n8n/bin/n8n")
}

fn node_exe(dir: &Path) -> PathBuf {
    dir.join(if cfg!(windows) { "node.exe" } else { "bin/node" })
}

/// Node.js on this computer at least `min_major`: the one this module downloaded, else the one on the PATH.
pub(crate) fn node_at_least(min_major: u32) -> Result<PathBuf> {
    let downloaded = node_exe(&node_dir(&pin().node.version));
    if downloaded.is_file() {
        return Ok(downloaded);
    }
    let node = backends::which("node").context("Node.js isn't on this computer's PATH")?;
    let out = std::process::Command::new(&node).arg("--version").output().context("checking Node.js's version")?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let major: u32 = version.trim_start_matches('v').split('.').next().unwrap_or_default().parse().unwrap_or(0);
    ensure!(major >= min_major, "n8n needs Node.js {min_major} or newer; this computer has {version}");
    Ok(node)
}

/// The program on this computer already, when it is whole.
pub fn downloaded() -> Option<Program> {
    let app = app_dir(&pin().n8n);
    let entry = entry_of(&app);
    let node = node_at_least(NODE_MAJOR).ok()?;
    entry.is_file().then_some(Program { node, entry, cwd: app, flavor: Flavor::Official })
}

/// `node_modules/npm/bin/npm-cli.js` beside a Node.js, in the Windows layout or the Unix one.
fn npm_cli(node: &Path) -> Option<PathBuf> {
    let dir = node.parent()?;
    [dir.join("node_modules/npm/bin/npm-cli.js"), dir.parent()?.join("lib/node_modules/npm/bin/npm-cli.js")]
        .into_iter()
        .find(|p| p.is_file())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Idle,
    Downloading,
    Unpacking,
    Installing,
    Failed,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Downloading => "downloading",
            Self::Unpacking => "unpacking",
            Self::Installing => "installing",
            Self::Failed => "failed",
        }
    }
}

struct Progress {
    phase: Phase,
    done: u64,
    total: u64,
    error: Option<String>,
}

/// Gets the program onto this computer, one install at a time.
pub struct Installer {
    progress: Mutex<Progress>,
    busy: tokio::sync::Mutex<()>,
}

impl Default for Installer {
    fn default() -> Self {
        Self {
            progress: Mutex::new(Progress { phase: Phase::Idle, done: 0, total: 0, error: None }),
            busy: tokio::sync::Mutex::new(()),
        }
    }
}

impl Installer {
    /// What the settings page and the start page show while the program arrives.
    pub fn status(&self) -> Value {
        let p = self.progress.locked();
        json!({ "phase": p.phase.name(), "done": p.done, "total": p.total, "error": p.error })
    }

    fn set(&self, notify: &impl Fn(), phase: Phase, done: u64, total: u64, error: Option<String>) {
        *self.progress.locked() = Progress { phase, done, total, error };
        notify();
    }

    /// The program, fetching it first when it isn't here yet; `notify` runs on every change of the progress.
    pub async fn ensure(&self, notify: impl Fn()) -> Result<Program> {
        if let Some(program) = downloaded() {
            return Ok(program);
        }
        let _busy = self.busy.lock().await;
        if let Some(program) = downloaded() {
            return Ok(program);
        }
        let result = self.install(&notify).await;
        match &result {
            Ok(_) => self.set(&notify, Phase::Idle, 0, 0, None),
            Err(e) => {
                tracing::warn!(error = format!("{e:#}"), "getting n8n's program");
                self.set(&notify, Phase::Failed, 0, 0, Some(format!("{e:#}")));
            }
        }
        result
    }

    async fn install(&self, notify: &impl Fn()) -> Result<Program> {
        let pin = pin();
        let root = base_dir();
        tokio::fs::create_dir_all(&root).await.context("creating n8n's program folder")?;
        if let Some(free) = crate::tools::comfy::free_space(&root) {
            ensure!(
                free >= FREE_NEEDED,
                "n8n needs {} GB free to download; {} GB are free",
                FREE_NEEDED >> 30,
                free >> 30
            );
        }
        let node = match node_at_least(NODE_MAJOR) {
            Ok(node) => node,
            Err(e) if NODE_DOWNLOAD => {
                tracing::info!(error = format!("{e:#}"), "no usable Node.js: downloading the pinned one");
                self.fetch_node(&pin.node, notify).await?
            }
            Err(e) => return Err(e),
        };
        self.set(notify, Phase::Installing, 0, 0, None);
        let app = app_dir(&pin.n8n);
        npm_install(&node, &app, &pin.n8n).await?;
        let entry = entry_of(&app);
        ensure!(entry.is_file(), "the n8n install has no {}", entry.display());
        remove_other_versions(&root.join("app"), &app);
        Ok(Program { node, entry, cwd: app, flavor: Flavor::Official })
    }

    /// The pinned Node.js portable, downloaded, checked and unpacked; returns its `node` binary.
    async fn fetch_node(&self, pin: &NodePin, notify: &impl Fn()) -> Result<PathBuf> {
        let seven = seven_zip().context("7za.exe is missing beside the host: reinstall Codync")?;
        let work = base_dir().join(".download");
        tokio::fs::create_dir_all(&work).await.context("preparing the download folder")?;
        let archive = work.join("node.zip");
        let source = Source { url: pin.url.clone(), size: pin.size, sha256: pin.sha256.clone() };
        let (checked, expected) = (archive.clone(), (source.size, source.sha256.clone()));
        if !tokio::task::spawn_blocking(move || already_downloaded(&checked, expected.0, &expected.1)).await? {
            self.set(notify, Phase::Downloading, 0, source.size, None);
            download(&source, &archive, |done| self.set(notify, Phase::Downloading, done, source.size, None)).await?;
        }
        self.set(notify, Phase::Unpacking, 0, 0, None);
        let target = node_dir(&pin.version);
        let version = pin.version.clone();
        let placed = target.clone();
        tokio::task::spawn_blocking(move || unpack_node(&seven, &archive, &work, &version, &placed))
            .await
            .context("unpacking Node.js")??;
        Ok(node_exe(&target))
    }
}

/// Unpacks the Node.js archive and moves its `node-<version>-win-x64` folder to `target`. Blocking.
fn unpack_node(seven: &Path, archive: &Path, work: &Path, version: &str, target: &Path) -> Result<()> {
    let unpacked = work.join("unpacked");
    if unpacked.exists() {
        std::fs::remove_dir_all(&unpacked).with_context(|| format!("clearing {}", unpacked.display()))?;
    }
    let mut command = std::process::Command::new(seven);
    command
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", unpacked.display()))
        .arg(archive)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let out = command.output().context("running 7za")?;
    ensure!(out.status.success(), "unpacking Node.js failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    let inner = unpacked.join(format!("node-{version}-win-x64"));
    ensure!(node_exe(&inner).is_file(), "the download doesn't hold Node.js");
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    if target.exists() {
        std::fs::remove_dir_all(target).with_context(|| format!("clearing {}", target.display()))?;
    }
    rename_patiently(&inner, target).with_context(|| format!("moving Node.js to {}", target.display()))?;
    let _ = std::fs::remove_dir_all(work);
    Ok(())
}

/// `npm install n8n@<version>` into `app`, with the Node.js that will run it.
async fn npm_install(node: &Path, app: &Path, version: &str) -> Result<()> {
    let npm = npm_cli(node).context("this Node.js has no npm beside it")?;
    tokio::fs::create_dir_all(app).await.context("creating n8n's app folder")?;
    let path = std::env::join_paths(
        node.parent()
            .map(Path::to_path_buf)
            .into_iter()
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())),
    )?;
    let mut command = tokio::process::Command::new(node);
    command
        .arg(npm)
        .arg("install")
        .arg("--prefix")
        .arg(app)
        .arg(format!("n8n@{version}"))
        .args(["--no-audit", "--no-fund", "--no-update-notifier", "--loglevel=error"])
        .env("PATH", path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    let child = command.spawn().context("running npm")?;
    let out = match tokio::time::timeout(INSTALL_TIMEOUT, child.wait_with_output()).await {
        Ok(out) => out.context("running npm")?,
        Err(_) => bail!("installing n8n took longer than {} minutes", INSTALL_TIMEOUT.as_secs() / 60),
    };
    if !out.status.success() {
        let text = String::from_utf8_lossy(&out.stderr);
        let tail: String = text.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
        bail!("installing n8n failed: {}", tail.trim());
    }
    Ok(())
}

/// Removes the app versions beside `keep`: a new pin replaces the old one.
fn remove_other_versions(apps: &Path, keep: &Path) {
    for entry in std::fs::read_dir(apps).into_iter().flatten().filter_map(Result::ok) {
        let path = entry.path();
        if path != keep
            && path.is_dir()
            && let Err(e) = std::fs::remove_dir_all(&path)
        {
            tracing::warn!(error = format!("{e:#}"), path = %path.display(), "removing an older n8n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pin_is_readable_and_names_n8n_and_node() {
        let pin = pin();
        assert_eq!(pin.n8n.split('.').count(), 3);
        assert!(pin.node.url.contains(&pin.node.version));
        assert_eq!(pin.node.sha256.len(), 64);
    }

    #[test]
    fn each_build_has_its_own_names() {
        assert_eq!(Flavor::Automan.env("PORT"), "AUTOMAN_PORT");
        assert_eq!(Flavor::Official.env("PORT"), "N8N_PORT");
        assert_eq!(Flavor::Automan.cookie(), "automan-auth");
        assert_eq!(Flavor::Official.cookie(), "n8n-auth");
        assert_eq!(Flavor::Official.key_header(), "X-N8N-API-KEY");
    }

    #[test]
    fn an_older_app_version_is_removed_and_the_pinned_one_kept() {
        let dir = std::env::temp_dir().join(format!("codync-n8n-program-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (old, keep) = (dir.join("1.0.0"), dir.join("2.0.0"));
        std::fs::create_dir_all(&old).expect("old");
        std::fs::create_dir_all(&keep).expect("keep");
        remove_other_versions(&dir, &keep);
        assert!(!old.exists() && keep.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
