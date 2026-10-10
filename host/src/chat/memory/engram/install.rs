//! Install one checksum-pinned binary, shared by every bot on this host.

use super::VERSION;
use crate::LockExt;
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

const RELEASES: &str = "https://github.com/Gentleman-Programming/engram/releases/download";
const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// When the last download failed, and how long until another is tried.
static FAILED_AT: Mutex<Option<Instant>> = Mutex::new(None);
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

pub(super) async fn binary() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(binary) = std::env::var_os("CODYNC_TEST_ENGRAM") {
        return Ok(binary.into());
    }
    let _guard = INSTALL.lock().await;
    let root = crate::service::data_dir().join("engram").join(VERSION);
    let name = if cfg!(windows) { "engram.exe" } else { "engram" };
    let binary = root.join(name);
    if tokio::fs::try_exists(root.join(".installed")).await? && tokio::fs::try_exists(&binary).await? {
        return Ok(binary);
    }
    // Every turn asks for memory: after a failed download (offline), don't make each one wait on another.
    if FAILED_AT.locked().is_some_and(|at| at.elapsed() < RETRY_AFTER) {
        bail!("Engram couldn't be downloaded; trying again in a few minutes");
    }
    let installed = install(root, name).await;
    if installed.is_err() {
        *FAILED_AT.locked() = Some(Instant::now());
    }
    installed.map(|()| binary)
}

async fn install(root: PathBuf, name: &str) -> Result<()> {
    let (platform, checksum) = distribution(std::env::consts::OS, std::env::consts::ARCH)?;
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive = format!("engram_{VERSION}_{platform}.{extension}");
    let mut response = crate::http()
        .get(format!("{RELEASES}/v{VERSION}/{archive}"))
        .timeout(Duration::from_secs(180))
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(bytes.len() + chunk.len() <= MAX_ARCHIVE_BYTES, "Engram download exceeds size limit");
        bytes.extend_from_slice(&chunk);
    }
    verify(&bytes, &checksum)?;
    let staging = root.with_extension(format!("staging-{}", uuid::Uuid::new_v4()));
    let (target, stage, executable) = (root, staging.clone(), name.to_owned());
    let result = tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&stage)?;
        let file = stage.join(&archive);
        std::fs::write(&file, bytes)?;
        crate::agent::registry::extract(&file, &stage, std::path::Path::new(&executable))?;
        let output = std::process::Command::new(stage.join(&executable)).arg("version").output()?;
        ensure!(output.status.success(), "Engram version check failed");
        let version = String::from_utf8(output.stdout)?;
        ensure!(
            version.split_whitespace().any(|v| v.trim_start_matches('v') == VERSION),
            "unexpected Engram version: {version}"
        );
        std::fs::write(stage.join(".installed"), VERSION)?;
        // A previous incomplete installation has never been used by a process.
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&stage, &target)?;
        Ok(())
    })
    .await?;
    if result.is_err()
        && let Err(error) = tokio::fs::remove_dir_all(&staging).await
    {
        tracing::warn!(%error, "could not remove incomplete Engram installation");
    }
    result.context("installing Engram")
}

fn verify(bytes: &[u8], expected: &str) -> Result<()> {
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(hash, "{byte:02x}");
    }
    ensure!(hash == expected, "Engram download checksum mismatch");
    Ok(())
}

fn distribution(os: &str, arch: &str) -> Result<(String, String)> {
    let os = match os {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => bail!("Engram does not support {os}"),
    };
    let arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => bail!("Engram does not support {arch}"),
    };
    let target = format!("{os}_{arch}");
    let release: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../packaging/engram/release.json"))?;
    ensure!(release["version"] == VERSION, "Engram release manifest version mismatch");
    let checksum = release["assets"][&target].as_str().context("missing Engram release checksum")?.to_owned();
    Ok((target, checksum))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_corrupt_release_and_unsupported_platform() {
        assert!(verify(b"corrupt", &distribution("macos", "aarch64").unwrap().1).is_err());
        assert!(distribution("linux", "riscv64").is_err());
        for os in ["macos", "linux", "windows"] {
            for arch in ["x86_64", "aarch64"] {
                assert_eq!(distribution(os, arch).unwrap().1.len(), 64);
            }
        }
    }
}
