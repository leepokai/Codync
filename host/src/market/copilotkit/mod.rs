//! The connector catalogue Codync ships: every app the `CopilotKit/OpenBot` connector catalogue
//! offers, ported so it installs here the way it installs there.
//!
//! The entries keep upstream's keys, titles, vendors, hosts, auth kinds, headers, OAuth endpoints,
//! scopes, authorization parameters, registration endpoint, reviewed write/read tool lists and docs
//! links (see `NOTICE.md`). Codync only writes the sheet: category, tagline, description, prompts.
//! `install.rs` is the bridge that turns an entry into a connector with the same auth semantics.
//!
//! Browsing is offline; the file is embedded. `tools/import-copilotkit-connectors.mjs` regenerates
//! it from a CopilotKit checkout.
//!
//! This is a different catalogue from `market::openbot` beside it, which holds the agents and skills
//! of `nightly-labs/openbot`. Do not mix the two.

mod install;

pub use install::install_app;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// What an entry's server wants by way of a credential, in upstream's own terms.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    /// `none`, `deployment-bearer`, `user-oauth` or `builtin`.
    #[serde(default)]
    pub kind: String,
    /// The header a deployment key goes in, when it is not `Authorization`.
    #[serde(default)]
    pub header: Option<String>,
    /// `pasted` (the sheet asks for a client id and secret) or `dynamic` (the server registers us).
    #[serde(default)]
    pub client: Option<String>,
    /// The issuer a consent must come back from (RFC 9207), when the vendor states one.
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub authorization_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub revoke_url: Option<String>,
    #[serde(default)]
    pub registration_url: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    /// The vendor's own consent parameters, never the flow's security ones.
    #[serde(default)]
    pub authorization_params: BTreeMap<String, String>,
}

impl Auth {
    /// Whether the OAuth endpoints are the vendor's reviewed constants rather than discovered.
    pub fn pinned(&self) -> bool {
        self.authorization_url.is_some() && self.token_url.is_some()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub description: String,
    pub category: String,
    pub creator: String,
    pub version: String,
    /// The date the catalogue last changed it, YYYY-MM-DD.
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    pub featured: bool,
    /// `open` (nothing), `key` (fields), `link` (browser sign-in) or `local` (steps).
    pub setup: String,
    /// First-party and in-process: nothing to install, always offered.
    #[serde(default)]
    pub builtin: bool,
    pub server: Server,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub write_tools: Vec<String>,
    #[serde(default)]
    pub read_tools: Vec<String>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub docs_label: Option<String>,
    #[serde(default)]
    pub prompts: Vec<Prompt>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Server {
    pub name: String,
    /// `http` or `builtin`.
    pub transport: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub env: Option<String>,
    #[serde(default)]
    pub header: Option<String>,
    /// What a typed value is wrapped in before it becomes the header, such as `Bearer {apiKey}`.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub url: bool,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    pub required: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Prompt {
    pub id: String,
    pub text: String,
}

#[derive(Deserialize)]
struct Apps {
    apps: Vec<App>,
}

static APPS: LazyLock<Apps> =
    LazyLock::new(|| serde_json::from_str(include_str!("apps.json")).expect("valid app catalog"));

pub fn apps() -> &'static [App] {
    &APPS.apps
}

pub fn app(id: &str) -> Result<&'static App> {
    apps().iter().find(|a| a.id == id).ok_or_else(|| anyhow!("Unknown app"))
}

/// An app the computer already holds: a connector installed from it, plus the builtin ones.
pub fn installed(catalogs: &[String]) -> Vec<String> {
    let mut ids = catalogs.to_vec();
    ids.extend(apps().iter().filter(|a| a.builtin).map(|a| a.id.clone()));
    ids
}

/// An app's public fields, `installed` when a connector holds it.
pub fn app_public(a: &App, installed: bool) -> Value {
    json!({
        "id": a.id, "name": a.name, "tagline": a.tagline, "description": a.description,
        "category": a.category, "creator": a.creator, "version": a.version, "updated": a.updated, "icon": a.icon,
        "website": a.website, "featured": a.featured, "setup": a.setup, "builtin": a.builtin,
        "server": a.server, "fields": a.fields, "steps": a.steps, "docsUrl": a.docs_url,
        "docsLabel": a.docs_label, "prompts": a.prompts, "writeTools": a.write_tools,
        "readTools": a.read_tools, "installed": installed,
    })
}

