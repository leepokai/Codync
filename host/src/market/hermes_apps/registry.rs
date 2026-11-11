//! The terminal reuses the connector setup flow for bundled Hermes app manifests.

use super::{CATALOG, Entry, same, same_url};
use crate::market::{self, Connector, openbot};
use crate::store::Store;
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};

const PREFIX: &str = "hermes/";

fn entry(name: &str) -> Option<&'static Entry> {
    let id = name.strip_prefix(PREFIX)?;
    CATALOG.apps.iter().find(|e| e.app.server.name == id)
}

/// Present a pinned app as the remote connector shape the existing terminal understands.
pub fn server(name: &str) -> Option<Value> {
    let entry = entry(name)?;
    let app = &entry.app;
    let mut variables = Map::new();
    let mut url = app.server.url.clone().unwrap_or_default();
    for field in &app.fields {
        variables.insert(
            field.id.clone(),
            json!({"description":field.label,"isSecret":!field.url,"isRequired":field.required || field.url}),
        );
        if field.url {
            url = format!("{{{}}}", field.id);
        }
    }
    Some(json!({"name":name,"title":format!("{} · Hermes",app.name),"description":app.description,
        "version":app.version,"websiteUrl":entry.origin,"codyncCatalog":app.id,
        "remotes":[{"type":"streamable-http","url":url,"variables":variables}]}))
}

pub fn listings(store: &Store, search: &str) -> Result<Vec<Value>> {
    let connectors = market::connectors(store)?;
    let query = search.trim().to_lowercase();
    let mut result = Vec::new();
    for entry in &CATALOG.apps {
        let app = &entry.app;
        if openbot::apps().iter().any(|a| same(a, app)) {
            continue;
        }
        if !query.is_empty()
            && !app.name.to_lowercase().contains(&query)
            && !app.description.to_lowercase().contains(&query)
        {
            continue;
        }
        let alias = format!("{PREFIX}{}", app.server.name);
        if let Some(server) = server(&alias)
            && let Some(mut card) = market::registry::listing(&server, &connectors)
        {
            card["installed"] = installed(&connectors, entry).into();
            result.push(card);
        }
    }
    Ok(result)
}

fn installed(connectors: &[Connector], entry: &Entry) -> bool {
    connectors.iter().any(|c| {
        c.catalog.as_deref() == Some(&entry.app.id) || same_url(c.url.as_deref(), entry.app.server.url.as_deref())
    })
}

/// Use the same installation and vault path as the graphical clients. Inputs never become code.
pub async fn install(store: &Store, name: &str, option: &str, inputs: &Value) -> Result<Value> {
    ensure!(option == "remote:0", "Unknown Hermes connector option");
    let entry = entry(name).context("Unknown Hermes connector")?;
    let app = &entry.app;
    let mut url = app.server.url.clone();
    let mut env = Map::new();
    let mut headers = Map::new();
    for field in &app.fields {
        let value = inputs[&field.id].as_str().map(str::trim).filter(|s| !s.is_empty());
        ensure!(value.is_some() || (!field.required && !field.url), "{} is required", field.label);
        if let Some(value) = value {
            ensure!(value.len() <= 64 * 1024, "{} is too long", field.label);
            if field.url {
                url = Some(value.to_owned());
            } else if let Some(key) = &field.env {
                env.insert(key.clone(), value.into());
            } else if let Some(key) = &field.header {
                headers.insert(key.clone(), value.into());
            }
        }
    }
    let existing = market::connectors(store)?;
    if app.fields.is_empty()
        && let Some(connector) = existing
            .iter()
            .find(|c| c.catalog.as_deref() == Some(&app.id) || same_url(c.url.as_deref(), url.as_deref()))
    {
        return Ok(connector.public());
    }
    market::add_custom_connector(
        store,
        &json!({"name":app.server.name,"description":app.description,
        "url":url,"env":env,"headers":headers,"catalog":app.id}),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_terminal_entries_expose_auth_and_url_fields_with_exact_names() {
        let asana = server("hermes/asana").unwrap();
        let options = market::install::install_options(&asana);
        let fields = options[0]["inputs"].as_array().unwrap();
        assert!(fields.iter().any(|f| f["name"] == "ASANA_CLIENT_SECRET" && f["secret"] == true));
        let n8n = server("hermes/n8n-official").unwrap();
        assert_eq!(n8n["remotes"][0]["url"], "{N8N_MCP_SERVER_URL}");
        assert!(server("hermes/missing").is_none());
    }
}
