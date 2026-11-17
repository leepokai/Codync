//! A phone scanning a computer that isn't in an account (docs/plans/optional-sign-in.md): the computer confirms in
//! the desktop app, the terminal UI, `codync-host pair` or `codync-host access` before the phone gets in, and says
//! whether it joins the phone's account. Without joining, the phone connects only over Wi-Fi or Tailscale. A paired
//! phone adding the computer to its account later asks the same way. Both screens show the same 6-digit code
//! ([`crate::remote::crypto::pair_check_code`]).

use super::certificate::Certified;
use super::{crypto, owner};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

/// What this computer said to a phone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Let it in, and join its account.
    Join,
    /// Let it in; this computer stays out of an account, so the phone connects only over Wi-Fi or Tailscale.
    PhoneOnly,
    Decline,
}

impl Answer {
    /// The `decidePairing` body that gives this answer to `request_id`.
    pub fn body(self, request_id: &Value) -> Value {
        match self {
            Self::Join => json!({"requestId": request_id, "approve": true, "join": true}),
            Self::PhoneOnly => json!({"requestId": request_id, "approve": true, "join": false}),
            Self::Decline => json!({"requestId": request_id, "approve": false}),
        }
    }
}

/// What this computer is asked about one phone.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub device_name: String,
    pub platform: String,
    pub email: Option<String>,
    pub code: String,
    /// The phone is signed in and the Codync cloud is on, so this computer can join its account.
    pub can_join: bool,
    /// It can stay out of an account: not when the phone came over the Codync relay, which it couldn't use afterwards.
    pub phone_only: bool,
    /// The phones that lose access if it joins.
    pub removes: Vec<String>,
}

impl Question {
    /// The phone with `key`, signed in to `account` if it has one. The caller says which answers it may get.
    pub fn about(hub: &Hub, key: &str, device_name: &str, platform: &str, account: Option<&Certified>) -> Result<Self> {
        Ok(Self {
            device_name: device_name.to_owned(),
            platform: platform.to_owned(),
            email: account.and_then(|a| a.email.clone()),
            code: crypto::pair_check_code(&crypto::unb64_n::<32>(key)?, &hub.identity.sign_pub()),
            can_join: false,
            phone_only: false,
            removes: account.map_or_else(Vec::new, |a| owner::removed_by_joining(hub, &a.user_id, key)),
        })
    }
}

/// How long the computer has to answer: inside the 2 minutes the relay keeps a pairing socket open, so an
/// answer still reaches the phone, which waits longer.
pub const ASK_FOR: Duration = Duration::from_secs(90);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    #[serde(rename = "requestId")]
    id: String,
    #[serde(flatten)]
    question: Question,
    expires_at: i64,
    #[serde(skip)]
    answer: Option<oneshot::Sender<Answer>>,
}

/// The phones waiting for this computer's answer (memory only: a restart declines them).
#[derive(Default)]
pub struct PairingRequests(Mutex<Vec<Pending>>);

impl PairingRequests {
    pub fn json(&self) -> Vec<Value> {
        self.0.locked().iter().map(|p| json!(p)).collect()
    }
}

