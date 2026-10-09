//! The official ACP agent registry (<https://agentclientprotocol.com/registry>):
//! ~40 coding agents with a known-good way to launch each over ACP — an npx or
//! uvx package, or a per-platform binary archive we download on first use.
//!
//! Registry JSON is untrusted input: ids, versions and commands become paths on
//! disk, so they're checked before use.

use crate::LockExt;
use crate::shell;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const URL: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
/// Upstream updates hourly; once a day is plenty for picking agents.
const REFRESH_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

static CACHE: Mutex<Option<Value>> = Mutex::new(None);

/// How this machine would launch a registry agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    /// A per-platform archive, downloaded once into `~/.codync/agents`.
    Download,
    Npx,
    Uvx,
}

fn cache_file() -> PathBuf {
    crate::service::data_dir().join("registry.json")
}

/// Registry agents (from memory, else the on-disk copy).
pub fn agents() -> Vec<Value> {
    let mut cache = CACHE.locked();
    if cache.is_none() {
        *cache = std::fs::read_to_string(cache_file()).ok().and_then(|s| serde_json::from_str(&s).ok());
    }
    cache.as_ref().and_then(|v| v["agents"].as_array().cloned()).unwrap_or_default()
}

pub fn agent(id: &str) -> Option<Value> {
    agents().into_iter().find(|a| a["id"] == id)
}

pub async fn refresh_loop() {
    loop {
        match fetch().await {
            Ok(v) => {
                if let Err(error) = tokio::fs::write(cache_file(), v.to_string()).await {
                    tracing::warn!(%error, "couldn't cache the ACP registry");
                }
                *CACHE.locked() = Some(v);
            }
            Err(error) => {
                tracing::info!(error = format!("{error:#}"), "ACP registry refresh failed; using the cached copy");
            }
        }
        tokio::time::sleep(REFRESH_EVERY).await;
    }
}

async fn fetch() -> Result<Value> {
    let v: Value =
        crate::http().get(URL).timeout(Duration::from_secs(20)).send().await?.error_for_status()?.json().await?;
    if !v["agents"].is_array() {
        bail!("unexpected registry format");
    }
    Ok(v)
}

pub fn platform() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin-aarch64",
        ("macos", "x86_64") => "darwin-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("linux", "x86_64") => "linux-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => return None,
    })
}

pub fn launch_kind(agent: &Value) -> Option<Launch> {
    let d = &agent["distribution"];
    let on_path = crate::agent::backends::on_path;
    if let Some(p) = platform()
        && d["binary"][p].is_object()
    {
        return Some(Launch::Download);
    }
    if d["npx"].is_object() && on_path("npx") {
        return Some(Launch::Npx);
    }
    if d["uvx"].is_object() && on_path("uvx") {
        return Some(Launch::Uvx);
    }
    None
}

fn args_of(v: &Value) -> String {
    v["args"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).map(shell::quote).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// `KEY=value ` pairs from a registry `env` object.
pub fn env_prefix(env: &Value) -> String {
    shell::env_prefix(env.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.as_str(), v.as_str()?))))
}

/// One path component made only of `[A-Za-z0-9._-]` and not `.`/`..`.
fn safe_component(s: &str) -> Option<&str> {
    let ok = !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    ok.then_some(s)
}

/// A relative path that stays inside its base directory (no `..`, no root).
fn contained(rel: &str) -> Option<&Path> {
    let p = Path::new(rel);
    p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)).then_some(p)
}

/// How to run a registry agent: `program` alone (what ACP `terminal` sign-in
/// methods add their own args to) and `args` for its ACP mode.
pub struct Cmd {
    pub program: String,
    pub args: String,
}

impl Cmd {
    /// The shell command that starts it over ACP stdio.
    pub fn acp(&self) -> String {
        format!("{} {}", self.program, self.args)
    }
}

/// Downloads the agent first if needed.
/// `progress` receives short status lines ("Downloading Cursor…").
pub async fn command(agent: &Value, progress: impl Fn(&str)) -> Result<Cmd> {
    let name = agent["name"].as_str().unwrap_or("agent");
    let d = &agent["distribution"];
    match launch_kind(agent) {
        Some(Launch::Download) => {
            let platform = platform().ok_or_else(|| anyhow!("no download for this platform"))?;
            let target = &d["binary"][platform];
            let cmd =
                target["cmd"].as_str().and_then(contained).ok_or_else(|| anyhow!("registry entry has a bad cmd"))?;
            let dir = install_binary(agent, target, cmd, &progress).await?;
            Ok(Cmd {
                program: format!("{}{}", env_prefix(&target["env"]), shell::quote(&dir.join(cmd).to_string_lossy())),
                args: args_of(target),
            })
        }
        Some(Launch::Npx) => {
            prepare_npx(d["npx"]["package"].as_str().unwrap_or_default(), name, &progress).await?;
            Ok(npx_cmd(&d["npx"]))
        }
        Some(Launch::Uvx) => Ok(Cmd {
            program: format!(
                "{}uvx {}",
                env_prefix(&d["uvx"]["env"]),
                shell::quote(d["uvx"]["package"].as_str().unwrap_or_default())
            ),
            args: args_of(&d["uvx"]),
        }),
        None => bail!(
            "{name} has no build for this computer (needs {})",
            if d["uvx"].is_object() { "uv" } else { "Node.js" }
        ),
    }
}

