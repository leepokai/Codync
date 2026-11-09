//! The pair-a-phone sheet: its code, whether the computer is in an account, and the phones waiting for it to
//! let them in, and whether it joins their account (docs/plans/optional-sign-in.md).

use crossterm::event::KeyCode;
use serde_json::{Value, json};

use super::{After, App, Overlay, PairCode};
use crate::remote::pairing_requests::Answer;

impl App {
    pub(super) fn open_pair(&mut self) {
        self.overlays.push(Overlay::Pair(PairCode::Loading));
        self.request_pairing_code();
        self.call("cloudStatus", json!({}), After::Cloud);
        self.call("pairingRequests", json!({}), After::PairingRequests);
    }

    /// A new code, with a short one to type where the QR is hard to scan (a terminal over SSH).
    pub(super) fn request_pairing_code(&mut self) {
        self.call("pairing", json!({"shortCode": true}), After::Pairing);
    }

    pub(super) fn pair_reply(&mut self, after: &After, r: Result<Value, String>) {
        match (after, r) {
            (After::Pairing, r) => {
                if let Some(Overlay::Pair(code)) = self.overlays.last_mut() {
                    *code = match r {
                        // In place of the code, which may have expired.
                        Err(e) => PairCode::Failed(e),
                        Ok(v) => v["pairingUrl"].as_str().map_or_else(
                            || PairCode::Failed("The host sent no pairing code.".into()),
                            |url| PairCode::Ready {
                                url: url.to_owned(),
                                short_code: v["shortCode"].as_str().map(str::to_owned),
                            },
                        ),
                    };
                }
            }
            (After::Cloud, Ok(v)) => self.cloud = v,
            (After::PairingRequests, Ok(v)) => self.pairing_requests_changed(&v),
            // The host may not have the answer: its list brings the phone back while it still waits.
            (After::PairingDecided, Err(e)) => {
                self.flash(&e);
                self.call("pairingRequests", json!({}), After::PairingRequests);
            }
            // An answer the host took (its event follows), or a host before accounts, which has neither list: the
            // sheet shows the code alone.
            _ => {}
        }
    }

    /// A `pairingRequests` event: the whole list, each time it changes. A paired phone's Add to account needs no code,
    /// so no sheet may be open when it asks: this opens one, and closes it once nobody's asking.
    pub(super) fn pairing_requests_changed(&mut self, v: &Value) {
        self.pairing_requests = requests(v);
        if self.pairing_requests.is_empty() {
            self.overlays.retain(|o| !matches!(o, Overlay::Pair(PairCode::QuestionOnly)));
        } else if !self.overlays.iter().any(|o| matches!(o, Overlay::Pair(_))) {
            self.overlays.push(Overlay::Pair(PairCode::QuestionOnly));
        }
    }

    /// A key on the sheet about the first waiting phone. A key it isn't offered means nothing.
    pub(super) fn decide_pairing(&mut self, key: KeyCode) {
        let Some(request) = self.pairing_requests.first() else { return };
        let Some(answer) = pairing_answer(key, request) else { return };
        let body = answer.body(&request["requestId"]);
        // Off the sheet at once; the host's event follows.
        self.pairing_requests.remove(0);
        self.call("decidePairing", body, After::PairingDecided);
    }

    /// Known to be in no account while it could join one (the host's `canJoinAccount`): phones reach it only over
    /// Wi-Fi or Tailscale until it joins.
    pub fn outside_account(&self) -> bool {
        self.cloud["canJoinAccount"] == true
    }
}

/// `y` lets the phone in, joining its account when that's possible, `w` lets it in with the computer out of an account,
/// where that's a choice, and `n` turns it away.
fn pairing_answer(key: KeyCode, request: &Value) -> Option<Answer> {
    let can_join = request["canJoin"] == true;
    match key {
        KeyCode::Char('y') => Some(if can_join { Answer::Join } else { Answer::PhoneOnly }),
        KeyCode::Char('w') if can_join && request["phoneOnly"] == true => Some(Answer::PhoneOnly),
        KeyCode::Char('n') => Some(Answer::Decline),
        _ => None,
    }
}

