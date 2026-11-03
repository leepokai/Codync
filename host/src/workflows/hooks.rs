//! A workflow webhook is one more hook in the list the cloud already stores.

use super::model::RunSource;
use super::run;
use crate::hub::Hub;
use crate::routines::hooks::{self, Headers, Refused};
use crate::store::now_ms;
use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use serde_json::json;

struct Hook {
    workflow: String,
    node: String,
    secret: String,
}

pub fn receive(hub: &Hub, hook_id: &str, headers: &Headers, body: &[u8]) -> Result<serde_json::Value, Refused> {
    // The secret is checked before the workflow gate, and before a paused workflow is told apart
    // from an unknown one. `run::start` takes that gate itself and drops it before a script runs.
    let Some(hook) = lookup(&hub.store, hook_id).map_err(|_| Refused::Invalid("couldn't read the webhook".into()))?
    else {
        return Err(Refused::Unauthorized);
    };
    if !hooks::authorized(&hook.secret, headers, body) {
        return Err(Refused::Unauthorized);
    }
    let Some(event) = hooks::event(headers, body)? else {
        return Ok(json!({"ping": true}));
    };
    let delivery = hooks::delivery_id(headers)?;
    if let Some(id) = delivery.as_deref()
        && let Some(previous) =
            recall(&hub.store, &hook, id).map_err(|_| Refused::Invalid("couldn't read the delivery".into()))?
    {
        return previous;
    }
    let request = delivery.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
    match run::start(hub, &hook.workflow, &request, Some(&hook.node), RunSource::Webhook, &payload, None) {
        Ok(detail) => {
            let run_id = detail["run"]["id"].as_str().unwrap_or("").to_owned();
            if let Some(id) = delivery.as_deref() {
                let _ = remember(&hub.store, &hook, id, "started", None, Some(&run_id));
            }
            Ok(json!({"ok": true, "runId": run_id}))
        }
        Err(error) if error.to_string() == "This workflow is paused" => Err(Refused::Paused),
        Err(error) if error.to_string() == "A run is already in progress" => {
            if let Some(id) = delivery.as_deref() {
                let _ = remember(&hub.store, &hook, id, "ignored", Some("busy"), None);
            }
            Err(Refused::Busy)
        }
        Err(error) => Err(Refused::Invalid(error.to_string())),
    }
}

fn lookup(store: &crate::store::Store, hook_id: &str) -> Result<Option<Hook>> {
    store.with_immediate(|tx| {
        tx.query_row(
            "SELECT s.workflow_id, s.node_id, s.secret FROM workflow_webhook_secrets s JOIN workflows w ON w.id = s.workflow_id WHERE s.hook_id = ?1",
            [hook_id],
            |r| Ok(Hook { workflow: r.get(0)?, node: r.get(1)?, secret: r.get(2)? }),
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
    Ok(row.and_then(|(status, reason, run_id)| match (status.as_str(), reason.as_deref()) {
        ("started", _) => Some(Ok(json!({"ok": true, "runId": run_id}))),
        (_, Some("busy")) => None,
        _ => Some(Err(Refused::Invalid("event too large".into()))),
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
             ON CONFLICT(workflow_id, node_id, delivery_id) DO UPDATE SET status = excluded.status, reason = excluded.reason, run_id = excluded.run_id, received_at = excluded.received_at
             WHERE workflow_webhook_receipts.reason = 'busy'",
            params![super::model::new_id(), hook.workflow, hook.node, delivery, status, reason, run_id, now],
        )?;
        Ok(())
    })
}

/// The secret and local url for one trigger, when the detail asks for them.
pub fn reveal(
    store: &crate::store::Store,
    workflow: &str,
    reveal: bool,
    local_base: &str,
    public_base: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
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
                let mut item = json!({
                    "nodeId": node,
                    "url": public_base.map(|base| format!("{}/{}", base.trim_end_matches('/'), hook)),
                    "localUrl": format!("{}/hooks/workflows/{hook}", local_base.trim_end_matches('/')),
                });
                if reveal {
                    item["secret"] = secret.into();
                }
                item
            })
            .collect())
    })
}
