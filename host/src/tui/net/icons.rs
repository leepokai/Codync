use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use tokio::io::AsyncReadExt;

/// Local-file access belongs to this client, never to a remotely callable host method.
pub(super) async fn prepare(body: &Value) -> Result<Value> {
    let mut body = body.clone();
    if let Some(path) = body["localFile"].as_str() {
        let file = tokio::fs::File::open(path).await.context("Cannot open image")?;
        let mut bytes = Vec::new();
        file.take(5 * 1024 * 1024 + 1).read_to_end(&mut bytes).await?;
        ensure!(!bytes.is_empty() && bytes.len() <= 5 * 1024 * 1024, "Choose an image up to 5 MiB");
        body["data"] = STANDARD.encode(bytes).into();
        if let Some(object) = body.as_object_mut() {
            object.remove("localFile");
        }
    }
    Ok(body)
}
