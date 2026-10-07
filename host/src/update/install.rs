use super::release;
use crate::service;
use anyhow::{Context, Result, bail, ensure};
use std::{future::Future, path::Path, time::Duration};

#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    Standalone,
    AppBundle,
    Homebrew,
    Development,
}

pub fn method(executable: &Path) -> Method {
    let path = executable.to_string_lossy();
    // The Mac app's bundle, or the Windows app's `resources` folder.
    if path.contains(".app/Contents/") || (cfg!(windows) && path.contains("\\resources\\")) {
        Method::AppBundle
    } else if path.contains("/Cellar/") || path.contains("/linuxbrew/") {
        Method::Homebrew
    } else if path.contains("/target/debug/") || path.contains("/target/release/") || cfg!(debug_assertions) {
        Method::Development
    } else {
        Method::Standalone
    }
}

pub fn require_standalone(executable: &Path) -> Result<()> {
    match method(executable) {
        Method::Standalone => Ok(()),
        Method::AppBundle => bail!("this host belongs to the Codync app; update the app from Settings > Updates"),
        Method::Homebrew => bail!(
            "this host belongs to Homebrew; run `brew upgrade leepokai/codync/codync-host`, then `codync-host install`"
        ),
        Method::Development => bail!("this is a development build; rebuild it instead of replacing it with a release"),
    }
}

pub async fn health(port: u16) -> Option<serde_json::Value> {
    crate::http()
        .get(format!("http://127.0.0.1:{port}/health"))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

async fn verify_running(port: u16, version: &str, hash: &str, identity: Option<&str>) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(health) = health(port).await
            && health["version"] == version
            && health["binaryHash"] == hash
            && identity.is_none_or(|id| health["computerId"] == id)
        {
            return Ok(());
        }
        ensure!(tokio::time::Instant::now() < deadline, "the updated host did not become healthy within 30 seconds");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The backup lives beside the executable, so replacement and rollback use
/// atomic rename on the same filesystem. It is retained if rollback fails.
pub async fn replace<Stop, Stopped, Verify, Verified, Restore, Restored>(
    target: &Path,
    candidate: &Path,
    stop: Stop,
    verify: Verify,
    restore: Restore,
) -> Result<()>
where
    Stop: FnOnce() -> Stopped,
    Stopped: Future<Output = Result<()>>,
    Verify: FnOnce() -> Verified,
    Verified: Future<Output = Result<()>>,
    Restore: FnOnce() -> Restored,
    Restored: Future<Output = Result<()>>,
{
    let backup = target.with_file_name(format!(".codync-host-backup-{}", uuid::Uuid::new_v4()));
    std::fs::hard_link(target, &backup).context("creating the rollback copy")?;
    if let Err(error) = stop().await {
        let _ = std::fs::remove_file(&backup);
        return Err(error.context("old host could not be stopped; executable was not replaced"));
    }
    let update = async {
        std::fs::rename(candidate, target).context("replacing the host executable")?;
        verify().await
    }
    .await;
    if let Err(error) = update {
        // Renaming a live executable is safe on Unix; processes retain their
        // original inode. restore() then stops the failed service and restarts it.
        std::fs::rename(&backup, target)
            .with_context(|| format!("update failed ({error:#}); restore the saved binary at {}", backup.display()))?;
        restore()
            .await
            .with_context(|| format!("update failed ({error:#}); restored the old binary but could not restart it"))?;
        bail!("update failed; restored the previous host: {error:#}");
    }
    std::fs::remove_file(backup)?;
    Ok(())
}

/// Whether iPhones use this host: asked of the running host, assumed when it can't answer.
async fn iphones_paired(port: u16) -> bool {
    let devices = async {
        let token = std::fs::read_to_string(service::data_dir().join("token")).ok()?;
        let res: serde_json::Value = crate::http()
            .post(format!("http://127.0.0.1:{port}/api/devices"))
            .bearer_auth(token.trim())
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .await
            .ok()?;
        Some(res["devices"].as_array()?.iter().any(|d| d["platform"] == "ios"))
    };
    devices.await.unwrap_or(true)
}

/// The `minApp` a downloaded (verified) executable declares.
async fn staged_min_app(stage: &Path) -> Result<String> {
    let output = tokio::process::Command::new(stage).arg("compat").kill_on_drop(true).output();
    let output = tokio::time::timeout(Duration::from_secs(10), output).await??;
    ensure!(output.status.success(), "the downloaded host couldn't report the iPhone app it needs");
    let compat: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    compat["minApp"].as_str().map(str::to_owned).context("the downloaded host reported no minimum app version")
}

/// Whether a release needing `new_min_app` waits for the App Store (only if it raises the floor).
async fn app_store_gate(new_min_app: &str, port: u16) -> crate::compat::Gate {
    if !crate::compat::below(crate::compat::MIN_APP, new_min_app) {
        return crate::compat::Gate::Proceed;
    }
    let iphones = iphones_paired(port).await;
    let store = if iphones { crate::compat::app_store_version().await } else { Ok(None) };
    crate::compat::gate(crate::compat::MIN_APP, new_min_app, iphones, &store)
}

pub async fn apply(
    release: &release::Release,
    target: &Path,
    port: u16,
    force: bool,
    skip_app_check: bool,
) -> Result<crate::compat::Gate> {
    require_standalone(target)?;
    // The release's signed compat file answers before anything is downloaded; releases
    // without one are asked after download (below).
    let early_min_app = if skip_app_check { None } else { release::min_app(release).await.ok() };
    if let Some(min_app) = &early_min_app {
        let gate = app_store_gate(min_app, port).await;
        if gate != crate::compat::Gate::Proceed {
            return Ok(gate);
        }
    }
    let managed = service::installed();
    if managed {
        service::require_executable(target)?;
    }
    let old_health = health(port).await;
    if !force {
        ensure!(
            old_health.as_ref().is_none_or(|h| h["busy"] == false),
            "host is busy or its activity is unknown; retry when idle, or use --force"
        );
    }
    if !managed {
        drop(service::lock_host().context("stop the manually started host before updating its executable")?);
    }
    let bytes = release::archive(release).await?;
    let stage = target.with_file_name(format!(".codync-host-update-{}", uuid::Uuid::new_v4()));
    let helpers: Vec<Helper> = HELPERS.iter().map(|name| Helper::new(target, name)).collect();
    let result = async {
        release::extract(&bytes, &release.platform, &stage)?;
        let mut shipped = vec![];
        for helper in &helpers {
            if release::extract_file(&bytes, &release.platform, helper.name, &helper.stage)? {
                shipped.push(helper);
            }
        }
        let output = tokio::process::Command::new(&stage).arg("--version").kill_on_drop(true).output();
        let output = tokio::time::timeout(Duration::from_secs(10), output).await??;
        ensure!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == format!("codync-host {}", release.version),
            "downloaded executable reported the wrong version"
        );
        // Before anything stops: a release that needs a newer iPhone app than the App Store
        // has waits, so paired iPhones aren't asked for an update they can't get yet.
        if !skip_app_check && early_min_app.is_none() {
            let gate = app_store_gate(&staged_min_app(&stage).await?, port).await;
            if gate != crate::compat::Gate::Proceed {
                return Ok(gate);
            }
        }
        let old_hash = release::sha256(&std::fs::read(target)?);
        let new_hash = release::sha256(&std::fs::read(&stage)?);
        let identity = old_health.as_ref().and_then(|h| h["computerId"].as_str());
        // Recheck immediately before stopping; downloads can take minutes.
        if !force && managed {
            let current = health(port).await.context("cannot confirm the running host is idle")?;
            ensure!(current["busy"] == false, "host became busy while downloading; retry after its work finishes");
        }
        replace(
            target,
            &stage,
            || async {
                if managed {
                    tokio::task::spawn_blocking(service::stop).await??;
                }
                // Swapped while the host is stopped, so the new host never starts an old helper.
                // A failed swap keeps the old helper rather than leaving the host stopped.
                for helper in &shipped {
                    if let Err(error) = helper.swap() {
                        tracing::warn!(helper = helper.name, error = format!("{error:#}"), "helper not updated");
                    }
                }
                Ok(())
            },
            || async {
                if managed {
                    tokio::task::spawn_blocking(service::start).await??;
                    verify_running(port, &release.version, &new_hash, identity).await?;
                }
                Ok(())
            },
            || async {
                for helper in &helpers {
                    helper.restore();
                }
                if managed {
                    tokio::task::spawn_blocking(service::stop).await??;
                    tokio::task::spawn_blocking(service::start).await??;
                    verify_running(port, env!("CARGO_PKG_VERSION"), &old_hash, identity).await?;
                }
                Ok(())
            },
        )
        .await?;
        Ok(crate::compat::Gate::Proceed)
    }
    .await;
    let _ = std::fs::remove_file(stage);
    for helper in &helpers {
        helper.finish();
    }
    result
}

