//! A bot's artifact (`docs/features/artifacts.md`): its HTML read from the host (`readArtifact`), then saved to
//! Downloads or opened in this computer's browser. Progress and the outcome come as `Msg::FileDownload`.

use super::Client;
use super::files::{Download, Partial, decode_chunk, downloads, place};
use crate::chat::artifacts::MAX_ARTIFACT;
use crate::chat::files::digest_hex;
use crate::tui::app::Msg;
use anyhow::{Result, anyhow, ensure};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt as _;
use tokio::sync::mpsc::UnboundedSender;

/// Where an artifact's HTML goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArtifactTo {
    Downloads,
    Browser,
}

impl Client {
    pub fn fetch_artifact(&self, bot: String, artifact: String, to: ArtifactTo, tx: UnboundedSender<Msg>) -> Download {
        let (client, progress) = (self.clone(), tx.clone());
        Download::spawn(tx, move |id| async move { deliver(&client, &bot, &artifact, to, &id, &progress).await })
    }
}

async fn deliver(
    client: &Client,
    bot: &str,
    artifact: &str,
    to: ArtifactTo,
    id: &str,
    tx: &UnboundedSender<Msg>,
) -> Result<String> {
    let (title, html) = read(client, bot, artifact, id, tx).await?;
    let name = file_name(&title);
    match to {
        ArtifactTo::Downloads => {
            let dir = downloads()?;
            let partial = Partial(dir.join(format!(".codync-{id}.partial")));
            let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&partial.0).await?;
            file.write_all(&html).await?;
            file.sync_all().await?;
            drop(file);
            Ok(format!("Saved {}", place(&partial, &dir, &name).await?.display()))
        }
        ArtifactTo::Browser => {
            let temp = std::env::temp_dir().join("codync-artifacts");
            let id = id.to_owned();
            let (path, opened) = tokio::task::spawn_blocking(move || -> Result<(PathBuf, bool)> {
                let path = write_temp(&temp, &name, &id, &html)?;
                let opened = crate::shell::open(path.as_os_str());
                Ok((path, opened))
            })
            .await??;
            Ok(if opened {
                format!("Opened {title} in the browser")
            } else {
                format!("No browser to open it: saved {}", path.display())
            })
        }
    }
}

/// The artifact's title and whole HTML, checked against its size and SHA-256.
async fn read(
    client: &Client,
    bot: &str,
    artifact: &str,
    id: &str,
    tx: &UnboundedSender<Msg>,
) -> Result<(String, Vec<u8>)> {
    let mut html: Vec<u8> = Vec::new();
    // The size and hash of the version being read: a publish meanwhile changes them.
    let mut version: Option<(u64, String)> = None;
    loop {
        let offset = html.len() as u64;
        let body = json!({"botId": bot, "artifactId": artifact, "offset": offset});
        let response = client.call("readArtifact", &body).await.map_err(|e| anyhow!(e))?;
        let size = response["size"].as_u64().ok_or_else(|| anyhow!("invalid artifact"))?;
        let sha = response["artifact"]["sha256"].as_str().unwrap_or_default().to_owned();
        ensure!(size <= MAX_ARTIFACT && sha.len() == 64, "invalid artifact");
        let (want_size, want_sha) = version.get_or_insert_with(|| (size, sha.clone()));
        ensure!(*want_size == size && *want_sha == sha, "the artifact changed while loading; try again");
        html.extend_from_slice(&decode_chunk(&response, size, offset)?);
        let title = response["artifact"]["title"].as_str().unwrap_or_default();
        if html.len() as u64 == size {
            ensure!(digest_hex(&Sha256::digest(&html)) == sha, "artifact integrity check failed; try again");
            return Ok((title.to_owned(), html));
        }
        let status = Ok(format!("Loading {title}: {}/{size} bytes · f cancels", html.len()));
        let _ = tx.send(Msg::FileDownload { id: id.to_owned(), status, done: false });
    }
}

/// Blocking: writes `html` as `dir/name`, replacing the copy an earlier `o` left.
fn write_temp(dir: &Path, name: &str, id: &str, html: &[u8]) -> Result<PathBuf> {
    private_dir(dir)?;
    let partial = Partial(dir.join(format!(".{id}.partial")));
    std::fs::write(&partial.0, html)?;
    let path = dir.join(name);
    // A rename replaces whatever is there (even a link) instead of writing through it.
    std::fs::rename(&partial.0, &path)?;
    Ok(path)
}

/// Makes `dir`, which only its owner writes in. On Unix the temp folder may be shared: a link, or a
/// folder others may write in, is refused, so no one can swap the artifact before the browser reads it
/// (one only someone else may write in fails the write). The desktop app's copy (0755) is fine.
fn private_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        match std::fs::DirBuilder::new().mode(0o700).create(dir) {
            Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => return Err(error.into()),
            _ => {}
        }
        let meta = std::fs::symlink_metadata(dir)?;
        let writable_by_others = meta.permissions().mode() & 0o022;
        ensure!(meta.is_dir() && writable_by_others == 0, "{} isn't a private folder", dir.display());
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)?;
    Ok(())
}

/// `<title>.html` as a file name on every system: no path separators or characters Windows refuses,
/// no leading dot (a hidden file).
pub fn file_name(title: &str) -> String {
    const UNSAFE: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let safe: String = title.chars().map(|c| if c.is_control() || UNSAFE.contains(&c) { ' ' } else { c }).collect();
    let safe = safe.split_whitespace().collect::<Vec<_>>().join(" ");
    let safe = safe.trim_matches(|c: char| c == '.' || c == ' ');
    format!("{}.html", if safe.is_empty() { "Artifact" } else { safe })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_file_names_stay_one_plain_file() {
        assert_eq!(file_name("Weekly report"), "Weekly report.html");
        assert_eq!(file_name("Q3: revenue / costs"), "Q3 revenue costs.html");
        assert_eq!(file_name("../../etc/passwd"), "etc passwd.html");
        assert_eq!(file_name(".hidden."), "hidden.html");
        assert_eq!(file_name("a\nb\tc"), "a b c.html");
        assert_eq!(file_name(" ?* "), "Artifact.html");
        assert_eq!(file_name("報告：第一週"), "報告：第一週.html", "only ASCII separators are unsafe");
    }

    #[cfg(unix)]
    #[test]
    fn temp_copies_replace_the_last_one_where_only_the_owner_writes() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = std::env::temp_dir().join(format!("codync-artifacts-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).expect("test folder");
        let dir = root.join("codync-artifacts");
        let first = write_temp(&dir, "A.html", "1", b"one").expect("first copy");
        assert_eq!(write_temp(&dir, "A.html", "2", b"two").expect("second copy"), first);
        assert_eq!(std::fs::read(&first).expect("copy"), b"two");
        assert_eq!(std::fs::read_dir(&dir).expect("folder").count(), 1, "no partial left behind");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert!(write_temp(&dir, "A.html", "3", b"three").is_ok(), "a folder others only read is fine");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("chmod");
        assert!(write_temp(&dir, "A.html", "4", b"four").is_err(), "a folder others write in is refused");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
