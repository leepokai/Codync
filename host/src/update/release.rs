//! Signed release manifests pin platform, version, URL, size and SHA-256.
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, io::Read, path::Path, sync::LazyLock, time::Duration};

const GITHUB_RELEASES: &str = "https://github.com/leepokai/Codync/releases";
pub const MAX_ARCHIVE: usize = 128 * 1024 * 1024;
const MAX_UNPACKED: u64 = 256 * 1024 * 1024;
const PUBLIC_KEY: &str = include_str!("../../../packaging/updates/host-public-key.txt");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub platform: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Where releases come from: GitHub's, or the channel in `CODYNC_RELEASES_URL` (dev cloud computers use the dev
/// channel: docs/guides/development.md#dev-release-channel). Every channel has GitHub's layout:
/// `<base>/latest/download/<asset>` and `<base>/download/v<version>/<asset>`.
pub fn releases() -> &'static str {
    static BASE: LazyLock<String> = LazyLock::new(|| match std::env::var("CODYNC_RELEASES_URL") {
        Ok(url) if url.starts_with("https://") => url.trim_end_matches('/').to_owned(),
        Ok(url) => {
            tracing::warn!(url, "CODYNC_RELEASES_URL must be https; using GitHub's releases");
            GITHUB_RELEASES.to_owned()
        }
        Err(_) => GITHUB_RELEASES.to_owned(),
    });
    &BASE
}

pub fn platform() -> Result<String> {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        other => bail!("host updates do not support {other}"),
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        other => bail!("host updates do not support {other}"),
    };
    Ok(format!("{os}-{arch}"))
}

pub fn sha256(bytes: &[u8]) -> String {
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(hash, "{byte:02x}");
    }
    hash
}

/// A manifest signed for `platform` whose archive is served from the channel `base` (`releases()`).
pub fn verify_manifest(bytes: &[u8], signature: &str, public_key: &str, platform: &str, base: &str) -> Result<Release> {
    let key: [u8; 32] =
        B64.decode(public_key.trim())?.try_into().map_err(|_| anyhow::anyhow!("invalid update public key"))?;
    let signature = Signature::from_slice(&B64.decode(signature.trim())?)?;
    VerifyingKey::from_bytes(&key)?.verify_strict(bytes, &signature).context("release signature is invalid")?;
    let release: Release = serde_json::from_slice(bytes).context("reading the signed release manifest")?;
    let version = semver::Version::parse(&release.version).context("invalid release version")?;
    ensure!(version.pre.is_empty() && version.build.is_empty(), "only stable host releases are supported");
    ensure!(release.platform == platform, "release targets another platform");
    let expected = format!("{base}/download/v{version}/codync-host-{platform}.tar.gz");
    ensure!(release.url == expected, "release download URL does not match its version and platform");
    ensure!(release.size > 0 && release.size <= MAX_ARCHIVE as u64, "invalid release archive size");
    ensure!(
        release.sha256.len() == 64 && release.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid release checksum"
    );
    Ok(release)
}

pub async fn download(url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut response = crate::http()
        .get(url)
        .header(reqwest::header::USER_AGENT, concat!("codync-host/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_mins(3))
        .send()
        .await?
        .error_for_status()?;
    ensure!(
        response.content_length().is_none_or(|size| size <= limit as u64),
        "update download exceeds the size limit"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(bytes.len().saturating_add(chunk.len()) <= limit, "update download exceeds the size limit");
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn latest() -> Result<Release> {
    // Only the version from the latest manifest, then that version's own files: fetching manifest and signature
    // through separate latest redirects could straddle two releases.
    let url = format!("{}/latest/download/codync-host-{}.update.json", releases(), platform()?);
    let latest: Release =
        serde_json::from_slice(&download(&url, 16 * 1024).await?).context("reading the latest release manifest")?;
    tagged(&semver::Version::parse(&latest.version)?).await
}

/// The signed release `version`, whether or not it has the *latest* mark yet (a cloud computer catching up to
/// the version an app needs).
pub async fn tagged(version: &semver::Version) -> Result<Release> {
    let platform = platform()?;
    let url = format!("{}/download/v{version}/codync-host-{platform}.update.json", releases());
    let bytes = download(&url, 16 * 1024).await.context("this release has no valid signed host update manifest")?;
    let signature = download(&format!("{url}.sig"), 1024).await?;
    let release = verify_manifest(&bytes, std::str::from_utf8(&signature)?, PUBLIC_KEY, &platform, releases())?;
    ensure!(release.version == version.to_string(), "manifest version differs from the selected release");
    Ok(release)
}

pub async fn archive(release: &Release) -> Result<Vec<u8>> {
    let bytes = download(&release.url, MAX_ARCHIVE).await?;
    ensure!(bytes.len() as u64 == release.size, "update archive size differs from signed manifest");
    ensure!(
        sha256(&bytes).eq_ignore_ascii_case(&release.sha256),
        "update archive checksum differs from signed manifest"
    );
    Ok(bytes)
}

/// Extract just the expected regular executable. Never unpack archive paths,
/// symlinks, permissions or other files into the user's installation.
pub fn extract(bytes: &[u8], platform: &str, destination: &Path) -> Result<()> {
    ensure!(
        extract_file(bytes, platform, "codync-host", destination)?,
        "release archive does not contain its host executable"
    );
    Ok(())
}

/// Extracts one executable shipped beside the host (`codync-screen` on Linux), with the same
/// rules as [`extract`]. `false` when the archive doesn't carry it.
pub fn extract_file(bytes: &[u8], platform: &str, name: &str, destination: &Path) -> Result<bool> {
    let decoder = flate2::read::GzDecoder::new(bytes).take(MAX_UNPACKED);
    let mut archive = tar::Archive::new(decoder);
    let expected = format!("codync-host-{platform}/{name}");
    let mut found = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.as_ref() != Path::new(&expected) {
            continue;
        }
        ensure!(!found && entry.header().entry_type().is_file(), "update executable must be a single regular file");
        ensure!(entry.size() > 0 && entry.size() < MAX_UNPACKED, "invalid update executable size");
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(destination)?;
        std::io::copy(&mut entry, &mut file)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(std::fs::Permissions::from_mode(0o755))?;
        }
        file.sync_all()?;
        found = true;
    }
    Ok(found)
}
