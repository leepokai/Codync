//! Company sign-in and user sync: an OpenID Connect provider the owner names, checked by discovery, and a bearer token
//! for a SCIM client. Codync has no member accounts of its own yet, so the SCIM endpoint lists the account owner and
//! this computer's paired devices as read-only users.

use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const CONFIG: &str = "enterprise.oidc";
const SCIM: &str = "enterprise.scim";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub issuer: String,
    pub client_id: String,
    #[serde(default)]
    pub domains: Vec<String>,
    /// What discovery answered, kept so the settings can show it without asking again.
    #[serde(default)]
    pub endpoints: Value,
}

fn load(store: &Store) -> Result<Option<Config>> {
    crate::market::vault::read(store, CONFIG)?
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

fn discovery_url(issuer: &str) -> String {
    format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'))
}

/// Checks the issuer answers OIDC discovery, the client id has a sane shape and every domain looks like one.
async fn verify(issuer: &str, client_id: &str, domains: &[String]) -> Result<Value> {
    if !issuer.starts_with("https://") || issuer.len() > 300 || issuer.chars().any(char::is_whitespace) {
        bail!("The issuer must be an https address.");
    }
    if client_id.is_empty() || client_id.len() > 200 || client_id.chars().any(char::is_whitespace) {
        bail!("Enter the client id from the provider.");
    }
    if domains.len() > 20 || domains.iter().any(|d| d.is_empty() || d.len() > 200 || !d.contains('.')) {
        bail!("Use up to 20 email domains, such as example.com.");
    }
    let response = crate::http()
        .get(discovery_url(issuer))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .context("The provider did not answer discovery.")?;
    if !response.status().is_success() {
        bail!("The provider answered {} for its discovery document.", response.status());
    }
    let answer: Value = response.json().await.context("The discovery document is not JSON.")?;
    if answer["issuer"].as_str().is_none()
        || answer["authorization_endpoint"].as_str().is_none()
        || answer["token_endpoint"].as_str().is_none()
    {
        bail!("The discovery document lacks its issuer or endpoints.");
    }
    Ok(json!({
        "issuer": answer["issuer"],
        "authorizationEndpoint": answer["authorization_endpoint"],
        "tokenEndpoint": answer["token_endpoint"],
        "userinfoEndpoint": answer["userinfo_endpoint"],
    }))
}

/// What the settings show: the saved company sign-in, without secrets, and whether a SCIM token exists.
pub fn status(store: &Store) -> Result<Value> {
    let config = load(store)?;
    let configured = config.is_some();
    Ok(json!({
        "configured": configured,
        "issuer": config.as_ref().map(|c| &c.issuer),
        "clientId": config.as_ref().map(|c| &c.client_id),
        "domains": config.as_ref().map(|c| &c.domains),
        "endpoints": config.as_ref().map(|c| &c.endpoints),
        "scimTokenSet": crate::market::vault::read(store, SCIM)?.is_some_and(|t| !t.is_empty()),
    }))
}

pub async fn set(hub: &std::sync::Arc<Hub>, body: &Value) -> Result<Value> {
    crate::market::vault::unlock(hub.clone()).await.ok();
    let issuer = body["issuer"].as_str().unwrap_or_default().trim().to_owned();
    let client_id = body["clientId"].as_str().unwrap_or_default().trim().to_owned();
    let domains: Vec<String> = body["domains"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|d| d.trim().trim_start_matches('@').to_lowercase())
        .filter(|d| !d.is_empty())
        .collect();
    let endpoints = verify(&issuer, &client_id, &domains).await?;
    let config = Config { issuer, client_id, domains, endpoints };
    crate::market::vault::write(&hub.store, CONFIG, &serde_json::to_string(&config)?)?;
    status(&hub.store)
}

pub fn disconnect(store: &Store) -> Result<Value> {
    crate::market::vault::write(store, CONFIG, "")?;
    status(store)
}

/// Makes a new SCIM bearer token (32 bytes, base64url), replacing the old one.
pub fn rotate_scim(store: &Store) -> Result<String> {
    let token = crate::remote::crypto::b64(&crate::remote::crypto::random::<32>());
    crate::market::vault::write(store, SCIM, &token)?;
    Ok(token)
}

/// The SCIM list this computer can answer: the account owner and its paired devices, read-only.
pub fn scim_users(hub: &Hub) -> Result<Value> {
    let mut resources = Vec::new();
    if let Some(owner) = hub.cloud.status().owner {
        resources.push(json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
            "id": owner.user_id,
            "userName": owner.email.unwrap_or_else(|| owner.user_id.clone()),
            "active": true,
            "displayName": "Owner",
        }));
    }
    for device in hub.store.devices()? {
        resources.push(json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
            "id": device.key,
            "userName": device.name,
            "active": true,
            "displayName": device.platform,
        }));
    }
    let total = resources.len();
    Ok(json!({"schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"], "totalResults": total,
        "startIndex": 1, "itemsPerPage": total, "Resources": resources}))
}

/// Whether `authorization: Bearer …` carries the SCIM token.
pub fn scim_allowed(store: &Store, header: Option<&str>) -> bool {
    let Some(given) = header.and_then(|h| h.strip_prefix("Bearer ")) else { return false };
    crate::market::vault::read(store, SCIM).ok().flatten().is_some_and(|token| {
        !token.is_empty()
            && token.len() == given.len()
            && token.bytes().zip(given.bytes()).fold(0_u8, |d, (a, b)| d | (a ^ b)) == 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::Store;

    fn hub() -> std::sync::Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-enterprise-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        Hub::new(store, "x".into(), Identity::load_or_create(&dir).unwrap(), "t".into(), 0)
    }

    #[test]
    fn discovery_lives_under_the_issuer() {
        assert_eq!(discovery_url("https://id.example.com/"), "https://id.example.com/.well-known/openid-configuration");
    }

    #[test]
    fn only_the_current_token_is_accepted() {
        let hub = hub();
        *hub.store.secret_key.lock().unwrap() = Some(zeroize::Zeroizing::new(vec![7; 32]));
        assert!(!scim_allowed(&hub.store, None));
        let token = rotate_scim(&hub.store).unwrap();
        assert!(scim_allowed(&hub.store, Some(&format!("Bearer {token}"))));
        assert!(!scim_allowed(&hub.store, Some("Bearer nope")));
        let newer = rotate_scim(&hub.store).unwrap();
        assert!(!scim_allowed(&hub.store, Some(&format!("Bearer {token}"))));
        assert!(scim_allowed(&hub.store, Some(&format!("Bearer {newer}"))));
    }
}
