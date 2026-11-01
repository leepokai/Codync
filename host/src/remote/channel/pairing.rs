//! The pairing channel (§4.1): one `pair`, answered, then closed with `4100`. The phone proves its account with a
//! certificate from the cloud (docs/plans/require-sign-in.md, phase 2): a computer in an account lets in phones of
//! that account only; one in no account asks first, then joins the phone's account.

use super::*;
use crate::remote::certificate::{self, Refusal};
use crate::remote::{owner, pairing_requests};

impl Channel {
    pub(super) async fn pair(
        &mut self,
        incoming: &mut mpsc::Receiver<Value>,
        mut opener: Opener,
        source: &str,
    ) -> CloseWith {
        let protocol = (close::PROTOCOL, "protocolError");
        let deadline = Instant::now() + PAIRING_MAX_AGE;
        let bytes = loop {
            let msg = tokio::select! {
                msg = incoming.recv() => msg,
                () = sleep_until(deadline) => return (close::PAIRING_EXPIRED, "pairingExpired"),
            };
            let Some(msg) = msg else { return (close::NORMAL, "") };
            match opener.open(&msg) {
                Ok(Some(bytes)) => break bytes,
                Ok(None) => {}
                Err(e) => return frame_close(&e),
            }
        };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { return protocol };
        let Some(id) = v["id"].as_u64() else { return protocol };
        if v["m"] != "pair" {
            return protocol;
        }
        let b = &v["b"];
        let name = b["name"].as_str().map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 100);
        let platform = b["platform"].as_str().filter(|p| matches!(*p, "ios" | "macos" | "android"));
        let (Some(name), Some(platform)) = (name, platform) else {
            self.send(err(id, 400, "`name` and `platform` (ios, macos or android) are required")).await;
            return protocol;
        };
        if !self.hub.pairing.locked().attempt(source) {
            return self.reject(RejectCode::RateLimited).await;
        }
        // The code is spent only on a phone that may come in, so a phone of the wrong account doesn't use it up.
        let account = match certificate::check(&b["certificate"], &self.key, now_ms()) {
            Ok(account) => account,
            Err(Refusal::Missing) => return self.reject(RejectCode::UnsupportedVersion).await,
            Err(Refusal::Invalid) => {
                self.send(err(id, 403, "Couldn't confirm your Codync account. Sign in again, then scan a new code."))
                    .await;
                return (close::UNAUTHORIZED, "unauthorized");
            }
        };
        let owner = owner::owner(&self.hub.store);
        if let Some(owner) = owner.as_ref().filter(|o| o.user_id != account.user_id) {
            let whose =
                owner.email.as_deref().map_or_else(|| "another".into(), |e| format!("{}'s", certificate::masked(e)));
            let message = format!(
                "This computer is in {whose} account. Move it to your account in Codync on that computer, or sign in with that account."
            );
            self.send(err(id, 403, &message)).await;
            return (close::UNAUTHORIZED, "otherAccount");
        }
        if !self.hub.pairing.locked().redeem(b["code"].as_str().unwrap_or_default()) {
            self.send(err(id, 403, "This pairing code expired. Show a new one on the computer.")).await;
            return (close::UNAUTHORIZED, "unauthorized");
        }
        if owner.is_none() {
            let Ok(device_key) = crypto::unb64_n::<32>(&self.key) else { return protocol };
            let code = pairing_requests::check_code(&device_key, &self.hub.identity.sign_pub());
            if !pairing_requests::ask(&self.hub, name, platform, account.email.as_deref(), code).await {
                self.send(err(id, 403, "The computer didn't let this phone in.")).await;
                return (close::UNAUTHORIZED, "declined");
            }
        }
        let device = Device {
            key: self.key.clone(),
            name: name.to_owned(),
            platform: platform.to_owned(),
            source: DeviceSource::Local,
            grant_id: None,
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: None,
            created_at: now_ms(),
            last_seen_at: None,
            account: Some(account.user_id.clone()),
        };
        if let Err(e) = self.hub.store.put_device(&device) {
            tracing::error!(error = format!("{e:#}"), "couldn't save a paired device");
            self.send(err(id, 500, "couldn't save this device")).await;
            return (close::NORMAL, "");
        }
        tracing::info!(device = self.key, platform, "device paired");
        crate::analytics::device_paired(&self.hub, crate::analytics::Via::Qr, platform, false);
        let mut ok = json!({"computerId": self.hub.identity.computer_id(), "name": crate::service::host_name()});
        if owner.is_none() {
            // Joining the phone's account: the computer signs the claim the phone made; the phone, which holds
            // the account's session, completes it with the cloud.
            ok["claim"] = self.claim(&b["claim"], &account.user_id).await;
        }
        self.send(json!({"id": id, "ok": ok})).await;
        (close::PAIRED, "paired")
    }

    /// The signed claim for `{claimId, nonce}`, or null without one or with no Codync accounts here.
    async fn claim(&self, claim: &Value, user_id: &str) -> Value {
        if claim.is_null() {
            return Value::Null;
        }
        let request = json!({"claimId": claim["claimId"], "nonce": claim["nonce"], "userId": user_id});
        match api::claim_sign(&self.hub, &request).await {
            Ok(signed) => signed,
            Err(e) => {
                tracing::info!(error = format!("{e:#}"), "paired without joining the account");
                Value::Null
            }
        }
    }

    async fn reject(&self, code: RejectCode) -> CloseWith {
        let _ = self.tx.send(ToDevice::Plain(json!({"t": "reject", "code": code, "message": code.message()}))).await;
        (code.close_code(), if code == RejectCode::RateLimited { "rateLimited" } else { "rejected" })
    }
}
