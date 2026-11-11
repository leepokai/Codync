//! Scoped local API keys: generated once, with their hashes and grants stored in the vault.

use crate::LockExt;
use crate::hub::Hub;
use crate::market::vault;
use crate::remote::crypto;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const SLOT: &str = "openai.api-keys";
static EDIT: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Grant {
    pub id: String,
    label: String,
    hash: String,
    pub bots: Vec<String>,
    created_at: i64,
}

fn read(hub: &Hub) -> Result<Vec<Grant>> {
    Ok(vault::read(&hub.store, SLOT)?.as_deref().map(serde_json::from_str).transpose()?.unwrap_or_default())
}

fn public(grant: &Grant) -> Value {
    json!({"id":grant.id,"label":grant.label,"bots":grant.bots,"createdAt":grant.created_at})
}

fn hash(secret: &str) -> String {
    crate::chat::files::digest_hex(&crypto::sha256(&[secret.as_bytes()]))
}

pub(super) async fn call(hub: &Arc<Hub>, method: &str, body: &Value) -> Result<Value> {
    vault::unlock(hub.clone()).await?;
    let _guard = EDIT.locked();
    let mut grants = read(hub)?;
    match method {
        "openaiKeys" => Ok(
            json!({"keys":grants.iter().map(public).collect::<Vec<_>>(),"baseUrl":format!("http://127.0.0.1:{}/v1",hub.port)}),
        ),
        "openaiRevokeKey" => {
            let id = body["id"].as_str().context("id is required")?;
            grants.retain(|g| g.id != id);
            vault::write(&hub.store, SLOT, &serde_json::to_string(&grants)?)?;
            Ok(json!({"revoked":true}))
        }
        "openaiCreateKey" => {
            ensure!(grants.len() < 100, "At most 100 local API keys");
            let label = body["label"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .context("label is required (at most 128 bytes)")?;
            let mut bots: Vec<String> =
                serde_json::from_value(body["bots"].clone()).context("bots must be a list of bot ids")?;
            bots.sort();
            bots.dedup();
            ensure!(!bots.is_empty() && bots.len() <= 100, "Choose 1 to 100 bots");
            for bot in &bots {
                ensure!(
                    hub.store.bot(bot)?.is_some_and(|r| !r.deleted && !r.config.hidden && !r.config.is_group()),
                    "Unknown or unavailable bot"
                );
            }
            let secret = format!("cdx_{}", crypto::b64(&crypto::random::<32>()));
            let grant = Grant {
                id: uuid::Uuid::new_v4().to_string(),
                label: label.to_owned(),
                hash: hash(&secret),
                bots,
                created_at: crate::store::now_ms(),
            };
            let item = public(&grant);
            grants.push(grant);
            vault::write(&hub.store, SLOT, &serde_json::to_string(&grants)?)?;
            Ok(json!({"key":secret,"grant":item}))
        }
        _ => anyhow::bail!("Unknown local API key method"),
    }
}

pub(super) async fn authenticate(hub: &Arc<Hub>, bearer: Option<&str>) -> Result<Option<Grant>> {
    let Some(secret) = bearer.and_then(|s| s.strip_prefix("Bearer ")).filter(|s| s.len() <= 256) else {
        return Ok(None);
    };
    if !secret.starts_with("cdx_") {
        return Ok(None);
    }
    vault::unlock(hub.clone()).await?;
    let digest = hash(secret);
    Ok(read(hub)?.into_iter().find(|g| crypto::ct_eq(g.hash.as_bytes(), digest.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Store};

    #[tokio::test]
    async fn hermes_openai_keys_are_scoped_revocable_and_never_list_the_secret() {
        let dir = std::env::temp_dir().join(format!("codync-openai-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let bot: BotConfig = serde_json::from_value(json!({"id":"a","name":"A","backend":"hermes","cwd":dir})).unwrap();
        store.save_bot(&bot).unwrap();
        let hub = Hub::new(store, "openai-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let created = call(&hub, "openaiCreateKey", &json!({"label":"Test","bots":["a"]})).await.unwrap();
        let secret = created["key"].as_str().unwrap();
        let header = format!("Bearer {secret}");
        let grant = authenticate(&hub, Some(&header)).await.unwrap().unwrap();
        assert_eq!(grant.bots, ["a"]);
        assert!(!call(&hub, "openaiKeys", &Value::Null).await.unwrap().to_string().contains(secret));
        assert!(!hub.store.kv_read(SLOT).unwrap().unwrap().contains(secret));
        call(&hub, "openaiRevokeKey", &json!({"id":grant.id})).await.unwrap();
        assert!(authenticate(&hub, Some(&header)).await.unwrap().is_none());
        assert!(call(&hub, "openaiCreateKey", &json!({"label":"Test","bots":["missing"]})).await.is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
