//! Durable text only. Attachments remain session-local, as on the phone and desktop.
use super::app::Editor;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

pub struct Drafts {
    path: PathBuf,
    saved: HashMap<String, String>,
}

impl Drafts {
    pub async fn load(url: &str) -> Result<Self> {
        use std::fmt::Write as _;
        let digest = Sha256::digest(url.as_bytes()).iter().fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        });
        let name = format!("tui-drafts-{digest}.json");
        let path = crate::service::data_dir().join(name);
        Self::at(path).await
    }

    async fn at(path: PathBuf) -> Result<Self> {
        tokio::task::spawn_blocking(move || {
            let saved = match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice(&bytes)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
                Err(e) => return Err(e.into()),
            };
            Ok(Self { path, saved })
        })
        .await?
    }

    pub fn restore(&self) -> HashMap<String, Editor> {
        self.saved.iter().map(|(key, text)| (key.clone(), Editor::with(text))).collect()
    }

    pub async fn save(&mut self, drafts: &HashMap<String, Editor>) -> Result<()> {
        let next: HashMap<_, _> = drafts
            .iter()
            .filter(|(_, editor)| !editor.text.is_empty())
            .map(|(key, editor)| (key.clone(), editor.text.clone()))
            .collect();
        if next == self.saved {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&next)?;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
            let result = (|| -> Result<()> {
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&temporary)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                std::fs::rename(&temporary, &path)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(temporary);
            }
            result
        })
        .await??;
        self.saved = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reopening_restores_exact_text_and_clears_only_the_sent_destination() {
        let dir = std::env::temp_dir().join(format!("codync-drafts-{}", uuid::Uuid::new_v4()));
        let path = dir.join("drafts.json");
        let mut store = Drafts::at(path.clone()).await.unwrap();
        let mut drafts = HashMap::from([
            ("bot".into(), Editor::with("  中文\nunfinished 🐱\n")),
            ("other".into(), Editor::with("other bot")),
        ]);
        store.save(&drafts).await.unwrap();
        let restored = Drafts::at(path.clone()).await.unwrap().restore();
        assert_eq!(restored["bot"].text, drafts["bot"].text);
        drafts.remove("bot");
        store.save(&drafts).await.unwrap();
        let restored = Drafts::at(path).await.unwrap().restore();
        assert!(!restored.contains_key("bot"));
        assert_eq!(restored["other"].text, "other bot");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
