//! Node.js for the agents whose ACP adapter comes from npm (Claude, Codex, Gemini, …) and for
//! installing npm CLIs: the computer's own when it has one, else a checksum-pinned official
//! build that Codync adds into `~/.codync/node` when the user asks (an agent's setup, or
//! installing an npm CLI).

use crate::{backends, registry};
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};
use std::time::Duration;

const VERSION: &str = "24.21.0";
const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// nodejs.org's name for a platform, and its archive's SHA-256 (from `SHASUMS256.txt`).
fn archive(os: &str, arch: &str) -> Option<(&'static str, &'static str)> {
    Some(match (os, arch) {
        ("macos", "aarch64") => ("darwin-arm64", "bed7eea5325e1108f32ce5228ddd6a5f0f08a499ee42aa7442aea583702f6057"),
        ("macos", "x86_64") => ("darwin-x64", "1462cb3b3046b815cf8ea436d3da450ec1a9f11dac7e5a46b0ada5305d7e8097"),
        ("linux", "aarch64") => ("linux-arm64", "724282c3b43aec998aa9527380465b45d229e021b58035f5f4f63095eabfe5d5"),
        ("linux", "x86_64") => ("linux-x64", "6e1db87ef58b8819e5d5402eff1536491b18edd8eb7bee5ef7897876e88dc5ff"),
        ("windows", "aarch64") => ("win-arm64", "8779b1bde1d39f8d420e3b57aa657b39891af434d3de44a919044cec06785921"),
        ("windows", "x86_64") => ("win-x64", "158f7685b44de51f6c0df1d153526cbcd3e1bc739a8dfc607721cef75de9e541"),
        _ => return None,
    })
}

/// `~/.codync/node/<version>`: Codync's Node.
fn root() -> PathBuf {
    super::setup::get().data_dir.join("node").join(VERSION)
}

/// Where `npm install -g` puts CLIs when it runs on Codync's Node; outlives Node updates.
fn global_prefix() -> PathBuf {
    super::setup::get().data_dir.join("node").join("global")
}

/// The commands under an npm prefix: Windows keeps them at the top.
fn bin_dir(prefix: PathBuf) -> PathBuf {
    if cfg!(windows) { prefix } else { prefix.join("bin") }
}

/// Codync's Node and the CLIs installed with it, for the agent search path (after the
/// computer's own, so its Node wins).
pub fn search_dirs() -> [PathBuf; 2] {
    [bin_dir(root()), bin_dir(global_prefix())]
}

/// Node.js on a computer without its own: Codync's, missing or added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Missing,
    Added,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Added => "added",
        }
    }
}

/// Whether every npm agent on this computer can start: its own `npx`, Codync's installed
/// copy, or a checksum-pinned archive for its operating system and architecture.
pub fn npm_supported() -> bool {
    backends::which("npx").is_some_and(|path| !path.starts_with(root()))
        || root().join(".installed").exists()
        || archive(std::env::consts::OS, std::env::consts::ARCH).is_some()
}

/// `None` while the computer has its own Node; otherwise whether Codync's is there.
pub fn state() -> Option<State> {
    state_for("npx")
}

/// `program` is the command the agent actually runs: npm adapters need `npx`, while `WorkBuddy` runs `node`.
pub fn state_for(program: &str) -> Option<State> {
    if backends::which(program).is_some_and(|path| !path.starts_with(root())) {
        return None;
    }
    Some(if root().join(".installed").exists() { State::Added } else { State::Missing })
}

/// The command `program` for starting an agent, adding Codync's Node when the computer has neither.
///
/// Agents launched from npm need this before their ACP process starts. A system command wins;
/// otherwise the download is checksum-pinned and shared by every npm agent.
pub async fn prepare(program: &str, progress: &impl Fn(&str)) -> Result<PathBuf> {
    if let Some(path) = backends::which(program).filter(|path| !path.starts_with(root())) {
        return Ok(path);
    }
    install(progress).await?;
    let path = codync_program(program).context("Codync's Node.js is missing a required command")?;
    ensure!(path.is_file(), "Codync's Node.js is missing {program}");
    Ok(path)
}

/// PATH for an agent that runs Codync's Node: its bin folder first, then the computer's own.
/// `npx` starts npm by name, and a downloaded CLI's shim runs `node` by name.
pub fn launch_path(program: &Path) -> Result<String> {
    if program.starts_with(root()) { path_value_for(&root()) } else { Ok(std::env::var("PATH").unwrap_or_default()) }
}

/// A command that runs beside `program`: its own folder for Codync's Node, else the normal search path.
pub fn program_for(program: &Path, command: &str, path: &str) -> Result<PathBuf> {
    if program.starts_with(root()) {
        return codync_program(command).context("Codync's Node.js is missing a required command");
    }
    let path = std::env::split_paths(path)
        .flat_map(|dir| {
            let names: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat", ""] } else { &[""] };
            names.iter().map(move |extension| dir.join(format!("{command}{extension}")))
        })
        .find(|candidate| candidate.is_file());
    path.with_context(|| format!("{command} is missing"))
}

fn path_value_for(root: &Path) -> Result<String> {
    let bin = bin_dir(root.to_path_buf());
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::iter::once(bin).chain(std::env::split_paths(&path));
    Ok(std::env::join_paths(dirs)?.to_string_lossy().into_owned())
}

