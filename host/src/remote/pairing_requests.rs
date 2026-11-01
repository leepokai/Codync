//! A signed-in phone scanning a computer that isn't in an account (docs/plans/require-sign-in.md, phase 2):
//! the computer confirms in the desktop app, the terminal UI, `codync-host pair` or `codync-host access` before
//! the phone gets in and the computer joins the phone's account. Both screens show the same 6-digit code
//! ([`crate::remote::crypto::pair_check_code`]).

use crate::LockExt;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

/// How long the computer has to answer: inside the 2 minutes the relay keeps a pairing socket open, so an
/// answer still reaches the phone, which waits longer.
pub const ASK_FOR: Duration = Duration::from_secs(90);

struct Pending {
    id: String,
    device_name: String,
    platform: String,
    email: Option<String>,
    code: String,
    expires_at: i64,
    answer: Option<oneshot::Sender<bool>>,
}

/// The phones waiting for this computer's answer (memory only: a restart declines them).
#[derive(Default)]
pub struct PairingRequests(Mutex<Vec<Pending>>);

impl PairingRequests {
    pub fn json(&self) -> Vec<Value> {
        self.0
            .locked()
            .iter()
            .map(|p| {
                json!({"requestId": p.id, "deviceName": p.device_name, "platform": p.platform,
                       "email": p.email, "code": p.code, "expiresAt": p.expires_at})
            })
            .collect()
    }
}

/// Asks this computer whether the phone may come in and bring it into `email`'s account; false when it says
/// no or nobody answers in time. Dropping the future (the phone left) withdraws the question.
pub async fn ask(hub: &Hub, device_name: &str, platform: &str, email: Option<&str>, code: String) -> bool {
    let (tx, rx) = oneshot::channel();
    let id = uuid::Uuid::new_v4().to_string();
    hub.pairing_requests.0.locked().push(Pending {
        id: id.clone(),
        device_name: device_name.to_owned(),
        platform: platform.to_owned(),
        email: email.map(str::to_owned),
        code,
        expires_at: now_ms() + i64::try_from(ASK_FOR.as_millis()).unwrap_or(i64::MAX),
        answer: Some(tx),
    });
    emit(hub);
    let _withdraw = Withdraw { hub, id };
    matches!(tokio::time::timeout(ASK_FOR, rx).await, Ok(Ok(true)))
}

/// Takes a question off every screen once it is answered, unanswered, or the phone is gone.
struct Withdraw<'a> {
    hub: &'a Hub,
    id: String,
}

impl Drop for Withdraw<'_> {
    fn drop(&mut self) {
        self.hub.pairing_requests.0.locked().retain(|p| p.id != self.id);
        emit(self.hub);
    }
}

/// `decidePairing`: this computer's answer to one request.
pub fn decide(hub: &Hub, id: &str, approve: bool) -> Result<()> {
    let answer = hub
        .pairing_requests
        .0
        .locked()
        .iter_mut()
        .find(|p| p.id == id)
        .and_then(|p| p.answer.take())
        .ok_or_else(|| anyhow!("That request is gone. Scan the code again on the phone."))?;
    let _ = answer.send(approve);
    Ok(())
}

fn emit(hub: &Hub) {
    let _ = hub.events.send(json!({"type": "pairingRequests", "requests": hub.pairing_requests.json()}));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::Store;
    use std::sync::Arc;

    fn hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-pr-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Hub::new(
            Store::open(&dir.join("t.db")).unwrap(),
            "h".into(),
            Identity::load_or_create(&dir).unwrap(),
            "t".into(),
            0,
        )
    }

    #[tokio::test]
    async fn the_computer_answers() {
        let hub = hub();
        for approve in [true, false] {
            let asking = tokio::spawn({
                let hub = Arc::clone(&hub);
                async move { ask(&hub, "Pixel", "android", Some("k@example.com"), "123456".into()).await }
            });
            while hub.pairing_requests.json().is_empty() {
                tokio::task::yield_now().await;
            }
            let request = hub.pairing_requests.json()[0].clone();
            assert_eq!((request["deviceName"].as_str(), request["code"].as_str()), (Some("Pixel"), Some("123456")));
            decide(&hub, request["requestId"].as_str().unwrap(), approve).unwrap();
            assert_eq!(asking.await.unwrap(), approve);
            assert!(hub.pairing_requests.json().is_empty(), "answered requests go");
        }
        assert!(decide(&hub, "gone", true).is_err());
    }

    #[tokio::test]
    async fn a_phone_that_leaves_takes_its_question_along() {
        let hub = hub();
        let asking = tokio::spawn({
            let hub = Arc::clone(&hub);
            async move { ask(&hub, "Pixel", "android", None, "123456".into()).await }
        });
        while hub.pairing_requests.json().is_empty() {
            tokio::task::yield_now().await;
        }
        asking.abort();
        let _ = asking.await;
        assert!(hub.pairing_requests.json().is_empty(), "no screen keeps asking about a phone that is gone");
    }
}
