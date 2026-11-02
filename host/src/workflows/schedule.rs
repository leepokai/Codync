//! A paused workflow doesn't fire, and a busy one skips to the following time.

use super::model::RunSource;
use super::run;
use crate::hub::Hub;
use crate::routines::Trigger;
use crate::store::now_ms;
use anyhow::Result;
use rusqlite::params;

pub fn tick(hub: &Hub) -> Result<()> {
    let now = now_ms();
    let due = hub.store.with_immediate(|tx| {
        let mut st = tx.prepare(
            "SELECT t.workflow_id, t.node_id, t.schedule_json, t.next_due_at
             FROM workflow_triggers t JOIN workflows w ON w.id = t.workflow_id
             WHERE w.enabled = 1 AND t.kind = 'schedule' AND t.next_due_at IS NOT NULL AND t.next_due_at <= ?1",
        )?;
        let rows = st
            .query_map([now], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    for (workflow, node, raw, when) in due {
        let Ok(cron) = serde_json::from_str::<Trigger>(&raw) else { continue };
        let next = cron.next(now).ok().flatten();
        let _ = hub.store.with_immediate(|tx| {
            tx.execute("UPDATE workflow_triggers SET next_due_at = ?3, updated_at = ?4 WHERE workflow_id = ?1 AND node_id = ?2", params![workflow, node, next, now])?;
            Ok(())
        });
        if run::open_run(&hub.store, &workflow)?.is_some() {
            continue;
        }
        if let Err(error) = run::start(hub, &workflow, &node, Some(&node), RunSource::Schedule, "", Some(when)) {
            tracing::info!(workflow, error = format!("{error:#}"), "a scheduled workflow didn't start");
        }
    }
    hub.update_keep_awake();
    Ok(())
}

pub fn keeps_awake(store: &crate::store::Store) -> bool {
    store
        .with_immediate(|tx| {
            tx.query_row(
                "SELECT 1 FROM workflow_triggers t JOIN workflows w ON w.id = t.workflow_id WHERE w.enabled = 1 AND t.kind = 'schedule' AND t.next_due_at IS NOT NULL LIMIT 1",
                [],
                |_| Ok(true),
            )
            .optional_or()
        })
        .unwrap_or(false)
}

trait OptionalOr {
    fn optional_or(self) -> Result<bool>;
}

impl OptionalOr for rusqlite::Result<bool> {
    fn optional_or(self) -> Result<bool> {
        match self {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}
