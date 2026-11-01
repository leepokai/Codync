//! The pair-a-phone sheet: its code, whether the computer is in an account, and the phones waiting for it to
//! let them in, which brings it into their account (docs/plans/require-sign-in.md, phase 2).

use serde_json::{Value, json};

use super::{After, App, Overlay, PairCode};

impl App {
    pub(super) fn open_pair(&mut self) {
        self.overlays.push(Overlay::Pair(PairCode::Loading));
        self.call("pairing", json!({}), After::Pairing);
        self.call("cloudStatus", json!({}), After::Cloud);
        self.call("pairingRequests", json!({}), After::PairingRequests);
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
                            |url| PairCode::Ready(url.to_owned()),
                        ),
                    };
                }
            }
            (After::Cloud, Ok(v)) => self.cloud = v,
            (After::PairingRequests, Ok(v)) => self.pairing_requests = requests(&v),
            // Hosts before accounts have neither: the sheet shows the code alone.
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
        self.call("decidePairing", json!({"requestId": request["requestId"], "approve": approve}), After::Nothing);
    }

    /// Known to be in no account while it could join one: a signed-in phone that scans the code asks to bring the
    /// computer into its own. With the cloud off or a private relay there is no account to join.
    pub fn outside_account(&self) -> bool {
        self.cloud.is_object()
            && self.cloud["owner"].is_null()
            && self.cloud["enabled"] == true
            && self.cloud["relay"] != "private"
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
        app.pair_reply(&After::Cloud, Ok(json!({"enabled": false, "owner": null})));
        assert!(!app.outside_account(), "no account to join with the cloud off");
        app.pair_reply(&After::Cloud, Ok(json!({"enabled": true, "relay": "private", "owner": null})));
        assert!(!app.outside_account(), "nor on a private relay");
        app.pair_reply(&After::Cloud, Ok(json!({"enabled": true, "relay": "codync", "owner": null})));
        assert!(app.outside_account());
        app.pair_reply(&After::Cloud, Ok(json!({"enabled": true, "owner": {"userId": "user_1"}})));
        assert!(!app.outside_account());

        app.pairing_requests_changed(&json!({"requests": [{"requestId": "a", "deviceName": "Pixel", "code": "123456"},
            {"requestId": "b", "deviceName": "iPhone", "code": "654321"}]}));
        app.decide_pairing(true);
        assert_eq!(app.pairing_requests.len(), 1, "an answered phone leaves the sheet at once");
        assert_eq!(app.pairing_requests[0]["requestId"], "b");
        app.pair_reply(&After::PairingRequests, Err("unknown method".into()));
        assert_eq!(app.pairing_requests.len(), 1, "an older host's refusal changes nothing");
        app.pairing_requests_changed(&json!({"requests": []}));
        app.decide_pairing(false);
        assert_eq!(app.pairing_requests, Vec::<Value>::new());
    }
}
