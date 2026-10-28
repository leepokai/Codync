//! Optional 1Password references. The service token is encrypted in the same vault;
//! references resolve only at the execution boundary and never enter transcripts.
use crate::store::Store;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::{process::Stdio, time::Duration};

pub fn status(store: &Store) -> Result<Value> {
    Ok(json!({"provider":if cfg!(target_os="macos") {"Keychain"} else {"Secret Service"},
        "onePasswordConnected":super::vault::read(store,"credential.1password")?.is_some_and(|s| !s.is_empty())}))
}

pub async fn set_token(store: &Store, token: &str) -> Result<Value> {
    if !token.is_empty() {
        let result = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new("op")
                .args(["vault", "list", "--format", "json"])
                .env("OP_SERVICE_ACCOUNT_TOKEN", token)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status(),
        )
        .await;
        if !matches!(result, Ok(Ok(s)) if s.success()) {
            bail!(
                "Install the 1Password CLI on this computer and provide a service account token with access to your shared vault."
            );
        }
    }
    super::vault::write(store, "credential.1password", token)?;
    status(store)
}

pub async fn resolve(store: &Store, value: &str) -> Result<String> {
    if !value.starts_with("op://") {
        return Ok(value.to_owned());
    }
    if value.len() > 2048 || value.contains(['\n', '\r']) {
        bail!("Invalid 1Password reference");
    }
    let token = super::vault::read(store, "credential.1password")?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Connect 1Password in Credentials first"))?;
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new("op")
            .args(["read", "--no-newline", value])
            .env("OP_SERVICE_ACCOUNT_TOKEN", token)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| anyhow!("1Password timed out"))?
    .map_err(|_| anyhow!("Install the 1Password CLI on this computer"))?;
    if !result.status.success() {
        bail!("Could not read the 1Password reference. Check shared vault access.");
    }
    String::from_utf8(result.stdout).map_err(|_| anyhow!("Invalid credential encoding"))
}

/// Resolves every stored value of a connector's credentials at once, for the process it
/// launches and the arguments whose templates name them.
pub async fn resolve_all(store: &Store, values: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    let mut resolved = BTreeMap::new();
    for (name, value) in values {
        resolved.insert(name.clone(), resolve(store, value).await?);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn ordinary_values_are_unchanged_and_references_require_a_connected_vault() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(resolve(&store, " a secret with spaces ").await.unwrap(), " a secret with spaces ");
        assert!(resolve(&store, "op://shared/service/key").await.is_err());
        assert!(!status(&store).unwrap()["onePasswordConnected"].as_bool().unwrap());
        super::super::vault::write(&store, "credential.1password", "test-token").unwrap();
        assert!(status(&store).unwrap()["onePasswordConnected"].as_bool().unwrap());
        assert!(!status(&store).unwrap().to_string().contains("test-token"));
    }
}
