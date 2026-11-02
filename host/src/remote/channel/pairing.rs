//! The pairing channel (§4.1): one `pair`, answered, then closed with `4100`. The phone proves its account with a
//! certificate from the cloud (docs/plans/require-sign-in.md, phase 2): a computer in an account lets in phones of
//! that account only; one in no account asks first, then joins the phone's account.

use super::*;
use crate::remote::certificate::{self, Certified, Refusal};
use crate::remote::cloud::Owner;
use crate::remote::{owner, pairing_requests};

const PROTOCOL: CloseWith = (close::PROTOCOL, "protocolError");
const EXPIRED: &str = "This pairing code expired. Show a new one on the computer.";

/// Why `let_in` left the phone out.
enum Kept {
    /// The computer is in no account and hasn't asked its user yet.
    Unasked,
    /// Turned away: the message and the close reason.
    Out(String, &'static str),
    /// Let in, but it couldn't be saved.
    Unsaved,
}

impl Channel {
    pub(super) async fn pair(
        &mut self,
        incoming: &mut mpsc::Receiver<Value>,
        opener: Opener,
        source: &str,
    ) -> CloseWith {
        let (id, b) = match read_pair(incoming, opener).await {
            Ok(request) => request,
            Err(close) => return close,
        };
        let name = b["name"].as_str().map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 100);
        let platform = b["platform"].as_str().filter(|p| matches!(*p, "ios" | "macos" | "android"));
        let (Some(name), Some(platform)) = (name, platform) else {
            self.send(err(id, 400, "`name` and `platform` (ios, macos or android) are required")).await;
            return PROTOCOL;
        };
        let code = b["code"].as_str().unwrap_or_default();
        let account = match self.admit(id, &b, code, source).await {
            Ok(account) => account,
            Err(close) => return close,
        };
        let mut let_in = owner::while_unchanged(&self.hub, || self.let_in(&account, false, code, name, platform));
        if matches!(let_in, Err(Kept::Unasked)) {
            if let Err(close) = self.ask(id, name, platform, account.email.as_deref(), incoming).await {
                return close;
            }
            let_in = owner::while_unchanged(&self.hub, || self.let_in(&account, true, code, name, platform));
        }
        let owner = match let_in {
            Ok(owner) => owner,
            Err(Kept::Out(message, reason)) => return self.refuse(id, &message, reason).await,
            // `Unasked` only comes before the computer asked.
            Err(Kept::Unsaved | Kept::Unasked) => {
                self.send(err(id, 500, "couldn't save this device")).await;
                return (close::NORMAL, "");
            }
        };
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

    /// Before anything about this computer's account reaches the phone: a live code, then the phone's account.
    async fn admit(&self, id: u64, b: &Value, code: &str, source: &str) -> Result<Certified, CloseWith> {
        if !self.hub.pairing.locked().attempt(source) {
            return Err(self.reject(RejectCode::RateLimited).await);
        }
        if !self.hub.pairing.locked().valid(code) {
            return Err(self.refuse(id, EXPIRED, "unauthorized").await);
        }
        match certificate::check(&b["certificate"], &self.key, now_ms()) {
            Ok(account) => Ok(account),
            Err(Refusal::Missing) => Err(self.reject(RejectCode::UnsupportedVersion).await),
            Err(Refusal::Invalid) => {
                let message = "Couldn't confirm your Codync account. Sign in again, then scan a new code.";
                Err(self.refuse(id, message, "unauthorized").await)
            }
        }
    }

    /// Run under `owner::while_unchanged`, so the account the phone is checked against is the one it is saved
    /// under: a computer in an account lets in phones of that account only, one in no account only once it asked
    /// (`asked`). Returns this computer's account. The code is spent only now: a phone turned away leaves it to the
    /// right one, and while the computer asks the relay keeps the offer, and with it this socket.
    fn let_in(
        &self,
        account: &Certified,
        asked: bool,
        code: &str,
        name: &str,
        platform: &str,
    ) -> Result<Option<Owner>, Kept> {
        let owner = self.owner_for(account).map_err(|message| Kept::Out(message, "otherAccount"))?;
        if owner.is_none() && !asked {
            return Err(Kept::Unasked);
        }
        if !self.hub.pairing.locked().redeem(code) {
            return Err(Kept::Out(EXPIRED.into(), "unauthorized"));
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
            return Err(Kept::Unsaved);
        }
        Ok(owner)
    }