/// Linux helpers in the host's archive: the Remote screen helper and the computer-use driver.
/// The host starts them from beside itself, so an update replaces them there too.
const HELPERS: &[&str] = &["codync-screen", "cua-driver"];

/// Swaps a helper beside the host for the staged one, keeping a copy to roll back to.
struct Helper {
    name: &'static str,
    path: std::path::PathBuf,
    stage: std::path::PathBuf,
    backup: std::path::PathBuf,
    swapped: std::sync::atomic::AtomicBool,
}

impl Helper {
    fn new(target: &Path, name: &'static str) -> Self {
        let id = uuid::Uuid::new_v4();
        Self {
            name,
            path: target.with_file_name(name),
            stage: target.with_file_name(format!(".{name}-update-{id}")),
            backup: target.with_file_name(format!(".{name}-backup-{id}")),
            swapped: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn swap(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::hard_link(&self.path, &self.backup).with_context(|| format!("saving {}", self.name))?;
        }
        std::fs::rename(&self.stage, &self.path).with_context(|| format!("replacing {}", self.name))?;
        self.swapped.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Puts the previous helper back (or removes a new one that had none before).
    fn restore(&self) {
        if !self.swapped.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if self.backup.exists() {
            let _ = std::fs::rename(&self.backup, &self.path);
        } else {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn finish(&self) {
        let _ = std::fs::remove_file(&self.stage);
        let _ = std::fs::remove_file(&self.backup);
    }
}
