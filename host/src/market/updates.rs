//! Track Registry package releases without interrupting running connectors.

use super::{Connector, connectors, install::package_reference, registry::registry_server, update_connectors};
use crate::{LockExt, hub::Hub, store::Store};
use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::Instant;

const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY_INTERVAL: Duration = Duration::from_secs(15 * 60);
const POLL_INTERVAL: Duration = Duration::from_secs(60);

pub async fn automatic_loop(hub: Arc<Hub>) {
    let mut tracker = Tracker::default();
    loop {
        // Clients and bot sessions unlock the vault when needed. Background
        // updates must not cause an unsolicited keychain prompt at login.
        if hub.store.secret_key.locked().is_some()
            && let Err(error) = tracker.refresh_due(&hub.store, Instant::now(), registry_server).await
        {
            tracing::warn!(error = format!("{error:#}"), "connector update check failed");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Refresh one connector immediately before its process is launched. A failed
/// lookup is deliberately ignored by callers so an installed version remains
/// usable while the Registry or network is unavailable.
pub async fn refresh_connector(store: &Store, id: &str) {
    let Some(before) = connectors(store).ok().and_then(|all| all.into_iter().find(|c| c.id == id)) else {
        return;
    };
    let Some(name) = before.registry_name.as_deref().filter(|_| PackageKind::for_connector(&before).is_some()) else {
        return;
    };
    match registry_server(name).await.and_then(|server| apply_latest(store, &before, &server)) {
        Ok(true) => tracing::info!(connector = id, "connector updated for its next launch"),
        Ok(false) => {}
        Err(error) => tracing::debug!(connector = id, error = format!("{error:#}"), "connector update unavailable"),
    }
}

#[derive(Default)]
struct Tracker {
    next_check: HashMap<String, Instant>,
}

impl Tracker {
    async fn refresh_due(
        &mut self,
        store: &Store,
        now: Instant,
        lookup: impl AsyncFn(&str) -> Result<Value>,
    ) -> Result<()> {
        let installed = connectors(store)?;
        self.next_check.retain(|id, _| installed.iter().any(|c| c.id == *id));
        for c in installed {
            let Some(name) = c.registry_name.as_deref().filter(|_| PackageKind::for_connector(&c).is_some()) else {
                continue;
            };
            if self.next_check.get(&c.id).is_some_and(|next| *next > now) {
                continue;
            }
            let result = match lookup(name).await {
                Ok(server) => apply_latest(store, &c, &server),
                Err(error) => Err(error),
            };
            let interval = match result {
                Ok(changed) => {
                    if changed {
                        tracing::info!(connector = c.id, "connector updated for its next launch");
                    }
                    CHECK_INTERVAL
                }
                Err(error) => {
                    tracing::warn!(
                        connector = c.id,
                        error = format!("{error:#}"),
                        "keeping installed connector version"
                    );
                    RETRY_INTERVAL
                }
            };
            self.next_check.insert(c.id, now + interval);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PackageKind {
    Npm,
    Pypi,
    Oci,
    Nuget,
}

impl PackageKind {
    fn for_connector(c: &Connector) -> Option<Self> {
        match c.command.as_deref()? {
            "npx" => Some(Self::Npm),
            "uvx" => Some(Self::Pypi),
            "docker" => Some(Self::Oci),
            "dnx" => Some(Self::Nuget),
            _ => None,
        }
    }

    fn registry_type(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pypi => "pypi",
            Self::Oci => "oci",
            Self::Nuget => "nuget",
        }
    }

    fn split_reference(self, reference: &str) -> Option<(&str, &str)> {
        match self {
            Self::Npm | Self::Nuget => reference.rsplit_once('@'),
            Self::Pypi => reference.split_once("=="),
            Self::Oci => {
                reference.split_once('@').or_else(|| reference.rsplit_once(':').filter(|(_, tag)| !tag.contains('/')))
            }
        }
    }
}

struct Replacement {
    index: usize,
    reference: String,
}

/// Match by runtime and package identity, never by the Registry array index.
/// Only the version-bearing argument changes; credentials and user flags stay put.
fn replacement(c: &Connector, server: &Value) -> Result<Option<Replacement>> {
    let Some(kind) = PackageKind::for_connector(c) else { return Ok(None) };
    let mut matched = None;
    for p in server["packages"].as_array().into_iter().flatten() {
        if p["registryType"] != kind.registry_type() || p["transport"]["type"].as_str().is_some_and(|t| t != "stdio") {
            continue;
        }
        let Some(id) = p["identifier"].as_str() else { continue };
        let identity =
            if kind == PackageKind::Oci { kind.split_reference(id).map_or(id, |(name, _)| name) } else { id };
        for (index, arg) in c.args.iter().enumerate() {
            let Some((name, old_version)) = kind.split_reference(arg) else { continue };
            if name != identity {
                continue;
            }
            if matched.is_some() {
                bail!("Registry package selection is ambiguous; keeping installed version");
            }
            let reference = package_reference(p)?;
            let (_, new_version) =
                kind.split_reference(&reference).ok_or_else(|| anyhow!("invalid package reference"))?;
            // Avoid downgrades when a stale Registry replica serves an older release.
            let parse = |v: &str| semver::Version::parse(v.strip_prefix('v').unwrap_or(v));
            if let (Ok(old), Ok(new)) = (parse(old_version), parse(new_version))
                && new < old
            {
                bail!("Registry returned an older package version; keeping installed version");
            }
            matched = Some(Replacement { index, reference });
        }
    }
    matched.map(Some).ok_or_else(|| anyhow!("installed package is no longer offered by the Registry"))
}

fn apply_latest(store: &Store, before: &Connector, server: &Value) -> Result<bool> {
    if before.registry_name.is_none() || before.command.is_none() {
        return Ok(false);
    }
    if server["name"].as_str() != before.registry_name.as_deref() {
        bail!("Registry returned a different server");
    }
    let Some(change) = replacement(before, server)? else { return Ok(false) };
    if before.args[change.index] == change.reference {
        return Ok(false);
    }
    update_connectors(store, |all| {
        let Some(current) = all.iter_mut().find(|c| c.id == before.id) else { return Ok(false) };
        // A concurrent credential edit is preserved. A removal or replacement
        // must never be undone by a lookup that started before it.
        if current.registry_name != before.registry_name
            || current.command != before.command
            || current.args != before.args
        {
            return Ok(false);
        }
        current.args[change.index] = change.reference;
        Ok(true)
    })
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;
