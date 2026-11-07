//! The Discord REST calls the host makes: sending a reply, in pieces Discord accepts.

use super::Endpoint;
use crate::egress;
use anyhow::{Context as _, Result};
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

/// Sends a permission card with a button per option; returns the message id, so it can be settled.
pub(super) async fn card(
    ep: &Endpoint,
    token: &str,
    channel: &str,
    text: &str,
    buttons: &[(String, String)],
) -> Result<String> {
    let url = format!("{}/channels/{channel}/messages", ep.api);
    let client = egress::client(ep.policy, &url).await?;
    let row: Vec<serde_json::Value> = buttons
        .iter()
        .map(|(label, id)| {
            json!({"type": 2, "style": 1, "label": label.chars().take(80).collect::<String>(), "custom_id": id})
        })
        .collect();
    let body = json!({"content": text, "components": [{"type": 1, "components": row}]});
    let response = client
        .post(&url)
        .header("authorization", format!("Bot {token}"))
        .json(&body)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Discord card failed: {}", e.without_url()))?;
    let status = response.status();
    let answer = egress::json(response, 16 * 1024).await.unwrap_or_default();
    if !status.is_success() {
        let why = answer["message"].as_str().unwrap_or("refused").to_owned();
        anyhow::bail!("Discord answered {status}: {why}");
    }
    answer["id"].as_str().map(str::to_owned).context("Discord sent no card id")
}

/// Settles a card: its text changes and its buttons go away.
pub(super) async fn settle(ep: &Endpoint, token: &str, channel: &str, message: &str, text: &str) -> Result<()> {
    let url = format!("{}/channels/{channel}/messages/{message}", ep.api);
    let client = egress::client(ep.policy, &url).await?;
    let response = client
        .patch(&url)
        .header("authorization", format!("Bot {token}"))
        .json(&json!({"content": text, "components": []}))
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Discord card edit failed: {}", e.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        let why = egress::json(response, 16 * 1024).await.unwrap_or_default();
        let why = why["message"].as_str().unwrap_or("refused").to_owned();
        anyhow::bail!("Discord answered {status}: {why}");
    }
    Ok(())
}

/// Answers a component interaction: `kind` 6 defers the card's update, 4 shows a small ephemeral note.
pub(super) async fn interaction(ep: &Endpoint, token: &str, id: &str, kind: u8, note: Option<&str>) -> Result<()> {
    let url = format!("{}/interactions/{id}/{token}/callback", ep.api);
    let client = egress::client(ep.policy, &url).await?;
    let mut body = json!({"type": kind});
    if let Some(note) = note {
        body["data"] = json!({"content": note, "flags": 64});
    }
    let response = client.post(&url).json(&body).timeout(Duration::from_secs(20)).send().await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("Discord refused the interaction answer with {status}");
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
