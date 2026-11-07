//! The pairing channel (§4.1): one `pair`, answered, then closed with `4100`. A signed-in phone proves its account with
//! a certificate from the cloud (docs/plans/require-sign-in.md, phase 2); a signed-out phone sends none
//! (docs/plans/optional-sign-in.md). A computer in an account lets in phones of that account only; one in no account
//! asks first, and joins the phone's account only when it's told to.

use super::*;
use crate::analytics::PairedAccount;
use crate::remote::certificate::{self, Certified, Refusal};
use crate::remote::cloud::{self, Owner, RelayKind};
use crate::remote::owner;
use crate::remote::pairing_requests::{self, Answer, Question};

const PROTOCOL: CloseWith = (close::PROTOCOL, "protocolError");
const EXPIRED: &str = "This pairing code expired. Show a new one on the computer.";
const IN_AN_ACCOUNT: &str = "This computer is in a Codync account. Sign in to that account to connect.";
const SIGN_IN_FOR_RELAY: &str =
    "Sign in to pair over the internet, or connect to the same Wi-Fi or Tailscale as the computer.";
const TRY_AGAIN: &str = "Couldn't add this computer to your account. Scan the code again.";

/// The phone's `pair`, checked.
struct Request<'a> {
    id: u64,
    name: &'a str,
    platform: &'a str,
    code: &'a str,
    account: Option<&'a Certified>,
    /// The claim the phone made for its account: `(claimId, nonce)`.
    claim: Option<(&'a str, &'a str)>,
}

