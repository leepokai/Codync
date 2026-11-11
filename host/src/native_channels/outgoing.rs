use super::{approvals, config};
use crate::{LockExt, hub::Hub, market::Connector};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use tokio::sync::mpsc;

pub(super) async fn queue(hub: &Hub, c: &Connector, generation: &str, sends: &mpsc::Sender<Value>) -> Result<()> {
    for item in hub.store.native_candidates(&c.id, generation)? {
        let mut files = super::media::outgoing(&item).await;
        let command = {
            let _guard = hub.store.connector_lock.locked();
            ensure!(
                config::matches(&hub.store, c)? && config::generation(&hub.store, c)?.as_deref() == Some(generation),
                "Native connection changed before delivery"
            );
            let id = item["id"].as_str().unwrap_or_default();
            let command = if item["kind"] == "permission" {
                approvals::prepare(hub, c, generation, &item)?
            } else {
                let user = item["user"].as_str().unwrap_or_default();
                if config::bot(hub, c).ok().as_deref() != item["bot"].as_str()
                    || !config::authorized(hub, c, generation, user)?
                {
                    None
                } else {
                    let (files, notice) = match &files {
                        Ok(files) => (files.values(), ""),
                        Err(_) => (
                            vec![],
                            "\n\n[The attached file could not be delivered. Open the original file in Codync.]",
                        ),
                    };
                    Some(
                        json!({"id":id,"chat":item["chat"],"replyTo":item["replyTo"],"user":user,"bot":item["bot"],"text":format!("{}{notice}",item["data"]["text"].as_str().unwrap_or_default()),"files":files}),
                    )
                }
            };
            let skipped = Value::Null;
            let payload = command.as_ref().unwrap_or(&skipped);
            if !hub.store.native_reserve_send(
                id,
                &c.id,
                generation,
                payload,
                payload["code"].as_str(),
                command.is_none(),
            )? {
                continue;
            }
            if command.is_some() {
                if let Ok(files) = &mut files {
                    files.handoff();
                }
            }
            command
        };
        if let Some(mut command) = command {
            command["op"] = "send".into();
            // Once reserved, never enqueue another attempt after ambiguous transport failure.
            if sends.send(command.clone()).await.is_err() {
                if let Some(id) = command["id"].as_str() {
                    hub.store.native_finish_send(id, false, None)?;
                    super::media::cleanup_sent(&hub.store, id).await?;
                }
                anyhow::bail!("Native transport stopped before delivery");
            }
        }
    }
    Ok(())
}

pub(super) fn still_allowed(hub: &Hub, c: &Connector, generation: &str, command: &Value) -> Result<bool> {
    let _guard = hub.store.connector_lock.locked();
    Ok(config::matches(&hub.store, c)?
        && config::generation(&hub.store, c)?.as_deref() == Some(generation)
        && config::bot(hub, c).ok().as_deref() == command["bot"].as_str()
        && config::authorized(hub, c, generation, command["user"].as_str().unwrap_or_default())?)
}
