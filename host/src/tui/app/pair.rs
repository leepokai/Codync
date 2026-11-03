//! The pair-a-phone sheet: its code, whether the computer is in an account, and the phones waiting for it to
//! let them in, which brings it into their account (docs/plans/require-sign-in.md, phase 2).

use serde_json::{Value, json};

use super::{After, App, Overlay, PairCode};

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
            (After::PairingRequests, Ok(v)) => self.pairing_requests = requests(&v),
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

    /// A `pairingRequests` event: the whole list, each time it changes.
    pub(super) fn pairing_requests_changed(&mut self, v: &Value) {
        self.pairing_requests = requests(v);
    }

    /// `y` / `n` on the sheet: the first waiting phone gets in (and the computer joins its account), or doesn't.
    pub(super) fn decide_pairing(&mut self, approve: bool) {
        if self.pairing_requests.is_empty() {
            return;
        }
        // Off the sheet at once; the host's event follows.
        let request = self.pairing_requests.remove(0);
        let answer = json!({"requestId": request["requestId"], "approve": approve});
        self.call("decidePairing", answer, After::PairingDecided);
    }

    /// Known to be in no account while it could join one (the host's `canJoinAccount`): a signed-in phone that
    /// scans the code asks to bring the computer into its own.
    pub fn outside_account(&self) -> bool {
        self.cloud["canJoinAccount"] == true
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
    async fn waiting_phones_and_the_account_follow_the_host() {
        let mut app = test_app();
        assert!(!app.outside_account(), "unknown until the host says");
        app.pair_reply(&After::Cloud, Ok(json!({"canJoinAccount": true, "owner": null})));
        assert!(app.outside_account());
        app.pair_reply(&After::Cloud, Ok(json!({"canJoinAccount": false, "owner": {"userId": "user_1"}})));
        assert!(!app.outside_account());

        app.pairing_requests_changed(&json!({"requests": [{"requestId": "a", "deviceName": "Pixel", "code": "123456"},
            {"requestId": "b", "deviceName": "iPhone", "code": "654321"}]}));
        app.decide_pairing(true);
        assert_eq!(app.pairing_requests.len(), 1, "an answered phone leaves the sheet at once");
        assert_eq!(app.pairing_requests[0]["requestId"], "b");
        app.pair_reply(&After::PairingDecided, Err("The host took too long to answer.".into()));
        assert!(app.hint.is_some(), "a failed answer says why, and the host's list comes again");
        app.pair_reply(&After::PairingRequests, Err("unknown method".into()));
        assert_eq!(app.pairing_requests.len(), 1, "an older host's refusal changes nothing");
        app.pairing_requests_changed(&json!({"requests": []}));
        app.decide_pairing(false);
        assert_eq!(app.pairing_requests, Vec::<Value>::new());
    }
}
