use super::Client;
use crate::chat::files::{CHUNK_SIZE, SharedFile};
use crate::chat::uploads::MAX_FILE;
use crate::tui::app::Msg;
use anyhow::{Result, anyhow, ensure};
use base64::Engine as _;
use serde_json::json;
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
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl Client {
    pub fn download_file(&self, entry: String, file: SharedFile, tx: UnboundedSender<Msg>) -> Download {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(Notify::new());
        let task = Download { id: id.clone(), cancel: cancel.clone() };
        let client = self.clone();
        tokio::spawn(async move {
            let saved = tokio::select! {
                () = cancel.notified() => return,
                result = save(&client, &entry, &file, &id, &tx) => result,
            };
            let status = saved.map(|path| format!("Saved {}", path.display())).map_err(|error| format!("{error:#}"));
            let _ = tx.send(Msg::FileDownload { id, status, done: true });
        });
        task
    }
}

async fn save(client: &Client, entry: &str, meta: &SharedFile, id: &str, tx: &UnboundedSender<Msg>) -> Result<PathBuf> {
    crate::chat::files::name(&meta.name)?;
    ensure!(
        meta.size <= MAX_FILE && meta.sha256.len() == 64 && meta.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid file metadata"
    );
    let dir = dirs::download_dir().or_else(dirs::home_dir).ok_or_else(|| anyhow!("No Downloads folder"))?;
    let partial = Partial(dir.join(format!(".codync-{id}.partial")));
    let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&partial.0).await?;
    let mut offset = 0;
    let mut hash = Sha256::new();
    loop {
        let result = client.call("readFile", &json!({"entryId": entry, "fileId": meta.id, "offset": offset})).await;
        let response = result.map_err(|e| anyhow!(e))?;
        ensure!(response["size"].as_u64() == Some(meta.size), "file size changed");
        let encoded = response["data"].as_str().ok_or_else(|| anyhow!("invalid file chunk"))?;
        ensure!(encoded.len() <= CHUNK_SIZE / 3 * 4, "file chunk too large");
        let data = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        ensure!(
            data.len() as u64 == (meta.size - offset).min(CHUNK_SIZE as u64),
            "download truncated or changed; press f to retry"
        );
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
    // Link a complete file without replacing a concurrently created destination.
    for _ in 0..10_000 {
        let path = super::free_path(&dir, &meta.name);
        match tokio::fs::hard_link(&partial.0, &path).await {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(anyhow!("No available filename in Downloads"))
}
