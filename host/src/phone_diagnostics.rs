//! A phone's performance measurements, logged locally so a slow phone can be diagnosed later.
//! `recordPhonePerformance` appends one JSON line to `phone-performance.jsonl` in the data
//! directory. Only numbers are stored: no message content, names or ids, and nothing goes to
//! analytics.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt as _;

/// A measurement a phone sends (`apps/*/PhonePerformance.*`): `type` picks the shape.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Measurement {
    /// Waiting for a chat's messages to reach their first laid-out frame.
    #[serde(rename_all = "camelCase")]
    ChatOpen { platform: Platform, duration_ms: u64 },
    /// One scroll through a list.
    #[serde(rename_all = "camelCase")]
    Scroll {
        platform: Platform,
        list: ScrollList,
        rows: u64,
        frames: u64,
        hitches: u64,
        longest_ms: u64,
        hitch_ms_per_s: u64,
        fps: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop: Option<ScrollStop>,
        #[serde(skip_serializing_if = "Option::is_none")]
        jump: Option<u64>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Platform {
    Ios,
    Android,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScrollList {
    Bots,
    Chat,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ScrollStop {
    Top,
    End,
    Middle,
}

fn file() -> PathBuf {
    crate::service::data_dir().join("phone-performance.jsonl")
}

/// `recordPhonePerformance`: type-check `body` and append it as one JSONL line.
pub async fn record(body: Value) -> Result<Value> {
    record_at(&file(), body).await?;
    Ok(json!({}))
}

async fn record_at(path: &Path, body: Value) -> Result<()> {
    let measurement: Measurement = serde_json::from_value(body).context("malformed phone performance")?;
    // Serialize first, then stamp: an internally-tagged enum can't be `flatten`ed with other
    // fields. The host stamps arrival time, so a phone's clock is never trusted.
    let mut event = serde_json::to_value(&measurement)?;
    event
        .as_object_mut()
        .context("a measurement is an object")?
        .insert("at".to_owned(), chrono::Utc::now().to_rfc3339().into());
    let mut line = serde_json::to_vec(&event)?;
    line.push(b'\n');
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("opening {}", path.display()))?;
    file.write_all(&line).await.with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn log() -> PathBuf {
        std::env::temp_dir().join(format!("codync-phone-perf-{}", uuid::Uuid::new_v4())).join("phone-performance.jsonl")
    }

    fn lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path).unwrap().lines().map(|line| serde_json::from_str(line).unwrap()).collect()
    }

    #[tokio::test]
    async fn both_kinds_append_valid_jsonl() {
        let path = log();
        record_at(&path, json!({"type": "chat_open", "platform": "ios", "durationMs": 240})).await.unwrap();
        record_at(
            &path,
            json!({
                "type": "scroll", "platform": "android", "list": "chat", "rows": 320, "frames": 240,
                "hitches": 3, "longestMs": 58, "hitchMsPerS": 21, "fps": 120, "stop": "end", "jump": 14,
                "someFutureField": true
            }),
        )
        .await
        .unwrap();
        let lines = lines(&path);
        assert_eq!(lines.len(), 2);
        let (open, scroll) = (&lines[0], &lines[1]);
        assert_eq!(open["type"], "chat_open");
        assert_eq!(open["platform"], "ios");
        assert_eq!(open["durationMs"], 240);
        assert_eq!(scroll["type"], "scroll");
        assert_eq!(scroll["list"], "chat");
        assert_eq!(scroll["hitchMsPerS"], 21);
        assert_eq!(scroll["stop"], "end");
        assert_eq!(scroll["jump"], 14);
        assert!(scroll.get("someFutureField").is_none(), "unknown fields aren't logged");
        for line in &lines {
            assert!(line["at"].is_string());
            chrono::DateTime::parse_from_rfc3339(line["at"].as_str().unwrap()).unwrap();
        }
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn optional_fields_are_omitted_not_null() {
        let path = log();
        record_at(
            &path,
            json!({"type": "scroll", "platform": "ios", "list": "bots", "rows": 60, "frames": 90,
                   "hitches": 0, "longestMs": 9, "hitchMsPerS": 0, "fps": 120}),
        )
        .await
        .unwrap();
        let lines = lines(&path);
        assert!(lines[0].get("stop").is_none());
        assert!(lines[0].get("jump").is_none());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn malformed_input_is_rejected_and_writes_nothing() {
        let path = log();
        for bad in [
            json!({}),
            json!({"type": "nope", "platform": "ios"}),
            json!({"type": "chat_open", "platform": "ios"}),
            json!({"type": "scroll", "platform": "android", "list": "chat", "rows": 1, "frames": 1,
                   "hitches": 0, "longestMs": 1, "hitchMsPerS": 0}),
            json!({"type": "scroll", "platform": "android", "list": "settings", "rows": 1, "frames": 1,
                   "hitches": 0, "longestMs": 1, "hitchMsPerS": 0, "fps": 60}),
        ] {
            assert!(record_at(&path, bad).await.is_err());
        }
        assert!(!path.exists());
    }
}
