//! Marketplace methods: connectors, credentials, Composio, skills, and an agent's own gear.

use super::devices::Caller;
use super::str_arg;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::market;
use crate::market::openbot;
use crate::market::{Installed, connector_ids, enable_new};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::sync::Arc;

pub(super) async fn call(hub: &Arc<Hub>, caller: &Caller, method: &str, b: Value) -> Result<ControlFlow<Value, Value>> {
    Ok(ControlFlow::Break(match method {
        "credentialUpdateConnector" => {
            let id = str_arg(&b, "id")?;
            let previous_native = crate::native_channels::snapshot(hub, id)?;
            let fields = b["fields"].as_object().ok_or_else(|| anyhow!("Credential fields are required"))?;
            market::update_connectors(&hub.store, |all| {
                let c = all.iter_mut().find(|c| c.id == id).ok_or_else(|| anyhow!("Unknown connector"))?;
                let native = crate::native_channels::is_native(c);
                let values = if c.command.is_some() { &mut c.env } else { &mut c.headers };
                for (key, value) in fields {
                    if !values.contains_key(key) {
                        bail!("Unknown credential field");
                    }
                    let value = value
                        .as_str()
                        .filter(|s| (native || !s.is_empty()) && s.len() <= 64 * 1024)
                        .ok_or_else(|| anyhow!("Invalid credential value"))?;
                    values.insert(key.clone(), value.to_owned());
                }
                crate::native_channels::prepare_connector(c)?;
                Ok(())
            })?;
            crate::native_channels::changed(hub, previous_native)?;
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
        "credentialBotSecrets" => market::bot_secrets::list(&hub.store)?,
        "credentialRemoveBotSecret" => {
            market::bot_secrets::remove(&hub.store, str_arg(&b, "id")?)?;
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
            json!({"command":c.command,"args":c.args,"env":env,"fingerprint":crate::mcp::policy_management::fingerprint_value(&c)?})
        }
        "connectorCall" => {
            market::requests::call(hub, str_arg(&b, "botId")?, str_arg(&b, "name")?, &b["arguments"]).await?
        }
        "connectorInfo" => market::connector_info(&hub.store, str_arg(&b, "registryName")?).await?,
        "connectorRequestFinish" => market::requests::finish(hub, &b).await?,
        "connectorVerify" => market::verify::verify(&hub.store, str_arg(&b, "id")?, hub.port).await?,
        "composioPrepareCall" | "composioDispatchCall" => {
            if !matches!(caller, Caller::Local) {
                bail!("This method is restricted to the local MCP proxy.");
            }
            if method == "composioPrepareCall" {
                market::composio::prepare(hub, str_arg(&b, "botId")?, &b["arguments"], &b["rpcId"]).await?
            } else {
                market::composio::dispatch(hub, str_arg(&b, "id")?, str_arg(&b, "fingerprint")?).await?
            }
        }
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
            enable_new(hub, Installed::Connectors, &before)?;
            r
        }
        "composioConnection" => {
            let before = connector_ids(&hub.store)?;
            let r = market::composio::connection(&hub.store, str_arg(&b, "id")?).await?;
            enable_new(hub, Installed::Connectors, &before)?;
            r
        }
        "marketConnectors" => {
            let search = b["search"].as_str().unwrap_or_default();
            let cursor = b["cursor"].as_str().unwrap_or_default();
            let mut bundled = if b["includeBundledApps"] == true && cursor.is_empty() {
                market::hermes_apps::registry::listings(&hub.store, search)?
            } else {
                Vec::new()
            };
            if b["includeBundledApps"] == true && cursor.is_empty() {
                bundled.extend(crate::native_channels::apps::listings(&hub.store, search)?);
            }
            match market::browse_connectors(&hub.store, search, cursor).await {
                Ok(mut page) => {
                    bundled.extend(page["items"].as_array().into_iter().flatten().cloned());
                    page["items"] = json!(bundled);
                    page
                }
                Err(error) if !bundled.is_empty() => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "MCP Registry unavailable; showing bundled connectors"
                    );
                    json!({"items":bundled,"nextCursor":null})
                }
                Err(error) => return Err(error),
            }
        }
        "connectors" => market::list_connectors(&hub.store)?,
        "importConnectors" => {
            let before = connector_ids(&hub.store)?;
            let added = market::import_connectors(&hub.store, str_arg(&b, "config")?).await?;
            enable_new(hub, Installed::Connectors, &before)?;
            added
        }
        "installConnector" => {
            let native_before = crate::native_channels::snapshots(hub)?;
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
            enable_new(hub, Installed::Connectors, &before)?;
            for previous in native_before {
                crate::native_channels::changed(hub, Some(previous))?;
            }
            json!({"connector": c})
        }
        "connectorSignIn" => {
            // The browser can only reach this host's loopback page when it runs on this computer.
            let callback = if matches!(caller, Caller::Local) {
                market::oauth::Callback::Host
            } else {
                market::oauth::Callback::App
            };
            market::oauth::start(hub, str_arg(&b, "id")?, callback, b["showsCode"] == true).await?
        }
        "connectorSignInFinish" => {
            let c = market::oauth::finish(&hub.store, str_arg(&b, "state")?, b["code"].as_str(), b["error"].as_str())
                .await?;
            json!({"connector": c.public()})
        }
        "connectorTarget" => {
            market::oauth::target(&hub.store, str_arg(&b, "id")?, b["stale"].as_bool().unwrap_or(false)).await?
        }
        "removeConnector" => {
            let id = str_arg(&b, "id")?;
            let previous_native = crate::native_channels::snapshot(hub, id)?;
            match id.strip_prefix(market::composio::PREFIX) {
                Some(toolkit) => market::composio::disconnect(&hub.store, toolkit).await?,
                None => market::remove_connector(&hub.store, id)?,
            }
            crate::native_channels::changed(hub, previous_native)?;
            json!({})
        }
        "skills" => market::list_skills(&hub.store),
        // What this agent already has in the user's home. Codync's own skills and connectors stay on the bot.
        "agentGear" => crate::agent::gear::list(b["backend"].as_str().unwrap_or("")).await,
        "skillDetails" => match b["source"].as_str() {
            Some(source) if source.starts_with(openbot::SOURCE) => openbot::skill_details(source)?,
            Some(source) if source.starts_with(market::hermes::SOURCE) => market::hermes::skill_details(source)?,
            _ => market::skill_details(&hub.store, &b).await?,
        },
        "installSkill" => {
            let before = Installed::Skills.ids(&hub.store)?;
            let s = if b["source"].as_str().is_some_and(|s| s.starts_with(openbot::SOURCE)) {
                serde_json::to_value(openbot::install(&hub.store, str_arg(&b, "source")?)?)?
            } else if b["source"].as_str().is_some_and(|s| s.starts_with(market::hermes::SOURCE)) {
                serde_json::to_value(market::hermes::install(&hub.store, str_arg(&b, "source")?)?)?
            } else if b["source"].is_string() {
                market::install_skill(&hub.store, str_arg(&b, "source")?).await?
            } else {
                let text = |k: &str| b[k].as_str().unwrap_or_default();
                market::add_custom_skill(&hub.store, text("name"), text("description"), text("instructions"), None)?
                    .public()
            };
            enable_new(hub, Installed::Skills, &before)?;
            json!({"skill": s})
        }
        "removeSkill" => {
            market::remove_skill(&hub.store, str_arg(&b, "id")?)?;
            json!({})
        }
        _ => return Ok(ControlFlow::Continue(b)),
    }))
}
