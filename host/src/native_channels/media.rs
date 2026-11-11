//! Resolve published chat files into bounded, immutable transport-owned snapshots.
use crate::chat::files::{self, SharedFile};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};

#[derive(Default)]
pub(super) struct PublishedFiles {
    values: Vec<Value>,
    paths: Vec<std::path::PathBuf>,
}
impl PublishedFiles {
    pub(super) fn values(&self) -> Vec<Value> {
        self.values.clone()
    }
    pub(super) fn handoff(&mut self) {
        self.paths.clear();
    }
}
impl Drop for PublishedFiles {
    fn drop(&mut self) {
        for path in &self.paths {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error,"Native unpublished snapshot cleanup failed");
            }
        }
    }
}

pub(super) async fn outgoing(item: &Value) -> Result<PublishedFiles> {
    let bot = item["bot"].as_str().context("Native file bot is missing")?.to_owned();
    let values = item["data"]["files"].as_array().cloned().unwrap_or_default();
    if values.is_empty() {
        return Ok(PublishedFiles::default());
    }
    ensure!(item["data"]["author"] == bot, "Native files must be published by their own bot");
    tokio::task::spawn_blocking(move || resolve(&bot, &values)).await?
}

fn resolve(bot: &str, values: &[Value]) -> Result<PublishedFiles> {
    ensure!(
        Path::new(bot).components().count() == 1 && !bot.contains(['/', '\\', ':']) && bot != "." && bot != "..",
        "Invalid native file bot"
    );
    ensure!(values.len() <= 10, "Native delivery accepts at most ten files");
    let root = files::root(bot);
    let target = crate::service::data_dir().join("native-channel-outgoing");
    resolve_at(&root, &target, values)
}

fn resolve_at(root: &Path, target: &Path, values: &[Value]) -> Result<PublishedFiles> {
    std::fs::create_dir_all(target)?;
    let mut result = PublishedFiles::default();
    let mut total = 0_u64;
    for value in values {
        let meta: SharedFile = serde_json::from_value(value.clone())?;
        uuid::Uuid::parse_str(&meta.id)?;
        let name = files::name(&meta.name)?;
        total = total.checked_add(meta.size).context("Native file size overflow")?;
        ensure!(meta.size <= 20 * 1024 * 1024 && total <= 20 * 1024 * 1024, "Native files exceed 20 MiB total");
        let path = target.join(uuid::Uuid::new_v4().to_string());
        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
        result.paths.push(path.clone());
        let copied = (|| -> Result<()> {
            let mut offset = 0;
            let mut digest = Sha256::new();
            // read() validates regular-file type, UUID-derived path and immutable size on each chunk.
            while offset < meta.size {
                let bytes = files::read(root, &meta, offset)?;
                ensure!(!bytes.is_empty(), "Native shared snapshot is truncated");
                offset += bytes.len() as u64;
                digest.update(&bytes);
                output.write_all(&bytes)?;
            }
            if meta.size == 0 {
                files::read(root, &meta, 0)?;
            }
            ensure!(files::digest_hex(&digest.finalize()) == meta.sha256, "Native shared snapshot checksum changed");
            output.sync_all()?;
            Ok(())
        })();
        copied?;
        result.values.push(json!({"path":path,"name":name,"size":meta.size,"sha256":meta.sha256}));
    }
    Ok(result)
}

pub(super) async fn cleanup_sent(store: &crate::store::Store, id: &str) -> Result<()> {
    use rusqlite::OptionalExtension;
    let payload: Option<String> = store.with_connection(|db| {
        Ok(db.query_row("SELECT payload FROM native_channel_outbox WHERE entry=?1", [id], |r| r.get(0)).optional()?)
    })?;
    let Some(payload) = payload else { return Ok(()) };
    let payload: Value = serde_json::from_str(&payload)?;
    let root = crate::service::data_dir().join("native-channel-outgoing");
    for file in payload["files"].as_array().into_iter().flatten().take(10) {
        let Some(path) = file["path"].as_str().map(Path::new) else { continue };
        if path.parent() == Some(root.as_path()) {
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

/// Called only before a replacement transport starts, after its predecessor has dropped.
/// Unknown receipts stay non-retryable; their disposable copies are no longer needed.
pub(super) async fn cleanup_unknown(store: &crate::store::Store, connection: &str) -> Result<()> {
    let ids:Vec<String>=store.with_connection(|db| {
        let mut q=db.prepare("SELECT entry FROM native_channel_outbox WHERE connection=?1 AND state IN ('sending','unknown') AND json_array_length(payload,'$.files')>0 ORDER BY updated_at LIMIT 64")?;
        Ok(q.query_map([connection],|r|r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    for id in ids {
        cleanup_sent(store, &id).await?;
        store.with_connection(|db| {db.execute("UPDATE native_channel_outbox SET state='unknown',payload=json_set(payload,'$.files',json('[]')) WHERE entry=?1 AND state IN ('sending','unknown')",[&id])?;Ok(())})?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outgoing_files_refuse_path_metadata_and_oversized_publications() {
        assert!(resolve("bot", &[json!({"path":"secret","name":"secret"})]).is_err());
        assert!(resolve("../bot", &[]).is_err());
        assert!(
            resolve(
                "bot",
                &[json!({"id":uuid::Uuid::new_v4().to_string(),"name":"large.bin","size":21*1024*1024,"sha256":"unused"})]
            )
            .is_err()
        );
    }

    #[test]
    fn published_file_uuid_resolves_to_verified_transport_snapshot() {
        let root = std::env::temp_dir().join(format!("native-media-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("published")).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let content = b"native published attachment";
        std::fs::write(root.join("published").join(&id), content).unwrap();
        let meta = json!({"id":id,"name":"report.txt","size":content.len(),"sha256":files::digest_hex(&Sha256::digest(content))});
        let files = resolve_at(&root.join("published"), &root.join("transport"), std::slice::from_ref(&meta)).unwrap();
        assert_eq!(std::fs::read(files.values()[0]["path"].as_str().unwrap()).unwrap(), content);
        let mut corrupt = meta;
        corrupt["sha256"] = "incorrect".into();
        assert!(resolve_at(&root.join("published"), &root.join("transport"), &[corrupt]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_or_unhanded_publications_remove_all_transport_copies() {
        let root = std::env::temp_dir().join(format!("native-media-cleanup-{}", uuid::Uuid::new_v4()));
        let source = root.join("published");
        let target = root.join("transport");
        std::fs::create_dir_all(&source).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let bytes = b"bounded snapshot";
        std::fs::write(source.join(&id), bytes).unwrap();
        let meta =
            json!({"id":id,"name":"file.txt","size":bytes.len(),"sha256":files::digest_hex(&Sha256::digest(bytes))});
        assert!(resolve_at(&source, &target, &[meta.clone(), json!({"invalid":true})]).is_err());
        assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
        {
            let guarded = resolve_at(&source, &target, &[meta]).unwrap();
            assert_eq!(guarded.values().len(), 1);
        }
        assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
