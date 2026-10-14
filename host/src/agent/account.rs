//! Multi-account management for agent backends (e.g. isolated Antigravity profiles).

use crate::agent::auth;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub created_at: i64,
}

pub fn accounts_dir() -> PathBuf {
    crate::service::data_dir().join("accounts")
}

pub fn account_dir(id: &str) -> PathBuf {
    accounts_dir().join(id)
}

/// Lists all configured accounts, optionally filtered by backend.
pub fn list(backend: Option<&str>) -> Result<Vec<AgentAccount>> {
    let dir = accounts_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut list = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let meta_path = entry.path().join("account.json");
            if let Ok(content) = fs::read_to_string(&meta_path)
                && let Ok(acc) = serde_json::from_str::<AgentAccount>(&content)
                && (backend.is_none() || backend == Some(&acc.backend))
            {
                list.push(acc);
            }
        }
    }
    list.sort_by_key(|a| a.created_at);
    Ok(list)
}

/// Returns environment variables needed to run an agent with `account_id`.
pub fn env(account_id: &str, backend: &str) -> Vec<(String, String)> {
    if backend == "antigravity" {
        vec![("GEMINI_HOME".to_string(), account_dir(account_id).to_string_lossy().to_string())]
    } else {
        Vec::new()
    }
}

/// Try to fetch the user's Google email using the OAuth tokens in `account_dir`.
async fn resolve_antigravity_email(account_id: &str) -> Option<String> {
    let token_path = account_dir(account_id).join("antigravity-acp").join("acp_token.json");
    let content = fs::read_to_string(&token_path).ok()?;
    let t: Value = serde_json::from_str(&content).ok()?;
    let refresh_token = t["refresh_token"].as_str()?;
    let client_id = t["client_id"].as_str()?;
    let client_secret = t["client_secret"].as_str()?;
    let token_uri = t["token_uri"].as_str().unwrap_or("https://oauth2.googleapis.com/token");

    let client = crate::http();
    let refresh_resp = client
        .post(token_uri)
        .json(&json!({
            "client_id": client_id,
            "client_secret": client_secret,
            "refresh_token": refresh_token,
            "grant_type": "refresh_token",
        }))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    let token_data: Value = refresh_resp.json().await.ok()?;
    let access_token = token_data["access_token"].as_str()?;

    let user_resp = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    let user_data: Value = user_resp.json().await.ok()?;
    user_data["email"].as_str().map(str::to_owned)
}

/// Triggers ACP interactive sign-in in an isolated profile directory, saving the resulting account.
pub async fn authenticate(store: &Store, backend: &str, name: Option<String>) -> Result<AgentAccount> {
    if backend != "antigravity" {
        bail!("Account login is currently supported for Google Antigravity");
    }

    let id = format!("acc_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let dir = account_dir(&id);
    fs::create_dir_all(&dir)?;

    let extra_env = env(&id, backend);
    let (acp, _inbox, _, _) = auth::start_with_env(store, backend, &extra_env).await?;

    let res = tokio::time::timeout(
        Duration::from_secs(10 * 60),
        acp.request("authenticate", json!({"methodId": "oauth-personal"})),
    )
    .await;
    acp.kill().await;

    if let Ok(r) = res {
        r.context("signing in to account")?;
    } else {
        let _ = fs::remove_dir_all(&dir);
        bail!("Sign-in didn't finish within 10 minutes.");
    }

    let email = resolve_antigravity_email(&id).await;
    let account_name = name.filter(|n| !n.trim().is_empty()).or(email).unwrap_or_else(|| format!("Google ({id})"));

    let account = AgentAccount {
        id: id.clone(),
        name: account_name,
        backend: backend.to_owned(),
        created_at: crate::store::now_ms(),
    };

    let meta_path = dir.join("account.json");
    fs::write(&meta_path, serde_json::to_string_pretty(&account)?)?;

    Ok(account)
}

/// Deletes an account profile directory.
pub fn delete(id: &str) -> Result<()> {
    let dir = account_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_env_sets_gemini_home() {
        let vars = env("acc_test123", "antigravity");
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].0, "GEMINI_HOME");
        assert!(vars[0].1.ends_with("acc_test123"));
    }

    #[test]
    fn other_backend_env_is_empty() {
        let vars = env("acc_test123", "claude");
        assert_eq!(vars, Vec::<(String, String)>::new());
    }

    #[test]
    fn accounts_serialization_round_trip() {
        let acc = AgentAccount {
            id: "acc_1".into(),
            name: "test@example.com".into(),
            backend: "antigravity".into(),
            created_at: 1000,
        };
        let s = serde_json::to_string(&acc).unwrap();
        let loaded: AgentAccount = serde_json::from_str(&s).unwrap();
        assert_eq!(loaded.id, "acc_1");
        assert_eq!(loaded.name, "test@example.com");
    }
}