fn npx_cmd(npx: &Value) -> Cmd {
    Cmd {
        program: format!(
            "{}npx -y {}",
            env_prefix(&npx["env"]),
            shell::quote(npx["package"].as_str().unwrap_or_default())
        ),
        args: args_of(npx),
    }
}

/// npx installs a package into `<npm cache>/_npx/<hash>` and writes its `package.json` last.
/// An install cut short (the first Codex download takes minutes, longer than the ACP handshake
/// may) leaves the packages without their bin links, and npx then fails with "command not
/// found" on every later run. So the download happens here, without the handshake's timeout,
/// and a half-finished one is started over.
async fn prepare_npx(package: &str, name: &str, progress: &impl Fn(&str)) -> Result<()> {
    static INSTALLING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one_at_a_time = INSTALLING.lock().await;
    let dir = npm_cache().await?.join("_npx").join(npx_hash(package));
    if tokio::fs::try_exists(dir.join("package.json")).await.unwrap_or(false) {
        return Ok(());
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
    progress(&format!("Downloading {name}…"));
    let install = tokio::process::Command::new(shell::program("npm"))
        .args(["exec", "--yes", "--package", package, "-c", "true"])
        // Away from any project, so npx doesn't settle for a local node_modules.
        .current_dir(crate::service::data_dir())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let failure = match tokio::time::timeout(Duration::from_secs(20 * 60), install).await {
        Ok(Ok(out)) if out.status.success() => return Ok(()),
        Ok(Ok(out)) => String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or_default().to_owned(),
        Ok(Err(e)) => format!("{e}"),
        Err(_) => "timed out".to_owned(),
    };
    let _ = tokio::fs::remove_dir_all(&dir).await;
    bail!("couldn't download {name} with npm: {failure}")
}

/// `npm config get cache`, asked once.
async fn npm_cache() -> Result<&'static Path> {
    static DIR: tokio::sync::OnceCell<PathBuf> = tokio::sync::OnceCell::const_new();
    let dir = DIR
        .get_or_try_init(|| async {
            let out = tokio::process::Command::new(shell::program("npm"))
                .args(["config", "get", "cache"])
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .context("running npm")?;
            let dir = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if !out.status.success() || dir.is_empty() {
                bail!("npm didn't report its cache directory");
            }
            Ok(PathBuf::from(dir))
        })
        .await?;
    Ok(dir)
}

