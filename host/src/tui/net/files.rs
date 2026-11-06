use super::Client;
use crate::chat::files::{CHUNK_SIZE, SharedFile};
use crate::chat::uploads::MAX_FILE;
use crate::tui::app::Msg;
use anyhow::{Result, anyhow, ensure};
use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt as _;
use tokio::sync::{Notify, mpsc::UnboundedSender};

pub struct Download {
    pub id: String,
    cancel: Arc<Notify>,
}
impl Drop for Download {
    fn drop(&mut self) {
        self.cancel.notify_one();
    }
}
impl Download {
    /// Runs `work` (handed this download's id) until it ends or the download is dropped; its outcome is the last
    /// `Msg::FileDownload`.
    pub(super) fn spawn<F>(tx: UnboundedSender<Msg>, work: impl FnOnce(String) -> F) -> Self
    where
        F: Future<Output = Result<String>> + Send + 'static,
    {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(Notify::new());
        let (work, task) = (work(id.clone()), Self { id: id.clone(), cancel: cancel.clone() });
        tokio::spawn(async move {
            let done = tokio::select! {
                () = cancel.notified() => return,
                result = work => result,
            };
            let _ = tx.send(Msg::FileDownload { id, status: done.map_err(|error| format!("{error:#}")), done: true });
        });
        task
    }
}
pub(super) struct Partial(pub(super) PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl Client {
    pub fn download_file(&self, entry: String, file: SharedFile, tx: UnboundedSender<Msg>) -> Download {
        let (client, progress) = (self.clone(), tx.clone());
        Download::spawn(tx, move |id| async move {
            let path = save(&client, &entry, &file, &id, &progress).await?;
            Ok(format!("Saved {}", path.display()))
        })
    }
}

async fn save(client: &Client, entry: &str, meta: &SharedFile, id: &str, tx: &UnboundedSender<Msg>) -> Result<PathBuf> {
    crate::chat::files::name(&meta.name)?;
    ensure!(
        meta.size <= MAX_FILE && meta.sha256.len() == 64 && meta.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid file metadata"
    );
    let dir = downloads()?;
    let partial = Partial(dir.join(format!(".codync-{id}.partial")));
    let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&partial.0).await?;
    let mut offset = 0;
    let mut hash = Sha256::new();
    loop {
        let result = client.call("readFile", &json!({"entryId": entry, "fileId": meta.id, "offset": offset})).await;
        let response = result.map_err(|e| anyhow!(e))?;
        ensure!(response["size"].as_u64() == Some(meta.size), "file size changed");
        let data = decode_chunk(&response, meta.size, offset)?;
        file.write_all(&data).await?;
        hash.update(&data);
        offset += data.len() as u64;
        let _ = tx.send(Msg::FileDownload {
            id: id.to_owned(),
            status: Ok(format!("Downloading {}: {offset}/{} bytes · f cancels", meta.name, meta.size)),
            done: false,
        });
        if offset == meta.size {
            break;
        }
    }
    let digest: String = crate::chat::files::digest_hex(&hash.finalize());
    ensure!(digest == meta.sha256, "file integrity check failed; press f to retry");
    file.sync_all().await?;
    drop(file);
    place(&partial, &dir, &meta.name).await
}

/// One `readFile` or `readArtifact` chunk of a `size`-byte file: exactly the bytes from `offset` on.
pub(super) fn decode_chunk(response: &Value, size: u64, offset: u64) -> Result<Vec<u8>> {
    let encoded = response["data"].as_str().ok_or_else(|| anyhow!("invalid chunk"))?;
    ensure!(encoded.len() <= CHUNK_SIZE / 3 * 4, "chunk too large");
    let data = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    ensure!(
        data.len() as u64 == (size - offset).min(CHUNK_SIZE as u64),
        "download truncated or changed; press f to retry"
    );
    Ok(data)
}

pub(super) fn downloads() -> Result<PathBuf> {
    dirs::download_dir().or_else(dirs::home_dir).ok_or_else(|| anyhow!("No Downloads folder"))
}

/// Links a complete file in `dir` as `name` (or `name (2)`…), never replacing one created meanwhile.
pub(super) async fn place(partial: &Partial, dir: &std::path::Path, name: &str) -> Result<PathBuf> {
    for _ in 0..10_000 {
        let path = super::free_path(dir, name);
        match tokio::fs::hard_link(&partial.0, &path).await {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(anyhow!("No available filename in Downloads"))
}