    /// This computer's account, or what to tell a phone of another account.
    fn owner_for(&self, account: &Certified) -> Result<Option<Owner>, String> {
        let owner = owner::owner(&self.hub.store);
        if let Some(owner) = owner.as_ref().filter(|o| o.user_id != account.user_id) {
            let whose =
                owner.email.as_deref().map_or_else(|| "another".into(), |e| format!("{}'s", certificate::masked(e)));
            return Err(format!(
                "This computer is in {whose} account. Move it to your account in Codync on that computer, or sign in with that account."
            ));
        }
        Ok(owner)
    }

    /// A computer in no account asks its user before the phone comes in; a phone that leaves meanwhile takes the
    /// question off the computer's screens.
    async fn ask(
        &self,
        id: u64,
        name: &str,
        platform: &str,
        email: Option<&str>,
        incoming: &mut mpsc::Receiver<Value>,
    ) -> Result<(), CloseWith> {
        let Ok(device_key) = crypto::unb64_n::<32>(&self.key) else { return Err(PROTOCOL) };
        let code = crypto::pair_check_code(&device_key, &self.hub.identity.sign_pub());
        let let_in = tokio::select! {
            let_in = pairing_requests::ask(&self.hub, name, platform, email, code) => let_in,
            // Nothing else comes on a pairing channel: its end means the phone gave up.
            () = async { while incoming.recv().await.is_some() {} } => return Err((close::NORMAL, "")),
        };
        if let_in { Ok(()) } else { Err(self.refuse(id, "The computer didn't let this phone in.", "declined").await) }
    }

    /// The signed claim for `{claimId, nonce}`, or null without one, or with the Codync cloud off or a private
    /// relay here (no account to join).
    async fn claim(&self, claim: &Value, user_id: &str) -> Value {
        let (Some(claim_id), Some(nonce)) = (claim["claimId"].as_str(), claim["nonce"].as_str()) else {
            return Value::Null;
        };
        if crate::remote::cloud::codync_url(&self.hub).is_err() {
            return Value::Null;
        }
        owner::sign_claim(&self.hub, claim_id, nonce, user_id).await.unwrap_or_else(|e| {
            tracing::info!(error = format!("{e:#}"), "paired without joining the account");
            Value::Null
        })
    }

    async fn refuse(&self, id: u64, message: &str, reason: &'static str) -> CloseWith {
        self.send(err(id, 403, message)).await;
        (close::UNAUTHORIZED, reason)
    }
}

/// Waits for the phone's one `pair`: its id and body.
async fn read_pair(incoming: &mut mpsc::Receiver<Value>, mut opener: Opener) -> Result<(u64, Value), CloseWith> {
    let deadline = Instant::now() + PAIRING_MAX_AGE;
    let bytes = loop {
        let msg = tokio::select! {
            msg = incoming.recv() => msg,
            () = sleep_until(deadline) => return Err((close::PAIRING_EXPIRED, "pairingExpired")),
        };
        let Some(msg) = msg else { return Err((close::NORMAL, "")) };
        match opener.open(&msg) {
            Ok(Some(bytes)) => break bytes,
            Ok(None) => {}
            Err(e) => return Err(frame_close(&e)),
        }
    };
    let Ok(mut v) = serde_json::from_slice::<Value>(&bytes) else { return Err(PROTOCOL) };
    match v["id"].as_u64() {
        Some(id) if v["m"] == "pair" => Ok((id, v["b"].take())),
        _ => Err(PROTOCOL),
    }
}