/// The catalogue, featured first, `search` over the name and tagline, marked installed.
pub fn browse_apps(installed: &[String], search: &str) -> Vec<Value> {
    let query = search.trim().to_lowercase();
    let mut list: Vec<&App> = apps()
        .iter()
        .filter(|a| {
            query.is_empty() || a.name.to_lowercase().contains(&query) || a.tagline.to_lowercase().contains(&query)
        })
        .collect();
    list.sort_by_key(|a| !a.featured);
    list.into_iter().map(|a| app_public(a, installed.contains(&a.id))).collect()
}

/// The connector `id` a builtin app answers with: it is not stored, so its public shape is made here.
pub fn builtin_public(a: &App) -> Value {
    json!({
        "id": a.id, "name": a.name, "description": a.description, "registryName": null,
        "catalog": a.id, "kind": "builtin", "command": null, "url": null, "keys": [],
        "auth": "none", "canSignIn": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_catalogue_is_complete_and_unique() {
        assert_eq!(apps().len(), 9, "every CopilotKit connector is here");
        assert_eq!(apps().iter().map(|a| &a.id).collect::<HashSet<_>>().len(), apps().len());
        assert!(apps().iter().any(|a| a.featured));
        for a in apps() {
            assert!(!a.name.trim().is_empty() && !a.tagline.trim().is_empty(), "{}", a.id);
            assert!(a.docs_url.as_deref().is_some_and(|d| d.starts_with("https://")), "{}", a.id);
        }
    }

    #[test]
    fn every_entry_describes_how_it_is_reached() {
        for a in apps() {
            match a.auth.kind.as_str() {
                "none" => {
                    assert!(!a.builtin, "{} needs no auth and runs nowhere", a.id);
                    assert!(a.server.url.is_some(), "{}", a.id);
                }
                "deployment-bearer" => {
                    assert!(a.server.url.is_some() && !a.fields.is_empty(), "{}", a.id);
                    let header = a.auth.header.as_deref().unwrap_or("Authorization");
                    assert!(a.fields.iter().any(|f| f.header.as_deref() == Some(header)), "{}", a.id);
                }
                "user-oauth" => {
                    assert!(a.server.url.is_some() && a.auth.pinned(), "{}", a.id);
                    assert!(a.auth.issuer.is_some(), "{}", a.id);
                    match a.auth.client.as_deref() {
                        Some("dynamic") => assert!(a.auth.registration_url.is_some(), "{}", a.id),
                        Some("pasted") => assert!(
                            a.fields.iter().any(|f| f.id == "clientId" && f.required)
                                && a.fields.iter().any(|f| f.id == "clientSecret" && f.required),
                            "{}",
                            a.id
                        ),
                        other => panic!("{} has an unknown client kind {other:?}", a.id),
                    }
                    assert!(!a.auth.scopes.is_empty() || a.auth.client.as_deref() == Some("dynamic"), "{}", a.id);
                }
                "builtin" => {
                    assert!(a.builtin && a.server.url.is_none(), "{}", a.id);
                    assert!(!a.write_tools.is_empty(), "{}", a.id);
                }
                other => panic!("{} has an unknown auth kind {other}", a.id),
            }
            for key in a.auth.authorization_params.keys() {
                assert!(
                    !matches!(
                        key.as_str(),
                        "client_id" | "redirect_uri" | "response_type" | "state" | "code_challenge"
                            | "code_challenge_method"
                    ),
                    "{} rewrites {key}",
                    a.id
                );
            }
        }
    }

    #[test]
    fn a_builtin_app_is_always_installed() {
        let ids = installed(&["notion".to_owned()]);
        assert!(ids.contains(&"notion".to_owned()) && ids.contains(&"routines".to_owned()));
        assert_eq!(builtin_public(app("routines").expect("routines"))["kind"], "builtin");
    }

    #[test]
    fn searching_finds_a_name_and_keeps_featured_first() {
        assert_eq!(browse_apps(&[], "notion").len(), 1);
        assert_eq!(browse_apps(&[], "zzz").len(), 0);
        let all = browse_apps(&[], "");
        assert!(all[0]["featured"] == true, "featured picks come first");
        assert_eq!(all.len(), apps().len());
    }
}