fn requests(v: &Value) -> Vec<Value> {
    v["requests"].as_array().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::super::test_app;
    use super::*;

    #[tokio::test]
    async fn a_phone_asking_opens_a_sheet_that_goes_with_the_question() {
        let mut app = test_app();
        let is_question = |app: &App| matches!(app.overlays.last(), Some(Overlay::Pair(PairCode::QuestionOnly)));
        app.pairing_requests_changed(&json!({"requests": [{"requestId": "a", "canJoin": true}]}));
        assert!(is_question(&app), "with no sheet open, Add to account would go unseen");
        app.pairing_requests_changed(&json!({"requests": []}));
        assert!(app.overlays.is_empty());

        app.overlays.push(Overlay::Pair(PairCode::Loading));
        app.pairing_requests_changed(&json!({"requests": [{"requestId": "a", "canJoin": true}]}));
        assert_eq!(app.overlays.len(), 1, "an open sheet shows it already");
        app.pairing_requests_changed(&json!({"requests": []}));
        assert_eq!(app.overlays.len(), 1, "and stays for its code");
    }

    #[test]
    fn a_key_means_only_what_the_phone_was_offered() {
        let both = json!({"canJoin": true, "phoneOnly": true});
        let has_to_join = json!({"canJoin": true, "phoneOnly": false});
        let signed_out = json!({"canJoin": false, "phoneOnly": true});
        let (y, w, n) = (KeyCode::Char('y'), KeyCode::Char('w'), KeyCode::Char('n'));
        assert_eq!(pairing_answer(y, &both), Some(Answer::Join));
        assert_eq!(pairing_answer(w, &both), Some(Answer::PhoneOnly));
        assert_eq!(pairing_answer(w, &has_to_join), None);
        assert_eq!(pairing_answer(y, &signed_out), Some(Answer::PhoneOnly));
        assert_eq!(pairing_answer(w, &signed_out), None);
        assert_eq!(pairing_answer(n, &signed_out), Some(Answer::Decline));
    }

    #[tokio::test]
    async fn waiting_phones_and_the_account_follow_the_host() {
        let mut app = test_app();
        assert!(!app.outside_account(), "unknown until the host says");
        app.pair_reply(&After::Cloud, Ok(json!({"canJoinAccount": true, "owner": null})));
        assert!(app.outside_account());
        app.pair_reply(&After::Cloud, Ok(json!({"canJoinAccount": false, "owner": {"userId": "user_1"}})));
        assert!(!app.outside_account());

        app.pairing_requests_changed(&json!({"requests": [{"requestId": "a", "deviceName": "Pixel", "code": "123456"},
            {"requestId": "b", "deviceName": "iPhone", "code": "654321"}]}));
        app.decide_pairing(KeyCode::Char('y'));
        assert_eq!(app.pairing_requests.len(), 1, "an answered phone leaves the sheet at once");
        assert_eq!(app.pairing_requests[0]["requestId"], "b");
        app.pair_reply(&After::PairingDecided, Err("The host took too long to answer.".into()));
        assert!(app.hint.is_some(), "a failed answer says why, and the host's list comes again");
        app.pair_reply(&After::PairingRequests, Err("unknown method".into()));
        assert_eq!(app.pairing_requests.len(), 1, "an older host's refusal changes nothing");
        app.decide_pairing(KeyCode::Char('w'));
        assert_eq!(app.pairing_requests.len(), 1, "not offered for this phone: nothing happens");
        app.pairing_requests_changed(&json!({"requests": []}));
        app.decide_pairing(KeyCode::Char('n'));
        assert_eq!(app.pairing_requests, Vec::<Value>::new());
    }
}
