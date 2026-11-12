//! The bridge: an entry of the CopilotKit catalogue, installed as a Codync connector.
//!
//! Upstream's auth kinds decide the connector, and nothing else does:
//!
//! - `none` is a plain remote server;
//! - `deployment-bearer` is the same server with the entry's header carrying the typed key, wrapped
//!   in the entry's own template (`Bearer {apiKey}`, or the key alone for Exa's `x-api-key`);
//! - `user-oauth` is a remote server with the vendor's endpoints, scopes and consent parameters
//!   pinned on the connector instead of discovered at sign-in. A `pasted` client comes from the
//!   sheet (Google issues no pasted token); a `dynamic` one registers itself at the entry's endpoint;
//! - `builtin` runs in this process already, so it installs nothing and answers with its public
//!   shape.
//!
//! Installing an app again replaces the connector under the id its bots know, keeping the new key or
//! the new sign-in, so Add after a failure is the way to repair one.

use super::{App, app, builtin_public};
use crate::market::install::fill;
use crate::market::{Connector, add_connector, connectors, oauth, unique_id};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Installs catalogue app `id` with the values the user typed (`{fieldId: value}`).
pub async fn install_app(store: &crate::store::Store, id: &str, values: &Value) -> Result<Value> {
    let a = app(id)?;
    if a.builtin {
        return Ok(builtin_public(a));
    }
    let url = a.server.url.clone().ok_or_else(|| anyhow::anyhow!("{} has no server", a.name))?;
    let all = connectors(store)?;
    let mut c = Connector {
        id: unique_id(&a.name, &all.iter().map(|c| c.id.clone()).collect::<Vec<_>>()),
        name: a.name.clone(),
        description: a.description.clone(),
        registry_name: None,
        catalog: Some(a.id.clone()),
        command: None,
        args: vec![],
        env: BTreeMap::new(),
        url: Some(url.clone()),
        headers: BTreeMap::new(),
        oauth: None,
    };
    let input = |key: &str| values[key].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    match a.auth.kind.as_str() {
        "none" => {}
        "deployment-bearer" => fill_headers(a, &input, &mut c)?,
        "user-oauth" => c.oauth = Some(oauth::OAuth::from_pins(a, &input)?),
        other => bail!("{} has an unknown auth kind {other}", a.id),
    }
    crate::egress::check(c.egress(), &url).await?;
    add_connector(store, c)
}

/// The entry's header fields, as the headers its server reads. A required one with no value refuses.
fn fill_headers(a: &App, input: &dyn Fn(&str) -> Option<String>, c: &mut Connector) -> Result<()> {
    for field in &a.fields {
        let Some(header) = &field.header else { continue };
        match input(&field.id) {
            Some(value) => {
                c.headers.insert(header.clone(), fill(field.template.as_deref(), &value));
            }
            None if field.required => bail!("{} is required", field.label),
            None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> crate::store::Store {
        crate::store::Store::open(std::path::Path::new(":memory:")).expect("store")
    }

    #[tokio::test]
    async fn a_key_entry_sends_its_key_in_the_entrys_header() {
        let store = store();
        install_app(&store, "exa-authenticated", &json!({"apiKey": " e-1 "})).await.expect("install");
        let c = connectors(&store).expect("read").pop().expect("connector");
        assert_eq!(c.headers["x-api-key"], "e-1");
        assert_eq!(c.catalog.as_deref(), Some("exa-authenticated"));
        assert!(c.oauth.is_none());
        assert!(install_app(&store, "exa-authenticated", &json!({})).await.is_err(), "the key is required");
    }

    #[tokio::test]
    async fn a_bearer_entry_wraps_the_key_and_installing_again_replaces_it() {
        let store = store();
        let first = install_app(&store, "firecrawl-authenticated", &json!({"apiKey": "fc-1"})).await.expect("install");
        assert_eq!(
            connectors(&store).expect("read").pop().expect("connector").headers["Authorization"],
            "Bearer fc-1"
        );
        let again = install_app(&store, "firecrawl-authenticated", &json!({"apiKey": "fc-2"})).await.expect("install");
        assert_eq!(again["id"], first["id"], "the same connector under the same id");
        let all = connectors(&store).expect("read");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].headers["Authorization"], "Bearer fc-2");
    }

    #[tokio::test]
    async fn a_pinned_oauth_entry_keeps_the_vendors_terms() {
        let store = store();
        install_app(&store, "google-drive", &json!({"clientId": "id", "clientSecret": "secret"}))
            .await
            .expect("install");
        let c = connectors(&store).expect("read").pop().expect("connector");
        let pin = c.oauth.expect("sign-in");
        assert_eq!((pin.client_id.as_str(), pin.client_secret.as_deref()), ("id", Some("secret")));
        assert_eq!(pin.scopes, ["https://www.googleapis.com/auth/drive.readonly"]);
        assert_eq!(pin.authorization_params["access_type"], "offline");
        assert_eq!(pin.issuer.as_deref(), Some("https://accounts.google.com"));
        assert!(!pin.signed_in(), "nobody signed in yet");
        assert!(install_app(&store, "google-drive", &json!({"clientId": "id"})).await.is_err(), "the secret is required");
    }

    #[tokio::test]
    async fn a_dynamic_entry_needs_no_client() {
        let store = store();
        install_app(&store, "notion", &json!({})).await.expect("install");
        let pin = connectors(&store).expect("read").pop().expect("connector").oauth.expect("sign-in");
        assert!(pin.client_id.is_empty());
        assert_eq!(pin.registration_url.as_deref(), Some("https://mcp.notion.com/register"));
    }

    #[tokio::test]
    async fn a_builtin_entry_installs_nothing() {
        let store = store();
        let app = install_app(&store, "routines", &json!({})).await.expect("install");
        assert_eq!(app["kind"], "builtin");
        assert!(connectors(&store).expect("read").is_empty());
    }
}
