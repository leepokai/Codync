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
        // The same gate a save uses. The update only lands when this row is still the schedule we read.
        let claimed = {
            let _gate = hub.workflows.enter();
            hub.store.with_immediate(|tx| claim_due(tx, &workflow, &node, &raw, when, next, now))?
        };
        if !claimed || run::open_run(&hub.store, &workflow)?.is_some() {
            continue;
        }
        if let Err(error) = run::start(hub, &workflow, &node, Some(&node), RunSource::Schedule, "", Some(when)) {
            tracing::info!(workflow, error = format!("{error:#}"), "a scheduled workflow didn't start");
        }
    }
    hub.update_keep_awake();
    Ok(())
}

/// Moves this due time forward only when the row is still that schedule. A save that changed the
/// cron, or turned the trigger into Manual, leaves the row alone.
fn claim_due(
    tx: &rusqlite::Transaction<'_>,
    workflow: &str,
    node: &str,
    raw: &str,
    when: i64,
    next: Option<i64>,
    now: i64,
) -> Result<bool> {
    let changed = tx.execute(
        "UPDATE workflow_triggers SET next_due_at = ?5, updated_at = ?6
         WHERE workflow_id = ?1 AND node_id = ?2 AND kind = 'schedule' AND schedule_json = ?3 AND next_due_at = ?4",
        params![workflow, node, raw, when, next, now],
    )?;
    Ok(changed == 1)
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

#[cfg(test)]
mod tests {
    use super::claim_due;
    use crate::store::Store;
    use crate::workflows::model::NodeKind;
    use crate::workflows::store;

    #[test]
    fn an_old_tick_does_not_claim_a_changed_schedule() {
        let db = Store::open(std::path::Path::new(":memory:")).unwrap();
        let id = store::create(&db, "N", None, None, None).unwrap();
        let node = store::load(&db, &id)
            .unwrap()
            .unwrap()
            .1
            .nodes
            .into_iter()
            .find(|n| n.kind == NodeKind::Trigger)
            .unwrap()
            .id;
        let raw = r#"{"type":"cron","expression":"0 9 * * *","timeZone":"UTC"}"#;
        let changed = r#"{"type":"cron","expression":"0 10 * * *","timeZone":"UTC"}"#;
        db.with_immediate(|tx| {
            tx.execute(
                "UPDATE workflow_triggers SET kind = 'schedule', schedule_json = ?3, next_due_at = ?4, updated_at = ?5 WHERE workflow_id = ?1 AND node_id = ?2",
                rusqlite::params![id, node, raw, 10_i64, 1_i64],
            )?;
            assert!(claim_due(tx, &id, &node, raw, 10, Some(20), 11)?);
            assert!(!claim_due(tx, &id, &node, raw, 10, Some(30), 12)?);
            tx.execute(
                "UPDATE workflow_triggers SET schedule_json = ?3 WHERE workflow_id = ?1 AND node_id = ?2",
                rusqlite::params![id, node, changed],
            )?;
            assert!(!claim_due(tx, &id, &node, raw, 20, Some(40), 13)?);
            tx.execute(
                "UPDATE workflow_triggers SET kind = 'manual' WHERE workflow_id = ?1 AND node_id = ?2",
                rusqlite::params![id, node],
            )?;
            assert!(!claim_due(tx, &id, &node, changed, 20, Some(40), 14)?);
            Ok(())
        })
        .unwrap();
    }
}
