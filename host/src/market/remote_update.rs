//! Remote edits keep bot references, but credentials belong to one endpoint revision.

use super::{Connector, connector_update, connectors, oauth, passwords, update_connectors};
use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::collections::BTreeMap;

/// Local edits keep their check-before-save contract. Remote connection edits must be
/// saved first so OAuth and the proxy can address the new endpoint by its existing ID.
pub async fn update_connector(store: &crate::store::Store, id: &str, b: &Value, port: u16) -> Result<Value> {
    use connector_update::EditFailure;
    let before = connectors(store)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| EditFailure::Invalid.error("Unknown connector"))?;
    if before.registry_name.is_some() {
        return Err(EditFailure::Refused
            .error("This connector came from the MCP registry; use its setup inputs when reinstalling"));
    }
    if before.command.is_some() {
        if b.get("url").is_some() || b.get("headers").is_some() {
            return Err(
                EditFailure::Invalid.error("A local connector accepts command, args and env, not URL or headers")
            );
        }
        return connector_update::update_custom_connector(store, id, b, port).await;
    }
    let (saved, needs_check) = save_remote(store, &before, b).await?;
    if needs_check {
        checked_remote(store, &saved, super::verify::verify(store, id, port)).await
    } else {
        Ok(serde_json::json!({"connector": saved.public(), "verify": null}))
    }
}

/// The check resolves its target by ID. Fence its result with the saved definition so
/// another edit cannot supply the readiness result reported for this one.
async fn checked_remote(
    store: &crate::store::Store,
    saved: &Connector,
    check: impl Future<Output = Result<Value>>,
) -> Result<Value> {
    current_connection(store, saved)?;
    let check = check.await;
    let current = current_connection(store, saved)?;
    let check = check.map_err(|e| connector_update::EditFailure::WontStart.error(e))?;
    Ok(serde_json::json!({"connector": current.public(), "verify": check}))
}

fn current_connection(store: &crate::store::Store, saved: &Connector) -> Result<Connector> {
    let conflict = || {
        connector_update::EditFailure::Conflict
            .error("The connector changed while it was being checked. Read it again and retry.")
    };
    let current = connectors(store)?.into_iter().find(|c| c.id == saved.id).ok_or_else(conflict)?;
    // Refreshing a grant may rotate tokens and expiry; a different registration or sign-out
    // is not that refresh. Every other saved field, including the revision, must still match.
    let mut expected = saved.clone();
    expected.oauth.clone_from(&current.oauth);
    if current != expected || !same_sign_in(saved.oauth.as_ref(), current.oauth.as_ref()) {
        return Err(conflict());
    }
    Ok(current)
}

fn same_sign_in(a: Option<&oauth::OAuth>, b: Option<&oauth::OAuth>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.client_id == b.client_id
                && a.client_secret == b.client_secret
                && a.redirect_uris == b.redirect_uris
                && a.token_endpoint == b.token_endpoint
                && a.signed_in() == b.signed_in()
        }
        _ => false,
    }
}

/// Prepare outside the lock, then save only if no other writer changed the definition.
async fn save_remote(store: &crate::store::Store, before: &Connector, b: &Value) -> Result<(Connector, bool)> {
    use connector_update::EditFailure;
    let mut edited = prepare(before, b).map_err(|e| EditFailure::Invalid.error(e))?;
    // Repeating a URL/header edit after sign-in is also a request to check and refresh.
    let needs_check = b.get("url").is_some() || b.get("headers").is_some();
    if edited.url != before.url {
        let mut headers = BTreeMap::new();
        for (key, value) in &edited.headers {
            headers.insert(key.clone(), passwords::resolve(store, value).await?);
        }
        if oauth::required(edited.url.as_deref().ok_or_else(|| anyhow!("Missing remote URL"))?, &headers).await {
            edited.oauth = Some(oauth::OAuth::default());
        }
    }
    let connector = update_connectors(store, |all| {
        let current = all
            .iter_mut()
            .find(|c| c.id == before.id)
            .ok_or_else(|| EditFailure::Conflict.error("Unknown connector"))?;
        // Discovery can take time. Never overwrite a concurrent credential refresh or edit.
        if current != before {
            return Err(EditFailure::Conflict.error("The connector changed during this edit. Read it again and retry."));
        }
        *current = edited;
        Ok(current.clone())
    })?;
    Ok((connector, needs_check))
}

fn prepare(before: &Connector, b: &Value) -> Result<Connector> {
    if ["command", "args", "env"].iter().any(|key| b.get(key).is_some()) {
        bail!("A remote connector accepts URL and headers, not command, args or env");
    }
    let mut edited = before.clone();
    if let Some(name) = connector_update::text(b, "name")?.filter(|s| !s.is_empty()) {
        edited.name = name;
    }
    if let Some(description) = connector_update::text(b, "description")? {
        edited.description = description;
    }
    if let Some(url) = connector_update::text(b, "url")? {
        validate_url(&url)?;
        edited.url = Some(url);
    }
    let moved = edited.url != before.url;
    if moved {
        if b.get("headers").is_none() {
            bail!("A changed URL requires replacement `headers`; use {{}} to send no saved headers");
        }
        edited.headers.clear();
        edited.oauth = None;
    }
    if let Some(headers) = b.get("headers") {
        merge_headers(&mut edited, headers)?;
    }
    if edited.url != before.url || edited.headers != before.headers {
        edited.auth_revision = uuid::Uuid::new_v4().to_string();
    }
    Ok(edited)
}

fn validate_url(raw: &str) -> Result<()> {
    let url = reqwest::Url::parse(raw).map_err(|_| anyhow!("Invalid remote URL"))?;
    let loopback = url.host_str().is_some_and(|h| {
        h == "localhost" || h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
    });
    if !(url.scheme() == "https" || url.scheme() == "http" && loopback) || url.host_str().is_none() {
        bail!("Use HTTPS, or HTTP on a loopback address");
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        bail!("Remote URLs cannot contain user information or a fragment");
    }
    Ok(())
}

fn merge_headers(c: &mut Connector, value: &Value) -> Result<()> {
    let fields = value.as_object().ok_or_else(|| anyhow!("`headers` must be an object"))?;
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in fields {
        let name =
            reqwest::header::HeaderName::from_bytes(key.as_bytes()).map_err(|_| anyhow!("Invalid header name"))?;
        if !seen.insert(name.as_str().to_owned()) {
            bail!("Header names must be unique ignoring case");
        }
        if name == reqwest::header::AUTHORIZATION && c.oauth.is_some() {
            bail!("OAuth manages Authorization for this connector; use connectorSignIn to renew it");
        }
        let replacement = match value {
            Value::Null => None,
            Value::String(s) if s.len() <= super::MAX_CREDENTIAL_BYTES => {
                reqwest::header::HeaderValue::from_str(s).map_err(|_| anyhow!("Invalid header value"))?;
                Some(s.clone())
            }
            _ => bail!("`headers` values are strings, or null to remove one"),
        };
        c.headers.retain(|stored, _| !stored.eq_ignore_ascii_case(key));
        if let Some(value) = replacement {
            c.headers.insert(key.clone(), value);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "remote_update_tests.rs"]
mod tests;
