//! The official ACP agent registry (<https://agentclientprotocol.com/registry>):
//! ~40 coding agents with a known-good way to launch each over ACP — an npx or
//! uvx package, or a per-platform binary archive we download on first use.
//!
//! Registry JSON is untrusted input: ids, versions and commands become paths on
//! disk, so they're checked before use.

use crate::LockExt;
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
        _ => return None,
    })
}

pub fn launch_kind(agent: &Value) -> Option<Launch> {
    let d = &agent["distribution"];
    let on_path = crate::backends::on_path;
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

fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=@:+,".contains(c)) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn args_of(v: &Value) -> String {
    v["args"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).map(shell_quote).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// `KEY=value ` pairs; keys that aren't plain identifiers are dropped (they'd be shell syntax).
fn env_prefix(v: &Value) -> String {
    v["env"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .filter_map(|(k, v)| Some(format!("{k}={} ", shell_quote(v.as_str()?))))
                .collect::<String>()
        })
        .unwrap_or_default()
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

/// Shell command that starts the agent over ACP stdio, downloading it first if needed.
/// `progress` receives short status lines ("Downloading Cursor…").
pub async fn command(agent: &Value, progress: impl Fn(&str)) -> Result<String> {
    let name = agent["name"].as_str().unwrap_or("agent");
    let d = &agent["distribution"];
    match launch_kind(agent) {
        Some(Launch::Download) => {
            let platform = platform().ok_or_else(|| anyhow!("no download for this platform"))?;
            let target = &d["binary"][platform];
            let cmd =
                target["cmd"].as_str().and_then(contained).ok_or_else(|| anyhow!("registry entry has a bad cmd"))?;
            let dir = install_binary(agent, target, cmd, &progress).await?;
            Ok(format!("{}{} {}", env_prefix(target), shell_quote(&dir.join(cmd).to_string_lossy()), args_of(target)))
        }
        Some(Launch::Npx) => Ok(format!(
            "{}npx -y {} {}",
            env_prefix(&d["npx"]),
            shell_quote(d["npx"]["package"].as_str().unwrap_or_default()),
            args_of(&d["npx"])
        )),
        Some(Launch::Uvx) => Ok(format!(
            "{}uvx {} {}",
            env_prefix(&d["uvx"]),
            shell_quote(d["uvx"]["package"].as_str().unwrap_or_default()),
            args_of(&d["uvx"])
        )),
        None => bail!(
            "{name} has no build for this computer (needs {})",
            if d["uvx"].is_object() { "uv" } else { "Node.js" }
        ),
    }
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
fn extract(archive: &Path, dir: &Path, cmd: &Path) -> Result<()> {
    let name = archive.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    // Multi-part extensions (`.tar.gz`) rule out `Path::extension`; `name` is already lowercased.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    let status = if name.ends_with(".zip") {
        std::process::Command::new("unzip").arg("-q").arg("-o").arg(archive).arg("-d").arg(dir).status()
    } else if [".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".tar"].iter().any(|e| name.ends_with(e)) {
        // Both GNU tar and bsdtar refuse `..` members by default.
        std::process::Command::new("tar").arg("-xf").arg(archive).arg("-C").arg(dir).status()
    } else {
        // Raw binary: it *is* the command.
        let exe = dir.join(cmd);
        if let Some(parent) = exe.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(archive, &exe)?;
        chmod_x(&exe)?;
        return Ok(());
    }
    .context("running the extractor")?;
    if !status.success() {
        bail!("couldn't extract {}", archive.display());
    }
    let _ = std::fs::remove_file(archive);
    let exe = dir.join(cmd);
    if exe.exists() {
        chmod_x(&exe)?;
    }
    Ok(())
}

fn chmod_x(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(path)?.permissions();
        p.set_mode(p.mode() | 0o755);
        std::fs::set_permissions(path, p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn npx_command_is_quoted() {
        let a = json!({"id": "x", "name": "X", "distribution": {"npx": {"package": "@s/x@1.0.0", "args": ["--acp", "a b"], "env": {"K": "v", "BAD;rm": "x"}}}});
        if crate::backends::on_path("npx") {
            assert_eq!(command(&a, |_| {}).await.unwrap(), "K=v npx -y @s/x@1.0.0 --acp 'a b'");
        }
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
