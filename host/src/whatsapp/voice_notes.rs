//! Messaging voice notes use the host speech credentials and Hermes container detection.

use crate::store::Store;
use crate::voice::transcription;
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value};
use std::path::PathBuf;
use tokio::io::AsyncReadExt as _;

pub(super) fn stored_path(media: &Map<String, Value>) -> Result<Option<PathBuf>> {
    let Some(path) = media.get("_codyncPath").and_then(Value::as_str) else { return Ok(None) };
    let root = crate::service::data_dir().join("whatsapp-media").canonicalize()?;
    let path = PathBuf::from(path).canonicalize()?;
    ensure!(path.starts_with(root) && path.is_file(), "Voice attachment is outside its media store");
    Ok(Some(path))
}

fn audio(media: &Map<String, Value>) -> bool {
    media.get("kind").and_then(Value::as_str) == Some("audio")
        || media.get("mimetype").and_then(Value::as_str).is_some_and(|mime| mime.starts_with("audio/"))
}

async fn transcript(store: &Store, media: &Map<String, Value>) -> Result<String> {
    if let Some(path) = stored_path(media)? {
        let file = tokio::fs::File::open(path).await?;
        let mut bytes = Vec::new();
        file.take(u64::try_from(transcription::MAX_NOTE_BYTES)? + 1).read_to_end(&mut bytes).await?;
        return transcription::note(store, &bytes).await;
    }
    let data = media.get("data").and_then(Value::as_str).context("Voice note has no audio data")?;
    ensure!(data.len() <= transcription::MAX_NOTE_BYTES.div_ceil(3) * 4, "Voice note exceeds 20 MiB");
    let bytes = STANDARD.decode(data).context("Voice note is not valid base64")?;
    transcription::note(store, &bytes).await
}

pub(super) async fn append(store: &Store, media: &Map<String, Value>, note: String) -> String {
    if !audio(media) {
        return note;
    }
    match transcript(store, media).await {
        Ok(text) => format!("{note}\n\n[Voice note transcript]\n{text}"),
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "WhatsApp voice note could not be transcribed");
            format!(
                "{note}\n\n[Voice note could not be transcribed. Keep the original attachment and ask the sender to clarify.]"
            )
        }
    }
}
