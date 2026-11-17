//! The keeper updates itself when the host asks (`update {version}`): its binary lives in its own state folder,
//! which only the keeper's user may write, so nothing else can. Only a newer, signed release is installed, checked
//! as the host's own updater checks one; then the keeper exits and systemd starts the new binary.

use crate::update::release;
use anyhow::{Context, Result, ensure};

/// Installs release `version` over the running binary; refused for a version not newer than this one, a
/// development build, or a binary in a folder the keeper can't write.
pub async fn install(version: &str) -> Result<()> {
    crate::environment::Environment::current().require_release_updates()?;
    let wanted = semver::Version::parse(version).context("not a release version")?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
    ensure!(wanted > current, "the keeper already runs {current}");
    let target = std::env::current_exe()?;
    let dir = target.parent().context("the keeper's binary has no folder")?.to_owned();
    let probe = dir.join(format!(".codync-keeper-probe-{}", uuid::Uuid::new_v4()));
    let writable = tokio::task::spawn_blocking(move || {
        let made = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
        let _ = std::fs::remove_file(&probe);
        made
    })
    .await?;
    ensure!(writable, "the keeper can't replace its binary in {}", dir.display());
    let release = release::tagged(&wanted).await?;
    let bytes = release::archive(&release).await?;
    tokio::task::spawn_blocking(move || {
        let stage = dir.join(format!(".codync-host-update-{}", uuid::Uuid::new_v4()));
        let installed = release::extract(&bytes, &release.platform, &stage)
            .and_then(|()| std::fs::rename(&stage, &target).context("replacing the keeper's binary"));
        if installed.is_err() {
            let _ = std::fs::remove_file(&stage);
        }
        installed
    })
    .await?
}
