//! Bridge write completion is not delivery: wait for the matching send acknowledgement.

use crate::LockExt;
use anyhow::{Result, anyhow, ensure};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;

#[derive(Default)]
pub(super) struct Receipts(Mutex<HashMap<String, oneshot::Sender<Result<()>>>>);

pub(super) struct Pending<'a> {
    book: &'a Receipts,
    pub id: String,
    receiver: Option<oneshot::Receiver<Result<()>>>,
}

impl Receipts {
    pub fn reserve(&self) -> Result<Pending<'_>> {
        let mut entries = self.0.locked();
        ensure!(entries.len() < 64, "WhatsApp has too many pending sends");
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        entries.insert(id.clone(), sender);
        Ok(Pending { book: self, id, receiver: Some(receiver) })
    }

    pub fn accept(&self, event: &Value) -> bool {
        if !matches!(event["type"].as_str(), Some("sent" | "error")) {
            return false;
        }
        let Some(id) = event["id"].as_str() else { return false };
        if let Some(sender) = self.0.locked().remove(id) {
            let acknowledged = event["type"] == "sent" && event["messageId"].as_str().is_some_and(|id| !id.is_empty());
            // Provider errors can contain personal data. Keep them out of routine logs.
            let result = if acknowledged { Ok(()) } else { Err(anyhow!("WhatsApp did not confirm the send")) };
            let _ = sender.send(result);
        }
        true
    }

    pub fn disconnect(&self) {
        for (_, sender) in self.0.locked().drain() {
            let _ = sender.send(Err(anyhow!("WhatsApp disconnected; the send outcome is unknown")));
        }
    }
}

impl Pending<'_> {
    pub async fn wait(mut self) -> Result<()> {
        let receiver = self.receiver.take().ok_or_else(|| anyhow!("WhatsApp receipt was already consumed"))?;
        tokio::time::timeout(std::time::Duration::from_secs(45), receiver)
            .await
            .map_err(|_| anyhow!("WhatsApp did not acknowledge the send; its outcome is unknown"))?
            .map_err(|_| anyhow!("WhatsApp acknowledgement was interrupted; its outcome is unknown"))?
    }
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.book.0.locked().remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn acknowledgements_are_correlated_and_require_a_message_id() {
        let book = Receipts::default();
        let first = book.reserve().unwrap();
        let second = book.reserve().unwrap();
        assert!(book.accept(&json!({"type":"sent","id":second.id,"messageId":"receipt"})));
        assert!(book.accept(&json!({"type":"sent","id":first.id,"messageId":null})));
        assert!(second.wait().await.is_ok());
        assert!(first.wait().await.is_err());
    }

    #[tokio::test]
    async fn cancelled_or_disconnected_sends_leave_no_receipt_waiters() {
        let book = Receipts::default();
        let cancelled = book.reserve().unwrap();
        drop(cancelled);
        assert!(book.0.locked().is_empty());
        let disconnected = book.reserve().unwrap();
        book.disconnect();
        assert!(disconnected.wait().await.is_err());
        assert!(book.0.locked().is_empty());
    }
}
