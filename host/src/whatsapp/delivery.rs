//! Routine delivery uses the existing link, pinned to its account and authorization generation.

use crate::hub::Hub;
use crate::routines::Output;
use crate::store::Store;
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Arc;

/// The linked account is still authorized, but nothing has entered the bridge yet.
#[derive(Debug)]
pub(crate) struct Unsent(pub anyhow::Error);

impl std::fmt::Display for Unsent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WhatsApp was not sent: {}", self.0)
    }
}
impl std::error::Error for Unsent {}

pub(super) fn unsent(error: anyhow::Error, bound: bool) -> anyhow::Error {
    if bound { anyhow::Error::new(Unsent(error)) } else { error }
}

pub(crate) fn matches(store: &Store, account: &str, expected: &Value) -> Result<bool> {
    let _configuration = crate::LockExt::locked(&store.connector_lock);
    matches_locked(store, account, expected)
}

/// Caller holds the connector lock through validation and receipt publication.
pub(super) fn matches_locked(store: &Store, account: &str, expected: &Value) -> Result<bool> {
    let current = store.kv_read("whatsapp.link.account")?;
    let epoch = store.kv_read("whatsapp.link.generation")?;
    Ok(!account.is_empty()
        && current.as_deref() == Some(account)
        && epoch.as_deref().is_some_and(|epoch| !epoch.is_empty())
        && expected == &json!([current, epoch]))
}

pub(crate) fn authorized(hub: &Hub, output: &Output, event: &Value) -> Result<bool> {
    matches(&hub.store, output.account.as_deref().unwrap_or_default(), &event["whatsappBinding"])
}

pub(crate) fn connected(hub: &Hub, account: Option<&str>) -> bool {
    hub.whatsapp.state.try_lock().is_ok_and(|state| state.state == "connected" && state.me.as_deref() == account)
}

pub(crate) async fn send(hub: &Arc<Hub>, output: &Output, event: &Value, text: &str) -> Result<()> {
    hub.whatsapp
        .send_bound_text(
            &hub.store,
            output.account.as_deref().unwrap_or_default(),
            &event["whatsappBinding"],
            output.target.trim(),
            text,
        )
        .await
}

pub(crate) fn jid(value: &str) -> bool {
    let Some((id, domain)) = value.split_once('@') else { return false };
    !id.is_empty()
        && id.len() <= 80
        && id.split('-').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && (domain != "s.whatsapp.net" || !id.contains('-'))
        && matches!(domain, "s.whatsapp.net" | "g.us")
}