/// The path of `program` under Codync's Node, trying the extensions Windows runs.
fn codync_program(program: &str) -> Option<PathBuf> {
    let dir = bin_dir(root());
    let extensions: &[&str] = if cfg!(windows) { &[".cmd", ".exe", ".bat", ""] } else { &[""] };
    extensions.iter().map(|extension| dir.join(format!("{program}{extension}"))).find(|path| path.is_file())
}

/// `codync-host node`: adds Codync's Node (an agent's setup step runs it in a terminal).
///
/// `program` is the command the agent actually uses. `None` checks both `node` and
/// `npx`, for the marketplace's Add Node.js row that belongs to the computer rather
/// than one agent.
pub async fn add(program: Option<&str>) -> Result<()> {
    let present = match program {
        Some(program) => state_for(program).is_none(),
        None => state_for("node").is_none() && state_for("npx").is_none(),
    };
    if present {
        println!("This computer already has the required Node.js command.");
        return Ok(());
    }
    install(&|line| println!("{line}")).await?;
    println!("Node.js {VERSION} is ready.");
    Ok(())
}

/// `codync-host npm …`: npm on the computer's Node, else on Codync's, whose global installs
/// land in `global_prefix`. Setup terminals install npm CLIs through it.
pub async fn npm(args: &[String]) -> Result<i32> {
    let mut command =
        if let Some(npm) = backends::which("npm") { tokio::process::Command::new(npm) } else { codync_npm().await? };
    let status = command.args(args).status().await.context("running npm")?;
    Ok(status.code().unwrap_or(1))
}

/// npm on Codync's Node (downloaded first), with its own Node first on PATH.
async fn codync_npm() -> Result<tokio::process::Command> {
    install(&|line| println!("{line}")).await?;
    let node_root = root();
    let mut command =
        tokio::process::Command::new(bin_dir(node_root.clone()).join(if cfg!(windows) { "npm.cmd" } else { "npm" }));
    command.env("PATH", path_value_for(&node_root)?).env("npm_config_prefix", global_prefix());
    Ok(command)
}

async fn install(progress: &impl Fn(&str)) -> Result<()> {
    let _one_at_a_time = INSTALL.lock().await;
    let root = root();
    if tokio::fs::try_exists(root.join(".installed")).await.unwrap_or(false) {
        return Ok(());
    }
    let (platform, checksum) = archive(std::env::consts::OS, std::env::consts::ARCH)
        .context("Codync has no Node.js build for this computer: install Node.js from nodejs.org")?;
    let name = format!("node-v{VERSION}-{platform}");
    let file = format!("{name}.{}", if cfg!(windows) { "zip" } else { "tar.gz" });
    progress("Downloading Node.js…");
    let mut response = (super::setup::get().http)()
        .get(format!("https://nodejs.org/dist/v{VERSION}/{file}"))
        .timeout(Duration::from_mins(10))
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(bytes.len() + chunk.len() <= MAX_ARCHIVE_BYTES, "Node.js download exceeds size limit");
        bytes.extend_from_slice(&chunk);
    }
    ensure!(registry::sha256_hex(&bytes) == checksum, "Node.js download checksum mismatch");
    progress("Installing Node.js…");
    tokio::task::spawn_blocking(move || unpack(&root, &name, &file, &bytes)).await?.context("installing Node.js")
}

/// Blocking: extracts into a staging folder next to `root`, then moves it into place.
fn unpack(root: &Path, name: &str, file: &str, bytes: &[u8]) -> Result<()> {
    let parent = root.parent().context("Node.js folder has no parent")?;
    let stage = parent.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let placed = place(root, &stage, name, file, bytes);
    let _ = std::fs::remove_dir_all(&stage);
    if placed.is_ok() {
        remove_other_versions(root);
    }
    placed
}

fn place(root: &Path, stage: &Path, name: &str, file: &str, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(stage)?;
    let archive = stage.join(file);
    std::fs::write(&archive, bytes)?;
    let node = Path::new(name).join(if cfg!(windows) { "node.exe" } else { "bin/node" });
    registry::extract(&archive, stage, &node)?;
    let out = std::process::Command::new(stage.join(&node)).arg("--version").output()?;
    let runs = out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == format!("v{VERSION}");
    ensure!(runs, "the downloaded Node.js doesn't run on this computer");
    // Another process (a setup terminal's `codync-host npm`) may have finished first.
    if !root.join(".installed").exists() {
        let _ = std::fs::remove_dir_all(root);
        std::fs::rename(stage.join(name), root)?;
        std::fs::write(root.join(".installed"), VERSION)?;
    }
    Ok(())
}

/// Older Nodes Codync downloaded (version-named folders; never `global` or a staging folder
/// another process is still filling).
fn remove_other_versions(root: &Path) {
    let Some(parent) = root.parent() else { return };
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        let versioned =
            path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(|c: char| c.is_ascii_digit()));
        if versioned && path != root && path.is_dir() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn node_path_comes_first() {
        let root = std::env::temp_dir().join(format!("codync-node-path-{}", uuid::Uuid::new_v4()));
        let value = path_value_for(&root).unwrap();
        let first = std::env::split_paths(&value).next().unwrap();
        assert_eq!(first, root.join("bin"));
    }

    #[test]
    fn every_supported_platform_has_a_pinned_archive() {
        for os in ["macos", "linux", "windows"] {
            for arch in ["x86_64", "aarch64"] {
                let (platform, checksum) = archive(os, arch).expect("supported");
                assert_ne!(platform, "");
                assert!(checksum.len() == 64 && checksum.chars().all(|c| c.is_ascii_hexdigit()));
            }
        }
        assert!(archive("linux", "riscv64").is_none());
    }
}
