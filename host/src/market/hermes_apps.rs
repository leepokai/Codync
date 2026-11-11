//! Source: Hermes optional-mcps/*/manifest.yaml at 10d25f8a (MIT).
//! Adaptation: app cards with duplicate detection against existing catalogs and connections.

use super::Connector;
use super::openbot::{self, App};
use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::Value;
use std::sync::LazyLock;

pub mod registry;

#[derive(Deserialize)]
struct Catalog {
    apps: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(flatten)]
    app: App,
    origin: String,
    auth: String,
}

static CATALOG: LazyLock<Catalog> =
    LazyLock::new(|| serde_json::from_str(include_str!("hermes/apps.json")).expect("valid pinned Hermes MCP catalog"));

/// A temporary discovery failure cannot turn a manifest's required OAuth into an anonymous connection.
pub(super) fn configure_connector(connector: &mut Connector) {
    let Some(entry) = CATALOG.apps.iter().find(|e| connector.catalog.as_deref() == Some(&e.app.id)) else { return };
    if entry.auth == "oauth" {
        connector.oauth.get_or_insert_with(super::oauth::OAuth::default);
    }
}

/// Asana requires the person's own registered MCP app. Never substitute another connection's
/// credentials or silently replace an already registered OAuth client.
pub(super) fn prepare_client(c: &Connector, oauth: &mut super::oauth::OAuth, callbacks: &[String]) -> Result<()> {
    if c.catalog.as_deref() != Some("hermes-asana") {
        return Ok(());
    }
    if !oauth.client_id.is_empty() {
        for callback in callbacks {
            if !oauth.redirect_uris.contains(callback) {
                oauth.redirect_uris.push(callback.clone());
            }
        }
        return Ok(());
    }
    let required = |key: &str| {
        c.env
            .get(key)
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .ok_or_else(|| anyhow!("{key} is required for your Asana MCP app"))
    };
    let client = required("ASANA_CLIENT_ID")?;
    let secret = required("ASANA_CLIENT_SECRET")?;
    oauth.client_id = client;
    oauth.client_secret = Some(secret);
    oauth.redirect_uris = callbacks.to_vec();
    Ok(())
}

/// Keep the OpenBot card when it already names this service or endpoint. User connections
/// are marked installed by endpoint/name; their configuration and credentials are never changed.
pub fn browse(store: &crate::store::Store, connectors: &[Connector], search: &str, port: u16) -> Vec<Value> {
    let query = search.trim().to_lowercase();
    let callbacks = super::oauth::callback_urls(store, port).join(" and ");
    CATALOG
        .apps
        .iter()
        .filter(|e| {
            !openbot::apps().iter().any(|a| same(a, &e.app))
                && (query.is_empty()
                    || e.app.name.to_lowercase().contains(&query)
                    || e.app.tagline.to_lowercase().contains(&query))
        })
        .map(|e| {
            let a = &e.app;
            let installed = connectors.iter().any(|c| {
                c.catalog.as_deref() == Some(&a.id)
                    || c.name.eq_ignore_ascii_case(&a.server.name)
                    || same_url(c.url.as_deref(), a.server.url.as_deref())
            });
            let mut card = openbot::app_public(a, installed);
            card["origin"] = e.origin.clone().into();
            card["license"] = "MIT".into();
            if a.server.name == "asana" {
                for field in card["fields"].as_array_mut().into_iter().flatten() {
                    field["hint"] = format!("Register {callbacks} as redirect URLs in your Asana MCP app.").into();
                }
            }
            card
        })
        .collect()
}

fn same(a: &App, b: &App) -> bool {
    a.server.name.eq_ignore_ascii_case(&b.server.name)
        || a.id == b.server.name
        || same_url(a.server.url.as_deref(), b.server.url.as_deref())
}

fn same_url(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a.trim_end_matches('/') == b.trim_end_matches('/'),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_upstream_manifests_are_present_without_duplicate_openbot_cards() {
        assert_eq!(CATALOG.apps.len(), 66);
        let mut ids = std::collections::HashSet::new();
        for entry in &CATALOG.apps {
            assert!(ids.insert(&entry.app.id));
            assert!(entry.origin.contains("10d25f8ad65fdf71304e9272959421a0c22e5d23"));
            assert!(entry.app.server.url.is_some() || entry.app.fields.iter().any(|f| f.url && f.required));
            assert!(matches!(
                entry.app.category.as_str(),
                "coding"
                    | "design"
                    | "documents"
                    | "research"
                    | "data-analytics"
                    | "productivity"
                    | "automation"
                    | "other"
            ));
        }
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
        for card in browse(&store, &[], "", 19223) {
            let entry = CATALOG.apps.iter().find(|e| e.app.id == card["id"]).unwrap();
            assert!(!openbot::apps().iter().any(|a| same(a, &entry.app)));
        }
    }

    #[test]
    fn endpoint_matching_does_not_collapse_unconfigured_servers() {
        assert!(!same_url(None, None));
        assert!(!same_url(Some(""), Some("")));
        assert!(same_url(Some("https://example.com/mcp/"), Some("https://example.com/mcp")));
        assert!(!same_url(Some("https://example.com/a"), Some("https://example.com/b")));
    }

    #[test]
    fn hermes_asana_uses_its_own_credentials_without_mutating_an_existing_client() {
        let mut c: Connector = serde_json::from_value(serde_json::json!({
            "id":"asana", "name":"asana", "catalog":"hermes-asana",
            "env":{"ASANA_CLIENT_ID":"own-client","ASANA_CLIENT_SECRET":"own-secret"}
        }))
        .unwrap();
        let mut oauth = super::super::oauth::OAuth::default();
        let callbacks = vec![
            "http://127.0.0.1:19223/oauth/callback".to_owned(),
            "https://cloud.example/v1/oauth/callback/dev".to_owned(),
        ];
        prepare_client(&c, &mut oauth, &callbacks).unwrap();
        assert_eq!(oauth.client_id, "own-client");
        assert_eq!(oauth.client_secret.as_deref(), Some("own-secret"));
        assert_eq!(oauth.redirect_uris, callbacks);
        c.env.clear();
        prepare_client(&c, &mut oauth, &callbacks).unwrap();
        assert_eq!(oauth.client_id, "own-client");
        let mut missing = super::super::oauth::OAuth::default();
        assert!(prepare_client(&c, &mut missing, &callbacks).is_err());
        assert!(missing.client_id.is_empty());
    }

    #[test]
    fn hermes_manifest_auth_survives_failed_discovery_without_touching_user_servers() {
        let mut connector: Connector =
            serde_json::from_value(serde_json::json!({"id":"figma","name":"figma","catalog":"hermes-figma"})).unwrap();
        assert!(connector.oauth.is_none());
        configure_connector(&mut connector);
        assert!(connector.oauth.is_some());
        connector.catalog = None;
        connector.oauth = None;
        configure_connector(&mut connector);
        assert!(connector.oauth.is_none());
    }
}
