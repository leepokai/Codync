//! Remote edits keep bot references, but credentials belong to one endpoint revision.

use super::{Connector, connector_update, connectors, oauth, passwords, update_connectors};
use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::collections::BTreeMap;

pub async fn update_connector(store: &crate::store::Store, b: &Value) -> Result<Value> {
    let id = connector_update::text(b, "id")?.filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`id` is required"))?;
    let before = connectors(store)?.into_iter().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
    if before.registry_name.is_some() {
        bail!("This connector came from the MCP registry; use its setup inputs when reinstalling");
    }
    if before.command.is_some() {
        if b.get("url").is_some() || b.get("headers").is_some() {
            bail!("A local connector accepts command, args and env, not URL or headers");
        }
        return connector_update::update_custom_connector(store, b);
    }
    let mut edited = prepare(&before, b)?;
    if edited.url != before.url {
        let mut headers = BTreeMap::new();
        for (key, value) in &edited.headers {
            headers.insert(key.clone(), passwords::resolve(store, value).await?);
        }
        if oauth::required(edited.url.as_deref().ok_or_else(|| anyhow!("Missing remote URL"))?, &headers).await {
            edited.oauth = Some(oauth::OAuth::default());
        }
    }
    update_connectors(store, |all| {
        let current = all.iter_mut().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
        // Discovery can take time. Never overwrite a concurrent credential refresh or edit.
        if current != &before {
            bail!("The connector changed during this edit. Read it again and retry.");
        }
        *current = edited;
        Ok(current.public())
    })
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
        edited.auth_revision = uuid::Uuid::new_v4().to_string();
    }
    if let Some(headers) = b.get("headers") {
        merge_headers(&mut edited, headers)?;
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
            Value::String(s) if s.len() <= 64 * 1024 => {
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
