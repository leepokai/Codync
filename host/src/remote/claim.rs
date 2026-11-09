//! Putting a cloud computer in its Codync account: the one-time ticket a cloud computer Codync runs
//! for a signed-in user finds on its first boot (`managed.json` in the data
//! folder, written by the provider before the machine starts). The ticket names the claim; the
//! host signs it (`owner::sign_claim`) and completes it itself, then deletes the file. The claim also names the
//! computer's keeper (`crate::keeper`), the only one the cloud then gives its vault key to: it waits for the keeper
//! rather than claim without it.

use crate::hub::Hub;
use crate::remote::cloud::{self, Approval, CloudError};
use anyhow::{Context, Result};
use reqwest::Method;
use serde::Deserialize;
use serde_json::json;
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
                rename(&hub).await;
                // A first boot: this host started before it knew it was a cloud computer, and its keeper gets its key
                // only now that the claim named it.
                tokio::spawn(super::managed::on_start(hub));
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

/// `POST /v1/host/claim`: the cloud checks the claim and this computer's signature, then sets the owner, and
/// remembers the keeper's key.
async fn complete(hub: &Arc<Hub>, ticket: &Ticket) -> Result<()> {
    let keeper = {
        let hub = hub.clone();
        tokio::task::spawn_blocking(move || crate::keeper::client::public_key(&hub.store))
            .await?
            .context("waiting for this computer's keeper")?
    };
    let mut body = super::owner::sign_claim(hub, &ticket.claim_id, &ticket.nonce, &ticket.user_id).await?;
    body["claimId"] = json!(ticket.claim_id);
    body["userId"] = json!(ticket.user_id);
    body["keeperKey"] = json!(keeper);
    let base = cloud::codync_url(hub)?;
    let done = cloud::call(hub, &base, Method::POST, "/v1/host/claim", Some(&body)).await?;
    // The name its user gave it, at once: until then it is only "Codync Cloud", like every other cloud computer.
    if let Some(name) = done["computer"]["name"].as_str().filter(|n| !n.trim().is_empty())
        && let Err(e) = super::managed::set_name(hub, name)
    {
        tracing::warn!(error = format!("{e:#}"), "couldn't take this cloud computer's name");
    }
    Ok(())
}

/// The account keeps the icon the computer registered with, which may predate it knowing it is a cloud computer:
/// registering again gives it a cloud computer's (the cloud keeps the name its user gave the machine).
async fn rename(hub: &Hub) {
    let Some(address) = cloud::current(&hub.store) else { return };
    if let Err(e) = cloud::register(hub, &address).await {
        tracing::warn!(error = format!("{e:#}"), "couldn't give the account this cloud computer's name");
    }
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
