//! Large backups reuse the chunked file transport used by chat attachments.

use super::Client;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

const CHUNK: usize = 384 * 1024;

pub(super) async fn call(client: &Client, method: &str, body: &Value) -> Result<Value, String> {
    if method == "importMemory" {
        let text = body["json"].as_str().unwrap_or_default();
        if text.len() > CHUNK {
            let upload = uuid::Uuid::new_v4().to_string();
            for (index, chunk) in text.as_bytes().chunks(CHUNK).enumerate() {
                let offset = index * CHUNK;
                client
                    .call(
                        "upload",
                        &json!({"botId":body["botId"], "uploadId":upload,
                    "name":"engram-backup.json", "data":STANDARD.encode(chunk), "offset":offset,
                    "done":offset + chunk.len() == text.len()}),
                    )
                    .await?;
            }
            return client.call(method, &json!({"botId":body["botId"], "uploadId":upload})).await;
        }
    }
    let result = client.call(method, body).await?;
    let Some(upload) = result["uploadId"].as_str().filter(|_| method == "exportMemory") else {
        return Ok(result);
    };
    let mut bytes = Vec::new();
    loop {
        let chunk =
            client.call("readUpload", &json!({"botId":body["botId"], "uploadId":upload, "offset":bytes.len()})).await?;
        let data = STANDARD.decode(chunk["data"].as_str().unwrap_or_default()).map_err(|e| e.to_string())?;
        if data.is_empty() {
            return Err("Memory backup transfer ended early".into());
        }
        bytes.extend_from_slice(&data);
        if bytes.len() as u64 >= chunk["size"].as_u64().unwrap_or(0) {
            break;
        }
    }
    Ok(json!({"json":String::from_utf8(bytes).map_err(|e| e.to_string())?}))
}
