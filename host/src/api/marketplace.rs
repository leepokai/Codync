//! Marketplace methods: connectors, credentials, Composio and skills.

use super::devices::Caller;
use super::str_arg;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::market;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

/// Everything a bot can turn on: installed connectors and connected apps.
pub(super) fn connector_ids(store: &crate::store::Store) -> Result<Vec<String>> {
    Ok(market::list_connectors(store)?["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["id"].as_str().map(str::to_owned))
        .collect())
}

/// A connector installed on the computer is on for every bot; each bot can turn it off.
fn enable_new_connectors(hub: &Hub, before: &[String]) -> Result<()> {
    let new: Vec<String> = connector_ids(&hub.store)?.into_iter().filter(|id| !before.contains(id)).collect();
    if new.is_empty() {
        return Ok(());
    }
    for row in hub.store.bots()? {
        if row.deleted || row.config.is_group() {
            continue;
        }
        let mut on = row.config.connectors;
        for id in &new {
            if !on.contains(id) {
                on.push(id.clone());
            }
        }
        hub.update_bot(&json!({"id": row.config.id, "connectors": on}))?;
    }
    Ok(())
}

pub(super) async fn call(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    Ok(ControlFlow::Break(match method {
        "credentialUpdateConnector" => {
            let id = str_arg(&b, "id")?;
            let fields = b["fields"].as_object().ok_or_else(|| anyhow!("Credential fields are required"))?;
            market::update_connectors(&hub.store, |all| {
                let c = all.iter_mut().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
                let values = if c.command.is_some() { &mut c.env } else { &mut c.headers };
                for (key, value) in fields {
                    if !values.contains_key(key) {
                        bail!("Unknown credential field");
                    }
                    let value = value
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 64 * 1024)
                        .ok_or_else(|| anyhow!("Invalid credential value"))?;
                    values.insert(key.clone(), value.to_owned());
                }
                Ok(())
            })?;
            let ready = market::verify::verify(&hub.store, id, hub.port).await?;
            for row in hub.store.bots()? {
                if !row.deleted && !row.config.is_group() && row.config.connectors.iter().any(|c| c == id) {
                    hub.send_cmd(&row.config.id, Cmd::RefreshTools)?;
                }
            }
            ready
        }
        "credentialStatus" => market::passwords::status(&hub.store)?,
        "credentialLogins" => market::logins::list(&hub.store)?,
        "credentialSaveLogin" => {
            let login = market::logins::save(
                &hub.store,
                str_arg(&b, "site")?,
                b["username"].as_str().unwrap_or_default(),
                str_arg(&b, "password")?,
            )?;
            json!({"login": login.public()})
        }
        "credentialRemoveLogin" => {
            market::logins::remove(&hub.store, str_arg(&b, "id")?)?;
            json!({})
        }
        "credentialSetOnePassword" => {
            market::passwords::set_token(&hub.store, b["token"].as_str().unwrap_or_default()).await?
        }
        "connectorRuntime" => {
            let id = str_arg(&b, "id")?;
            // Registry-backed packages may advance between background checks;
            // refresh just before launch so the new version applies next run.
            market::refresh_connector(&hub.store, id).await;
            let c = market::connectors(&hub.store)?
                .into_iter()
                .find(|c| c.id == id)
                .ok_or_else(|| anyhow!("Unknown connector"))?;
            let mut env = serde_json::Map::new();
            for (k, v) in &c.env {
                env.insert(k.clone(), market::passwords::resolve(&hub.store, v).await?.into());
            }
            json!({"command":c.command,"args":c.args,"env":env})
        }
        "connectorCall" => {
            market::requests::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "connectorInfo" => market::connector_info(&hub.store, str_arg(&b, "registryName")?).await?,
        "connectorRequestFinish" => market::requests::finish(hub, &b).await?,
        "connectorVerify" => market::verify::verify(&hub.store, str_arg(&b, "id")?, hub.port).await?,
        "composioCall" => {
            json!({"result": market::composio::call(&hub.store, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?})
        }
        "composioStatus" => market::composio::status(&hub.store)?,
        "setComposioKey" => market::composio::set_key(&hub.store, b["key"].as_str().unwrap_or_default()).await?,
        "composioToolkits" => {
            market::composio::toolkits(
                &hub.store,
                b["search"].as_str().unwrap_or_default(),
                b["cursor"].as_str().unwrap_or_default(),
            )
            .await?
        }
        "composioConnect" => market::composio::connect(&hub.store, str_arg(&b, "toolkit")?).await?,
        "composioConnectFields" => {
            let fields = b["fields"].as_object().ok_or_else(|| anyhow!("`fields` is required"))?;
            let before = connector_ids(&hub.store)?;
            let r = market::composio::connect_with_fields(
                &hub.store,
                str_arg(&b, "toolkit")?,
                str_arg(&b, "mode")?,
                fields,
            )
            .await?;
            enable_new_connectors(hub, &before)?;
            r
        }
        "composioConnection" => {
            let before = connector_ids(&hub.store)?;
            let r = market::composio::connection(&hub.store, str_arg(&b, "id")?).await?;
            enable_new_connectors(hub, &before)?;
            r
        }
        "marketConnectors" => {
            market::browse_connectors(
                &hub.store,
                b["search"].as_str().unwrap_or_default(),
                b["cursor"].as_str().unwrap_or_default(),
            )
            .await?
        }
        "marketSkills" => market::browse_skills(&hub.store).await?,
        "connectors" => market::list_connectors(&hub.store)?,
        "importConnectors" => {
            let before = connector_ids(&hub.store)?;
            let added = market::import_connectors(&hub.store, str_arg(&b, "config")?).await?;
            enable_new_connectors(hub, &before)?;
            added
        }
        "installConnector" => {
            let before = connector_ids(&hub.store)?;
            let c = if b["registryName"].is_string() {
                market::install_connector(
                    &hub.store,
                    str_arg(&b, "registryName")?,
                    str_arg(&b, "option")?,
                    &b["inputs"],
                )
                .await?
            } else {
                market::add_custom_connector(&hub.store, &b).await?
            };
            enable_new_connectors(hub, &before)?;
            json!({"connector": c})
        }
        "connectorSignIn" => {
            // The browser can only reach this host's loopback page when it runs on this computer.
            let callback = if matches!(caller, Caller::Local) {
                market::oauth::Callback::Host
            } else {
                market::oauth::Callback::App
            };
            market::oauth::start(&hub.store, hub.port, str_arg(&b, "id")?, callback).await?
        }
        "connectorSignInFinish" => {
            let c = market::oauth::finish(&hub.store, str_arg(&b, "state")?, b["code"].as_str(), b["error"].as_str())
                .await?;
            json!({"connector": c.public()})
        }
        "connectorSignOut" => json!({"connector": market::oauth::sign_out(&hub.store, str_arg(&b, "id")?)?.public()}),
        "connectorTarget" => {
            market::oauth::target(&hub.store, str_arg(&b, "id")?, b["stale"].as_bool().unwrap_or(false)).await?
        }
        "removeConnector" => {
            let id = str_arg(&b, "id")?;
            match id.strip_prefix(market::composio::PREFIX) {
                Some(toolkit) => market::composio::disconnect(&hub.store, toolkit).await?,
                None => market::remove_connector(&hub.store, id)?,
            }
            json!({})
        }
        "skills" => market::list_skills(&hub.store),
        "installSkill" => {
            let s = if b["source"].is_string() {
                market::install_skill(&hub.store, str_arg(&b, "source")?).await?
            } else {
                market::add_custom_skill(&hub.store, &b)?
            };
            json!({"skill": s})
        }
        "removeSkill" => {
            market::remove_skill(&hub.store, str_arg(&b, "id")?)?;
            json!({})
        }
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::BotConfig;

    #[tokio::test]
    async fn new_connectors_turn_on_for_every_bot() {
        let dir = std::env::temp_dir().join(format!("codync-connectors-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("memory store");
        for (id, on) in [("a", json!([])), ("b", json!(["old"]))] {
            let cfg: BotConfig = serde_json::from_value(
                json!({"id": id, "name": id, "backend": "fixture", "command": "true", "cwd": dir, "connectors": on, "notify": false}),
            )
            .expect("bot config");
            store.save_bot(&cfg).expect("save bot");
        }
        let connector = |id: &str| -> market::Connector {
            serde_json::from_value(json!({"id": id, "name": id, "url": "https://example.com/mcp"})).expect("connector")
        };
        market::save_connectors(&store, &[connector("old")]).expect("save");
        let hub = Hub::new(
            store,
            "test".into(),
            crate::remote::identity::Identity::load_or_create(&dir).expect("identity"),
            "test".into(),
            19222,
        );
        hub.start().expect("start");
        let before = connector_ids(&hub.store).expect("ids");
        market::save_connectors(&hub.store, &[connector("old"), connector("linear")]).expect("save");
        enable_new_connectors(&hub, &before).expect("enable");
        let on = |id: &str| hub.store.bot(id).expect("read").expect("bot").config.connectors;
        assert_eq!(on("a"), ["linear"], "the new connector is on; one turned off stays off");
        assert_eq!(on("b"), ["old", "linear"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
