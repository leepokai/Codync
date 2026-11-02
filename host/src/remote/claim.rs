//! Putting this computer in a Codync account: the signed claim, and the one-time ticket a cloud
//! computer Codync runs for a signed-in user finds on its first boot (`managed.json` in the data
//! folder, written by the provider before the machine starts). The ticket names the claim; the
//! host signs it and completes it itself, then deletes the file.

use crate::hub::Hub;
use crate::remote::cloud::{self, Approval, CloudError};
use crate::remote::crypto;
use anyhow::{Context, Result, bail};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const TICKET_FILE: &str = "managed.json";

/// What Codync's cloud left for this machine before it first started.
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Ticket {
    claim_id: String,
    nonce: String,
    user_id: String,
    #[serde(default)]
    approval: Option<Approval>,
}

/// The fields of a claim signed by this computer, for `/v1/claims/{id}/complete` or `/v1/host/claim`.
pub async fn payload(hub: &Arc<Hub>, claim_id: &str, nonce: &str, user_id: &str) -> Result<Value> {
    for (k, v) in [("claimId", claim_id), ("nonce", nonce), ("userId", user_id)] {
        // Fields are newline-separated in the signed string: a newline would forge another field.
        if v.is_empty() || v.contains('\n') {
            bail!("`{k}` is invalid");
        }
    }
    let computer_id = hub.identity.computer_id();
    let box_key = hub.identity.box_pub_b64();
    let input = crypto::claim_input(claim_id, nonce, user_id, &computer_id, &box_key);
    Ok(json!({
        "computerId": computer_id,
        "signKey": hub.identity.sign_pub_b64(),
        "boxKey": box_key,
        "name": crate::service::host_name(),
        "platform": std::env::consts::OS,
        "device": tokio::task::spawn_blocking(crate::service::device).await?,
        "version": env!("CARGO_PKG_VERSION"),
        "sig": crypto::b64(&hub.identity.sign(input.as_bytes())),
    }))
}

fn ticket_path() -> PathBuf {
    crate::service::data_dir().join(TICKET_FILE)
}

fn parse(bytes: &[u8]) -> Result<Ticket> {
    serde_json::from_slice(bytes).context("managed.json is not a valid claim ticket")
}

/// Refusals that retrying can't fix: the ticket is spent either way.
fn is_final(e: &anyhow::Error) -> bool {
    e.downcast_ref::<CloudError>().is_some_and(|c| {
        matches!(
            c.code.as_str(),
            "claimExpired" | "alreadyClaimed" | "notFound" | "badSignature" | "forbidden" | "badRequest"
        )
    })
}

/// Background task: completes the claim in `managed.json` once the cloud knows this computer.
pub async fn run_ticket(hub: Arc<Hub>) {
    let path = ticket_path();
    let Ok(bytes) = tokio::fs::read(&path).await else { return };
    let ticket = match parse(&bytes) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = format!("{e:#}"), "ignoring the claim ticket");
            remove(&path).await;
            return;
        }
    };
    if let Some(approval) = ticket.approval
        && let Err(e) = cloud::set_approval(&hub, approval)
    {
        tracing::warn!(error = format!("{e:#}"), "couldn't set account approval from the claim ticket");
    }
    // The ticket is deleted after the claim; the host keeps knowing it is a cloud computer.
    if let Err(e) = super::managed::set_managed(&hub.store) {
        tracing::warn!(error = format!("{e:#}"), "couldn't mark this computer as a cloud computer");
    }
    let mut wait = Duration::from_secs(1);
    loop {
        // Registering is the relay's job; checking for it is a local read, so it gets no backoff.
        if !hub.cloud.status().registered {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        match complete(&hub, &ticket).await {
            Ok(()) => {
                tracing::info!(claim_id = ticket.claim_id, "claimed this computer for its Codync account");
                remove(&path).await;
                return;
            }
            Err(e) if is_final(&e) => {
                tracing::warn!(error = format!("{e:#}"), claim_id = ticket.claim_id, "the claim ticket was refused");
                remove(&path).await;
                return;
            }
            Err(e) => tracing::warn!(error = format!("{e:#}"), "couldn't complete the claim yet"),
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(Duration::from_mins(1));
    }
}

/// `POST /v1/host/claim`: the cloud checks the claim and this computer's signature, then sets the owner.
async fn complete(hub: &Arc<Hub>, ticket: &Ticket) -> Result<()> {
    let mut body = payload(hub, &ticket.claim_id, &ticket.nonce, &ticket.user_id).await?;
    body["claimId"] = json!(ticket.claim_id);
    body["userId"] = json!(ticket.user_id);
    let base = cloud::codync_url(hub)?;
    cloud::call(hub, &base, Method::POST, "/v1/host/claim", Some(&body)).await?;
    Ok(())
}

async fn remove(path: &PathBuf) {
    if let Err(e) = tokio::fs::remove_file(path).await {
        tracing::warn!(error = %e, "couldn't delete the claim ticket");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_ticket_and_classifies_refusals() {
        let t = parse(br#"{"claimId":"clm_a","nonce":"n","userId":"user_1","approval":"auto"}"#).unwrap();
        assert_eq!(t.approval, Some(Approval::Auto));
        assert!(parse(br#"{"claimId":"clm_a"}"#).is_err());
        let refused =
            |code: &str| anyhow::Error::new(CloudError { status: 409, code: code.into(), message: String::new() });
        assert!(is_final(&refused("claimExpired")));
        assert!(!is_final(&refused("internal")));
        assert!(!is_final(&anyhow::anyhow!("the Codync cloud is unreachable")));
    }
}