/// Asks this computer about a phone; [`Answer::Decline`] when it says no or nobody answers in time. Dropping the
/// future (the phone left) withdraws the question.
pub async fn ask(hub: &Hub, question: Question) -> Answer {
    let (tx, rx) = oneshot::channel();
    let id = uuid::Uuid::new_v4().to_string();
    hub.pairing_requests.0.locked().push(Pending {
        id: id.clone(),
        question,
        expires_at: now_ms() + i64::try_from(ASK_FOR.as_millis()).unwrap_or(i64::MAX),
        answer: Some(tx),
    });
    emit(hub);
    let _withdraw = Withdraw { hub, id };
    tokio::time::timeout(ASK_FOR, rx).await.ok().and_then(Result::ok).unwrap_or(Answer::Decline)
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

/// `decidePairing`: this computer's answer to one request. `join` says whether it joins the phone's account; left
/// out, it joins when it can.
pub fn decide(hub: &Hub, id: &str, approve: bool, join: Option<bool>) -> Result<()> {
    let mut requests = hub.pairing_requests.0.locked();
    let gone = || anyhow!("That request is gone. Scan the code again on the phone.");
    let request = requests.iter_mut().find(|p| p.id == id).ok_or_else(gone)?;
    let answer = match (approve, join.unwrap_or(request.question.can_join)) {
        (false, _) => Answer::Decline,
        (true, true) if request.question.can_join => Answer::Join,
        (true, false) if request.question.phone_only => Answer::PhoneOnly,
        (true, true) => bail!("This computer can't join that phone's account."),
        (true, false) => bail!("This phone can only connect if the computer joins its account."),
    };
    let _ = request.answer.take().ok_or_else(gone)?.send(answer);
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

    fn question(can_join: bool, phone_only: bool) -> Question {
        Question {
            device_name: "Pixel".into(),
            platform: "android".into(),
            email: Some("k@example.com".into()),
            code: "123456".into(),
            can_join,
            phone_only,
            removes: vec!["Old phone".into()],
        }
    }

    async fn asking(hub: &Arc<Hub>, can_join: bool, phone_only: bool) -> (tokio::task::JoinHandle<Answer>, String) {
        let task = tokio::spawn({
            let hub = Arc::clone(hub);
            async move { ask(&hub, question(can_join, phone_only)).await }
        });
        while hub.pairing_requests.json().is_empty() {
            tokio::task::yield_now().await;
        }
        (task, hub.pairing_requests.json()[0]["requestId"].as_str().unwrap().to_owned())
    }

    #[test]
    fn a_question_reads_the_same_on_every_screen() {
        let requests = PairingRequests::default();
        requests.0.locked().push(Pending {
            id: "r1".into(),
            question: question(true, false),
            expires_at: 9,
            answer: None,
        });
        assert_eq!(
            requests.json(),
            [json!({"requestId": "r1", "deviceName": "Pixel", "platform": "android", "email": "k@example.com",
                    "code": "123456", "canJoin": true, "phoneOnly": false, "removes": ["Old phone"], "expiresAt": 9})]
        );
    }

    #[tokio::test]
    async fn the_computer_answers() {
        let hub = hub();
        for (approve, join, answer) in [
            (true, Some(true), Answer::Join),
            (true, None, Answer::Join),
            (true, Some(false), Answer::PhoneOnly),
            (false, None, Answer::Decline),
        ] {
            let (task, id) = asking(&hub, true, true).await;
            let request = hub.pairing_requests.json()[0].clone();
            assert_eq!((request["deviceName"].as_str(), request["code"].as_str()), (Some("Pixel"), Some("123456")));
            assert_eq!(request["removes"], json!(["Old phone"]));
            decide(&hub, &id, approve, join).unwrap();
            assert_eq!(task.await.unwrap(), answer);
            assert!(hub.pairing_requests.json().is_empty(), "answered requests go");
        }
        assert!(decide(&hub, "gone", true, None).is_err());
    }

    #[tokio::test]
    async fn only_an_answer_it_was_asked_for_counts() {
        let hub = hub();
        let (task, id) = asking(&hub, true, false).await;
        assert!(decide(&hub, &id, true, Some(false)).is_err(), "a phone on the relay can't stay out of the account");
        decide(&hub, &id, true, None).unwrap();
        assert_eq!(task.await.unwrap(), Answer::Join);

        let (task, id) = asking(&hub, false, true).await;
        assert!(decide(&hub, &id, true, Some(true)).is_err(), "a signed-out phone has no account to join");
        decide(&hub, &id, true, None).unwrap();
        assert_eq!(task.await.unwrap(), Answer::PhoneOnly, "left out, an approval lets it in without joining");
    }

    #[tokio::test]
    async fn a_phone_that_leaves_takes_its_question_along() {
        let hub = hub();
        let (task, _) = asking(&hub, true, true).await;
        task.abort();
        let _ = task.await;
        assert!(hub.pairing_requests.json().is_empty(), "no screen keeps asking about a phone that is gone");
    }
}