/// What the phone's let in with: the computer's answer, if it asked, and the claim it signed, if it joins.
type LetIn = (Option<Answer>, Option<Value>);

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
        kind: &ChannelKind,
    ) -> CloseWith {
        let (id, b) = match read_pair(incoming, opener).await {
            Ok(request) => request,
            Err(close) => return close,
        };
        let Some((name, platform)) = read_identity(&b) else {
            self.send(err(id, 400, "`name` and `platform` (ios, macos or android) are required")).await;
            return PROTOCOL;
        };
        let code = b["code"].as_str().unwrap_or_default();
        let on_codync_relay = self.on_codync_relay(kind);
        let account = match self.admit(id, &b, code, &kind.source(), on_codync_relay).await {
            Ok(account) => account,
            Err(close) => return close,
        };
        let claim = b["claim"]["claimId"].as_str().zip(b["claim"]["nonce"].as_str());
        let request = Request { id, name, platform, code, account: account.as_ref(), claim };
        let (answer, claim) = match self.let_in_or_ask(&request, on_codync_relay, incoming).await {
            Ok(let_in) => let_in,
            Err(close) => return close,
        };
        tracing::info!(device = self.key, platform, "device paired");
        let paired = PairedAccount::of(account.is_some(), answer);
        crate::analytics::device_paired(&self.hub, crate::analytics::Via::Qr, platform, false, paired);
        let mut ok = json!({"computerId": self.hub.identity.computer_id(), "name": crate::service::host_name()});
        if let Some(claim) = claim {
            // Joining the phone's account: the phone, which holds the account's session, completes the claim.
            ok["claim"] = claim;
        }
        self.send(json!({"id": id, "ok": ok})).await;
        (close::PAIRED, "paired")
    }

    /// Lets the phone in, asking this computer first when it's in no account. A join's claim is signed before the
    /// code is spent, so a failure leaves the code for another try.
    async fn let_in_or_ask(
        &self,
        request: &Request<'_>,
        on_codync_relay: bool,
        incoming: &mut mpsc::Receiver<Value>,
    ) -> Result<LetIn, CloseWith> {
        let mut let_in = owner::while_unchanged(&self.hub, || self.let_in(request, None));
        let (mut answer, mut claim) = (None, None);
        if matches!(let_in, Err(Kept::Unasked)) {
            // Over the Codync relay, joining is the only way in, and joining takes a claim.
            if on_codync_relay && request.claim.is_none() {
                return Err(self.refuse(request.id, TRY_AGAIN, "unauthorized").await);
            }
            let about = Question::about(&self.hub, &self.key, request.name, request.platform, request.account)
                .map_err(|_| PROTOCOL)?;
            let can_join = request.account.is_some() && request.claim.is_some() && cloud::codync_url(&self.hub).is_ok();
            let question = Question { can_join, phone_only: !on_codync_relay, ..about };
            let said = self.ask(request.id, question, incoming).await?;
            if said == Answer::Join {
                claim = Some(self.sign_claim(request).await?);
            }
            answer = Some(said);
            let_in = owner::while_unchanged(&self.hub, || self.let_in(request, answer));
        }
        match let_in {
            Ok(()) => Ok((answer, claim)),
            Err(Kept::Out(message, reason)) => Err(self.refuse(request.id, &message, reason).await),
            // `Unasked` only comes before the computer asked.
            Err(Kept::Unsaved | Kept::Unasked) => {
                self.send(err(request.id, 500, "couldn't save this device")).await;
                Err((close::NORMAL, ""))
            }
        }
    }

    /// Whether this channel came through the Codync relay, which a phone may use only with an account.
    fn on_codync_relay(&self, kind: &ChannelKind) -> bool {
        matches!(kind, ChannelKind::Relay { .. })
            && cloud::current(&self.hub.store).is_some_and(|address| address.kind == RelayKind::Codync)
    }

    /// Before anything about this computer's account reaches the phone: a live code, then the phone's account, if it
    /// has one.
    async fn admit(
        &self,
        id: u64,
        b: &Value,
        code: &str,
        source: &str,
        on_codync_relay: bool,
    ) -> Result<Option<Certified>, CloseWith> {
        if !self.hub.pairing.locked().attempt(source) {
            return Err(self.reject(RejectCode::RateLimited).await);
        }
        if !self.hub.pairing.locked().valid(code) {
            return Err(self.refuse(id, EXPIRED, "unauthorized").await);
        }
        match certificate::check(&b["certificate"], &self.key, now_ms()) {
            Ok(account) => Ok(Some(account)),
            Err(Refusal::Missing) if on_codync_relay => Err(self.refuse(id, SIGN_IN_FOR_RELAY, "unauthorized").await),
            Err(Refusal::Missing) => Ok(None),
            Err(Refusal::Invalid) => {
                let message = "Couldn't confirm your Codync account. Sign in again, then scan a new code.";
                Err(self.refuse(id, message, "unauthorized").await)
            }
        }
    }

    /// Run under `owner::while_unchanged`, so the account the phone is checked against is the one it is saved
    /// under: a computer in an account lets in phones of that account only, one in no account only once it asked
    /// (`answer`). The code is spent only now: a phone turned away leaves it to the right one, and while the computer
    /// asks the relay keeps the offer, and with it this socket.
    fn let_in(&self, request: &Request<'_>, answer: Option<Answer>) -> Result<(), Kept> {
        let owner = self.owner_for(request.account).map_err(|message| Kept::Out(message, "otherAccount"))?;
        if owner.is_none() && answer.is_none() {
            return Err(Kept::Unasked);
        }
        if !self.hub.pairing.locked().redeem(request.code) {
            return Err(Kept::Out(EXPIRED.into(), "unauthorized"));
        }
        let device = Device {
            key: self.key.clone(),
            name: request.name.to_owned(),
            platform: request.platform.to_owned(),
            source: DeviceSource::Local,
            grant_id: None,
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: None,
            created_at: now_ms(),
            last_seen_at: None,
            account: request.account.map_or(DeviceAccount::SignedOut, |a| DeviceAccount::Of(a.user_id.clone())),
        };
        if let Err(e) = self.hub.store.put_device(&device) {
            tracing::error!(error = format!("{e:#}"), "couldn't save a paired device");
            return Err(Kept::Unsaved);
        }
        Ok(())
    }

    /// This computer's account, or what to tell a phone of another account, or one with none.
    fn owner_for(&self, account: Option<&Certified>) -> Result<Option<Owner>, String> {
        let owner = owner::owner(&self.hub.store);
        let Some(account) = account else {
            return if owner.is_some() { Err(IN_AN_ACCOUNT.to_owned()) } else { Ok(None) };
        };
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
        question: Question,
        incoming: &mut mpsc::Receiver<Value>,
    ) -> Result<Answer, CloseWith> {
        let answer = tokio::select! {
            answer = pairing_requests::ask(&self.hub, question) => answer,
            // Nothing else comes on a pairing channel: its end means the phone gave up.
            () = async { while incoming.recv().await.is_some() {} } => return Err((close::NORMAL, "")),
        };
        if answer == Answer::Decline {
            return Err(self.refuse(id, "The computer didn't let this phone in.", "declined").await);
        }
        Ok(answer)
    }

    /// The computer said to join the phone's account: its signature on the phone's claim.
    async fn sign_claim(&self, request: &Request<'_>) -> Result<Value, CloseWith> {
        let (Some((claim_id, nonce)), Some(account)) = (request.claim, request.account) else {
            return Err(self.refuse(request.id, TRY_AGAIN, "unauthorized").await);
        };
        match owner::sign_claim(&self.hub, claim_id, nonce, &account.user_id).await {
            Ok(signed) => Ok(signed),
            Err(e) => {
                tracing::warn!(error = format!("{e:#}"), "couldn't sign a pairing phone's claim");
                Err(self.refuse(request.id, TRY_AGAIN, "unauthorized").await)
            }
        }
    }

    async fn refuse(&self, id: u64, message: &str, reason: &'static str) -> CloseWith {
        self.send(err(id, 403, message)).await;
        (close::UNAUTHORIZED, reason)
    }
}

/// The phone's name and platform, when they're usable.
fn read_identity(b: &Value) -> Option<(&str, &str)> {
    let name = b["name"].as_str().map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 100)?;
    let platform = b["platform"].as_str().filter(|p| matches!(*p, "ios" | "macos" | "android"))?;
    Some((name, platform))
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
