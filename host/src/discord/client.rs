//! The Discord REST calls the host makes: sending a reply, in pieces Discord accepts.

use super::Endpoint;
use crate::egress;
use anyhow::Result;
use serde_json::json;
use std::time::Duration;

/// Discord's limit for one message.
pub(super) const MAX_TEXT: usize = 2000;
/// What one piece may hold, leaving room for the split to land on a character boundary.
const PIECE: usize = 1900;

/// Sends `text` in the channel, the first piece answering `reply_to`; `files` ride on the last piece.
pub(super) async fn message(
    ep: &Endpoint,
    token: &str,
    channel: &str,
    reply_to: &str,
    text: &str,
    files: &[serde_json::Value],
) -> Result<()> {
    let url = format!("{}/channels/{channel}/messages", ep.api);
    let client = egress::client(ep.policy, &url).await?;
    let pieces = pieces(text);
    let last = pieces.len().saturating_sub(1);
    for (index, piece) in pieces.into_iter().enumerate() {
        let mut body = json!({"content": piece, "allowed_mentions": {"parse": ["users"]}});
        if index == 0 && !reply_to.is_empty() {
            body["message_reference"] =
                json!({"message_id": reply_to, "channel_id": channel, "fail_if_not_exists": false});
        }
        let mut request = client.post(&url).header("authorization", format!("Bot {token}"));
        if index == last && !files.is_empty() {
            let mut form = reqwest::multipart::Form::new().text("payload_json", body.to_string());
            let mut attached = 0;
            for file in files {
                let Some(path) = file["path"].as_str() else { continue };
                let Ok(bytes) = tokio::fs::read(path).await else { continue };
                let name = file["name"].as_str().unwrap_or("file").chars().take(120).collect::<String>();
                let type_ = file["mimetype"].as_str().unwrap_or("application/octet-stream").to_owned();
                let part = reqwest::multipart::Part::bytes(bytes)
                    .file_name(name)
                    .mime_str(&type_)
                    .unwrap_or_else(|_| reqwest::multipart::Part::bytes(Vec::new()));
                form = form.part(format!("files[{attached}]"), part);
                attached += 1;
            }
            if attached > 0 {
                request = request.multipart(form);
            } else {
                request = request.json(&body);
            }
        } else {
            request = request.json(&body);
        }
        let response = request
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Discord message failed: {}", e.without_url()))?;
        let status = response.status();
        if !status.is_success() {
            let why = egress::json(response, 16 * 1024).await.unwrap_or_default();
            let why = why["message"].as_str().unwrap_or("refused").to_owned();
            anyhow::bail!("Discord answered {status}: {why}");
        }
    }
    Ok(())
}

/// `text` cut into pieces Discord accepts, on character boundaries where it can.
pub(super) fn pieces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if current.chars().count() >= PIECE {
            out.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() || out.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_within_discords_limit() {
        let text = "é".repeat(MAX_TEXT + 500);
        let out = pieces(&text);
        assert!(out.len() >= 2);
        assert!(out.iter().all(|p| p.chars().count() <= PIECE));
        assert_eq!(out.concat(), text);
        assert_eq!(pieces("short"), ["short"]);
    }
}
