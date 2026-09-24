//! The official ACP agent registry (https://agentclientprotocol.com/registry):
//! ~40 coding agents with a known-good way to launch each over ACP — an npx or
//! uvx package, or a per-platform binary archive we download on first use.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const URL: &str = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

static CACHE: Mutex<Option<Value>> = Mutex::new(None);

fn cache_file() -> PathBuf {
    crate::service::data_dir().join("registry.json")
}

/// Registry agents (from memory, else the on-disk copy).
pub fn agents() -> Vec<Value> {
    let mut cache = CACHE.lock().unwrap();
    if cache.is_none() {
        *cache = std::fs::read_to_string(cache_file()).ok().and_then(|s| serde_json::from_str(&s).ok());
    }
    cache.as_ref().and_then(|v| v["agents"].as_array().cloned()).unwrap_or_default()
}

pub fn agent(id: &str) -> Option<Value> {
    agents().into_iter().find(|a| a["id"] == id)
}

/// Refreshes the registry once a day (it changes hourly upstream; daily is plenty).
pub async fn refresh_loop() {
    loop {
        match fetch().await {
            Ok(v) => {
                let _ = std::fs::write(cache_file(), v.to_string());
                *CACHE.lock().unwrap() = Some(v);
            }
            Err(e) => tracing::info!("ACP registry refresh failed (using cache): {e}"),
        }
        tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
    }
}

async fn fetch() -> Result<Value> {
    let v: Value = reqwest::Client::new()
        .get(URL)
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
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

/// How this machine would launch the agent: `npx` / `uvx` / `download`, or None.
pub fn launch_kind(agent: &Value) -> Option<&'static str> {
    let d = &agent["distribution"];
    let on_path = crate::backends::on_path;
    if let Some(p) = platform()
        && d["binary"][p].is_object()
    {
        return Some("download");
    }
    if d["npx"].is_object() && on_path("npx") {
        return Some("npx");
    }
    if d["uvx"].is_object() && on_path("uvx") {
        return Some("uvx");
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

fn env_prefix(v: &Value) -> String {
    v["env"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some(format!("{k}={} ", shell_quote(v.as_str()?))))
                .collect::<String>()
        })
        .unwrap_or_default()
}

/// Shell command that starts the agent over ACP stdio, downloading it first if needed.
/// `progress` receives short status lines ("Downloading Cursor…").
pub async fn command(agent: &Value, progress: impl Fn(&str)) -> Result<String> {
    let name = agent["name"].as_str().unwrap_or("agent");
    let d = &agent["distribution"];
    match launch_kind(agent) {
        Some("download") => {
            let target = &d["binary"][platform().unwrap()];
            let dir = install_binary(agent, target, &progress).await?;
            let cmd = target["cmd"].as_str().ok_or_else(|| anyhow!("registry entry has no cmd"))?;
            let exe = dir.join(cmd.trim_start_matches("./"));
            Ok(format!("{}{} {}", env_prefix(target), shell_quote(&exe.to_string_lossy()), args_of(target)))
        }
        Some("npx") => Ok(format!(
            "{}npx -y {} {}",
            env_prefix(&d["npx"]),
            shell_quote(d["npx"]["package"].as_str().unwrap_or_default()),
            args_of(&d["npx"])
        )),
        Some("uvx") => Ok(format!(
            "{}uvx {} {}",
            env_prefix(&d["uvx"]),
            shell_quote(d["uvx"]["package"].as_str().unwrap_or_default()),
            args_of(&d["uvx"])
        )),
        _ => bail!("{name} has no build for this computer (needs {}).", if d["uvx"].is_object() { "uv" } else { "Node.js" }),
    }
}

/// Downloads + extracts a binary distribution into ~/.codync/agents/<id>/<version>.
async fn install_binary(agent: &Value, target: &Value, progress: &impl Fn(&str)) -> Result<PathBuf> {
    let id = agent["id"].as_str().unwrap_or("agent");
    let version = agent["version"].as_str().unwrap_or("latest");
    let dir = crate::service::data_dir().join("agents").join(id).join(version);
    if dir.join(".installed").exists() {
        return Ok(dir);
    }
    let url = target["archive"].as_str().ok_or_else(|| anyhow!("registry entry has no archive"))?;
    progress(&format!("Downloading {}…", agent["name"].as_str().unwrap_or(id)));
    let bytes = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(600))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if let Some(want) = target["sha256"].as_str() {
        let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if !got.eq_ignore_ascii_case(want) {
            bail!("download checksum mismatch for {id}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let file_name = url.rsplit('/').next().unwrap_or("download").split('?').next().unwrap_or("download");
    let archive = dir.join(file_name);
    std::fs::write(&archive, &bytes)?;
    progress("Installing…");
    extract(&archive, &dir, target["cmd"].as_str().unwrap_or_default())?;
    std::fs::write(dir.join(".installed"), "")?;
    Ok(dir)
}

fn extract(archive: &Path, dir: &Path, cmd: &str) -> Result<()> {
    let name = archive.file_name().unwrap().to_string_lossy().to_lowercase();
    let status = if name.ends_with(".zip") {
        std::process::Command::new("unzip").arg("-q").arg("-o").arg(archive).arg("-d").arg(dir).status()
    } else if [".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".tar"].iter().any(|e| name.ends_with(e)) {
        std::process::Command::new("tar").arg("-xf").arg(archive).arg("-C").arg(dir).status()
    } else {
        // Raw binary: it *is* the command.
        let exe = dir.join(cmd.trim_start_matches("./"));
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
    let exe = dir.join(cmd.trim_start_matches("./"));
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
        let a = json!({"id": "x", "name": "X", "distribution": {"npx": {"package": "@s/x@1.0.0", "args": ["--acp", "a b"], "env": {"K": "v"}}}});
        if crate::backends::on_path("npx") {
            assert_eq!(command(&a, |_| {}).await.unwrap(), "K=v npx -y @s/x@1.0.0 --acp 'a b'");
        }
    }

    #[test]
    fn raw_binary_extract_marks_executable() {
        let dir = std::env::temp_dir().join(format!("codync-reg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("tool-darwin");
        std::fs::write(&archive, "#!/bin/sh\necho hi\n").unwrap();
        extract(&archive, &dir, "./bin/tool").unwrap();
        let out = std::process::Command::new(dir.join("bin/tool")).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
    }
}