/// npx's cache key for one package spec (libnpmexec: the first 16 hex digits of its SHA-512).
fn npx_hash(package: &str) -> String {
    use std::fmt::Write as _;
    sha2::Sha512::digest(package.as_bytes())[..8].iter().fold(String::with_capacity(16), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Downloads + extracts a binary distribution into `~/.codync/agents/<id>/<version>`.
async fn install_binary(agent: &Value, target: &Value, cmd: &Path, progress: &impl Fn(&str)) -> Result<PathBuf> {
    let id = agent["id"].as_str().and_then(safe_component).ok_or_else(|| anyhow!("registry entry has a bad id"))?;
    let version = agent["version"].as_str().and_then(safe_component).unwrap_or("latest");
    let dir = crate::service::data_dir().join("agents").join(id).join(version);
    if tokio::fs::try_exists(dir.join(".installed")).await.unwrap_or(false) {
        return Ok(dir);
    }
    let url = target["archive"].as_str().ok_or_else(|| anyhow!("registry entry has no archive"))?;
    if !url.starts_with("https://") {
        bail!("refusing a non-HTTPS download for {id}");
    }
    progress(&format!("Downloading {}…", agent["name"].as_str().unwrap_or(id)));
    let bytes = crate::http()
        .get(url)
        .timeout(Duration::from_secs(600))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await
        .with_context(|| format!("downloading {id}"))?;
    if let Some(want) = target["sha256"].as_str()
        && !sha256_hex(&bytes).eq_ignore_ascii_case(want)
    {
        bail!("download checksum mismatch for {id}");
    }
    progress("Installing…");
    let file_name =
        url.rsplit('/').next().and_then(|f| f.split('?').next()).and_then(safe_component).unwrap_or("download");
    let (dir2, file_name, cmd) = (dir.clone(), file_name.to_owned(), cmd.to_owned());
    tokio::task::spawn_blocking(move || -> Result<()> {
        let _ = std::fs::remove_dir_all(&dir2);
        std::fs::create_dir_all(&dir2)?;
        let archive = dir2.join(file_name);
        std::fs::write(&archive, &bytes)?;
        extract(&archive, &dir2, &cmd)?;
        std::fs::write(dir2.join(".installed"), "")?;
        remove_other_versions(&dir2);
        Ok(())
    })
    .await?
    .with_context(|| format!("installing {id}"))?;
    Ok(dir)
}

/// Registry builds update often; keep only the version just installed.
fn remove_other_versions(installed: &Path) {
    let Some(parent) = installed.parent() else { return };
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p != installed && p.is_dir() {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

/// Blocking: unpacks `archive` into `dir` and makes `cmd` executable.
pub(crate) fn extract(archive: &Path, dir: &Path, cmd: &Path) -> Result<()> {
    let name = archive.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    // Multi-part extensions (`.tar.gz`) rule out `Path::extension`; `name` is already lowercased.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    // Windows' bsdtar (`tar.exe`) reads zips too; it has no `unzip`.
    let status = if name.ends_with(".zip") && cfg!(unix) {
        std::process::Command::new("unzip").arg("-q").arg("-o").arg(archive).arg("-d").arg(dir).status()
    } else if [".zip", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".tar"].iter().any(|e| name.ends_with(e)) {
        // Both GNU tar and bsdtar refuse `..` members by default.
        std::process::Command::new("tar").arg("-xf").arg(archive).arg("-C").arg(dir).status()
    } else {
        // Raw binary: it *is* the command.
        let exe = dir.join(cmd);
        if let Some(parent) = exe.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(archive, &exe)?;
        #[cfg(unix)]
        chmod_x(&exe)?;
        return Ok(());
    }
    .context("running the extractor")?;
    if !status.success() {
        bail!("couldn't extract {}", archive.display());
    }
    let _ = std::fs::remove_file(archive);
    #[cfg(unix)]
    if dir.join(cmd).exists() {
        chmod_x(&dir.join(cmd))?;
    }
    Ok(())
}

#[cfg(unix)]
fn chmod_x(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(path)?.permissions();
    p.set_mode(p.mode() | 0o755);
    std::fs::set_permissions(path, p)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn npx_command_is_quoted() {
        let npx = json!({"package": "@s/x@1.0.0", "args": ["--acp", "a b"], "env": {"K": "v", "BAD;rm": "x"}});
        let expected =
            if cfg!(windows) { "K=v npx -y @s/x@1.0.0 --acp \"a b\"" } else { "env K=v npx -y @s/x@1.0.0 --acp 'a b'" };
        assert_eq!(npx_cmd(&npx).acp(), expected);
    }

    #[test]
    fn npx_hash_matches_npm() {
        // ~/.npm/_npx/317ff93dc6c5b519 after `npx -y @agentclientprotocol/codex-acp@2.0.0`.
        assert_eq!(npx_hash("@agentclientprotocol/codex-acp@2.0.0"), "317ff93dc6c5b519");
    }

    #[test]
    fn keeps_only_the_installed_version() {
        let root = std::env::temp_dir().join(format!("codync-ver-{}", uuid::Uuid::new_v4()));
        for v in ["1.0.0", "1.1.0"] {
            std::fs::create_dir_all(root.join(v)).unwrap();
        }
        remove_other_versions(&root.join("1.1.0"));
        assert!(!root.join("1.0.0").exists());
        assert!(root.join("1.1.0").exists());
    }

    #[cfg(unix)]
    #[test]
    fn raw_binary_extract_marks_executable() {
        let dir = std::env::temp_dir().join(format!("codync-reg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("tool-darwin");
        std::fs::write(&archive, "#!/bin/sh\necho hi\n").unwrap();
        extract(&archive, &dir, Path::new("./bin/tool")).unwrap();
        let out = std::process::Command::new(dir.join("bin/tool")).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
    }

    #[test]
    fn registry_paths_cannot_escape_the_agents_dir() {
        assert_eq!(safe_component("cursor"), Some("cursor"));
        assert_eq!(safe_component("1.0.3-beta"), Some("1.0.3-beta"));
        for bad in ["", ".", "..", "a/b", "../x", "x y"] {
            assert_eq!(safe_component(bad), None, "{bad:?}");
        }
        assert!(contained("./dist-package/cursor-agent").is_some());
        assert!(contained("../../bin/sh").is_none());
        assert!(contained("/bin/sh").is_none());
    }
}
