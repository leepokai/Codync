//! A workflow webhook is one more hook in the list the cloud already stores.

#![allow(clippy::format_push_string)]

use super::model::RunSource;
use super::run;
use crate::hub::Hub;
use crate::routines::hooks::{self, Headers, MAX_BODY, Refused};
use crate::store::now_ms;
use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use serde_json::json;
use sha2::{Digest, Sha256};

struct Hook {
    workflow: String,
    node: String,
    secret: String,
    enabled: bool,
}

pub fn receive(hub: &Hub, hook_id: &str, headers: &Headers, body: &[u8]) -> Result<serde_json::Value, Refused> {
    let _gate = hub.workflows.enter();
    let Some(hook) = lookup(&hub.store, hook_id).map_err(|_| Refused::Invalid("couldn't read the webhook".into()))?
    else {
        return Err(Refused::Unauthorized);
    };
    if !hook.enabled {
        return Err(Refused::Paused);
    }
    if !hooks::authorized(&hook.secret, headers, body) {
        return Err(Refused::Unauthorized);
    }
    if body.len() > MAX_BODY {
        let _ = remember(&hub.store, &hook, &delivery(headers, body)?, "ignored", Some("refused"), None);
        return Err(Refused::Invalid("event too large".into()));
    }
    let delivery = delivery(headers, body)?;
    if let Some(previous) =
        recall(&hub.store, &hook, &delivery).map_err(|_| Refused::Invalid("couldn't read the delivery".into()))?
    {
        return previous;
    }
    if run::open_run(&hub.store, &hook.workflow)
        .map_err(|_| Refused::Invalid("couldn't read the run".into()))?
        .is_some()
    {
        let _ = remember(&hub.store, &hook, &delivery, "ignored", Some("busy"), None);
        return Err(Refused::Busy);
    }
    let payload = String::from_utf8_lossy(body).into_owned();
    match run::start(hub, &hook.workflow, &delivery, Some(&hook.node), RunSource::Webhook, &payload, None) {
        Ok(detail) => {
            let run_id = detail["run"]["id"].as_str().unwrap_or("").to_owned();
            let _ = remember(&hub.store, &hook, &delivery, "started", None, Some(&run_id));
            let _ = hub;
            Ok(json!({"ok": true, "runId": run_id}))
        }
        Err(error) if error.to_string() == "A run is already in progress" => {
            let _ = remember(&hub.store, &hook, &delivery, "ignored", Some("busy"), None);
            Err(Refused::Busy)
        }
        Err(error) => Err(Refused::Invalid(error.to_string())),
    }
}

fn delivery(headers: &Headers, body: &[u8]) -> Result<String, Refused> {
    match hooks::delivery_id(headers)? {
        Some(id) => Ok(id),
        None => Ok(Sha256::digest(body).iter().fold(String::new(), |mut out, byte| {
            out.push_str(&format!("{byte:02x}"));
            out
        })),
    }
}

fn lookup(store: &crate::store::Store, hook_id: &str) -> Result<Option<Hook>> {
    store.with_immediate(|tx| {
        tx.query_row(
            "SELECT s.workflow_id, s.node_id, s.secret, w.enabled FROM workflow_webhook_secrets s JOIN workflows w ON w.id = s.workflow_id WHERE s.hook_id = ?1",
            [hook_id],
            |r| Ok(Hook { workflow: r.get(0)?, node: r.get(1)?, secret: r.get(2)?, enabled: r.get::<_, i64>(3)? == 1 }),
        )
        .optional()
        .map_err(anyhow::Error::from)
    })
}

fn recall(
    store: &crate::store::Store,
    hook: &Hook,
    delivery: &str,
) -> Result<Option<Result<serde_json::Value, Refused>>> {
    let row: Option<(String, Option<String>, Option<String>)> = store.with_immediate(|tx| {
        tx.query_row(
            "SELECT status, reason, run_id FROM workflow_webhook_receipts WHERE workflow_id = ?1 AND node_id = ?2 AND delivery_id = ?3",
            params![hook.workflow, hook.node, delivery],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(anyhow::Error::from)
    })?;
    Ok(row.map(|(status, reason, run_id)| match (status.as_str(), reason.as_deref()) {
        ("started", _) => Ok(json!({"ok": true, "runId": run_id})),
        (_, Some("busy")) => Err(Refused::Busy),
        _ => Err(Refused::Invalid("event too large".into())),
    }))
}

fn remember(
    store: &crate::store::Store,
    hook: &Hook,
    delivery: &str,
    status: &str,
    reason: Option<&str>,
    run_id: Option<&str>,
) -> Result<()> {
    let now = now_ms();
    store.with_immediate(|tx| {
        tx.execute(
            "INSERT INTO workflow_webhook_receipts(id, workflow_id, node_id, delivery_id, status, reason, run_id, received_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(workflow_id, node_id, delivery_id) DO NOTHING",
            params![super::model::new_id(), hook.workflow, hook.node, delivery, status, reason, run_id, now],
        )?;
        Ok(())
    })
}

/// The secret and local url for one trigger, when the detail asks for them.
pub fn reveal(store: &crate::store::Store, workflow: &str, reveal: bool) -> Result<Vec<serde_json::Value>> {
    store.with_immediate(|tx| {
        let mut st =
            tx.prepare("SELECT node_id, hook_id, secret FROM workflow_webhook_secrets WHERE workflow_id = ?1")?;
        let rows = st
            .query_map([workflow], |r| {
                let node: String = r.get(0)?;
                let hook: String = r.get(1)?;
                let secret: String = r.get(2)?;
                Ok((node, hook, secret))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(node, hook, secret)| {
                let mut item = json!({"nodeId": node, "url": format!("/hooks/workflows/{hook}")});
                if reveal {
                    item["secret"] = secret.into();
                }
                item
            })
            .collect())
    })
}
