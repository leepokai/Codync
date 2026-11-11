//! Source reference: Hermes `cron/delivery_queue.py` (MIT, 10d25f8a).
//! Adaptation: transactional per-run destinations in the host database; ambiguous sends never retry.

use super::Store;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

pub(super) fn open(c: &Connection) -> Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS routine_deliveries(
            run_id TEXT NOT NULL, position INTEGER NOT NULL, routine_id TEXT NOT NULL,
            output TEXT NOT NULL, event TEXT NOT NULL, content TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending','sending','delivered','unknown','suppressed')),
            created_at INTEGER NOT NULL, not_before INTEGER NOT NULL DEFAULT 0, finished_at INTEGER,
            PRIMARY KEY(run_id,position));
         CREATE INDEX IF NOT EXISTS routine_deliveries_pending
            ON routine_deliveries(status,created_at);",
    )?;
    Ok(())
}

pub(crate) struct Delivery {
    pub run: String,
    pub position: i64,
    pub routine: String,
    pub output: crate::routines::Output,
    pub event: Value,
    pub content: String,
}

impl Store {
    /// Capture authorization when work starts, before its durable flow intent exists.
    pub(crate) fn snapshot_whatsapp_output(&self, event: &Value) -> Result<Value> {
        self.with_connection(|c| {
            let (account, epoch): (Option<String>, Option<String>) = c.query_row(
                "SELECT (SELECT v FROM kv WHERE k='whatsapp.link.account'),
                        (SELECT v FROM kv WHERE k='whatsapp.link.generation')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let mut bound = if event.is_object() { event.clone() } else { serde_json::json!({"triggerData":event}) };
            bound["whatsappBinding"] = serde_json::json!([account, epoch]);
            Ok(bound)
        })
    }
    /// Enqueue all destinations atomically. Retained terminal rows are idempotency tombstones:
    /// replaying the same run cannot send it again, even after its content has been erased.
    pub(crate) fn queue_routine_outputs(
        &self,
        run: &str,
        routine: &str,
        outputs: &[crate::routines::Output],
        event: &Value,
        content: &str,
    ) -> Result<()> {
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            let event = serde_json::to_string(event)?;
            for (position, output) in outputs.iter().enumerate() {
                tx.execute(
                    "INSERT OR IGNORE INTO routine_deliveries
                     (run_id,position,routine_id,output,event,content,status,created_at)
                     VALUES(?1,?2,?3,?4,?5,?6,'pending',?7)",
                    params![
                        run,
                        i64::try_from(position)?,
                        routine,
                        serde_json::to_string(output)?,
                        event,
                        content,
                        super::now_ms()
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub(crate) fn pending_routine_outputs(&self) -> Result<Vec<Delivery>> {
        self.with_connection(|c| {
            let mut statement = c.prepare(
                "SELECT run_id,position,routine_id,output,event,content FROM routine_deliveries
                 WHERE status='pending' AND not_before<=?1 ORDER BY created_at,run_id,position LIMIT 32",
            )?;
            let rows = statement.query_map([super::now_ms()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?;
            let mut deliveries = Vec::new();
            for row in rows {
                let (run, position, routine, output, event, content) = row?;
                deliveries.push(Delivery {
                    run,
                    position,
                    routine,
                    output: serde_json::from_str(&output)?,
                    event: serde_json::from_str(&event)?,
                    content,
                });
            }
            Ok(deliveries)
        })
    }

    pub(crate) fn claim_routine_output(&self, delivery: &Delivery) -> Result<bool> {
        self.with_connection(|c| {
            Ok(c.execute(
                "UPDATE routine_deliveries SET status='sending' WHERE run_id=?1 AND position=?2 AND status='pending'",
                params![delivery.run, delivery.position],
            )? == 1)
        })
    }

    pub(crate) fn defer_routine_output(&self, delivery: &Delivery) -> Result<()> {
        self.with_connection(|c| {
            c.execute(
                "UPDATE routine_deliveries SET not_before=?3 WHERE run_id=?1 AND position=?2 AND status='pending'",
                params![delivery.run, delivery.position, super::now_ms() + 60_000],
            )?;
            Ok(())
        })
    }

    pub(crate) fn finish_routine_output(&self, delivery: &Delivery, delivered: bool) -> Result<()> {
        self.with_connection(|c| {
            c.execute(
                "UPDATE routine_deliveries SET status=?3,output='{}',event='{}',content='',finished_at=?4
                 WHERE run_id=?1 AND position=?2 AND status='sending'",
                params![
                    delivery.run,
                    delivery.position,
                    if delivered { "delivered" } else { "unknown" },
                    super::now_ms()
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn retry_unsent_routine_output(&self, delivery: &Delivery) -> Result<()> {
        self.with_connection(|c| {
            c.execute(
                "UPDATE routine_deliveries SET status='pending',not_before=?3
                 WHERE run_id=?1 AND position=?2 AND status='sending'",
                params![delivery.run, delivery.position, super::now_ms() + 60_000],
            )?;
            Ok(())
        })
    }

    pub(crate) fn suppress_routine_output(&self, delivery: &Delivery) -> Result<()> {
        self.with_connection(|c| {
            c.execute(
                "UPDATE routine_deliveries SET status='suppressed',output='{}',event='{}',content='',finished_at=?3
                 WHERE run_id=?1 AND position=?2 AND status='pending'",
                params![delivery.run, delivery.position, super::now_ms()],
            )?;
            Ok(())
        })
    }

    /// A previous host may have sent before it crashed. Never infer failure from a missing receipt.
    pub(crate) fn recover_routine_outputs(&self) -> Result<()> {
        self.with_connection(|c| {
            c.execute(
                "UPDATE routine_deliveries SET status='unknown',output='{}',event='{}',content='',finished_at=?1
                 WHERE status='sending'",
                [super::now_ms()],
            )?;
            Ok(())
        })
    }

    pub(crate) fn routine_output_status(&self, run: &str, position: i64) -> Result<Option<String>> {
        self.with_connection(|c| {
            Ok(c.query_row(
                "SELECT status FROM routine_deliveries WHERE run_id=?1 AND position=?2",
                params![run, position],
                |r| r.get(0),
            )
            .optional()?)
        })
    }

    pub(crate) fn cancel_routine_outputs(&self, routine: &str) -> Result<()> {
        self.with_connection(|c| {
            c.execute(
                "UPDATE routine_deliveries SET status=CASE WHEN status='pending' THEN 'suppressed' ELSE 'unknown' END,
                 output='{}',event='{}',content='',finished_at=?2
                 WHERE routine_id=?1 AND status IN ('pending','sending')",
                params![routine, super::now_ms()],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hermes_delivery_claims_are_exclusive_and_replayed_runs_do_not_duplicate() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let output =
            crate::routines::Output { kind: "telegram".into(), target: "123".into(), account: None, value: None };
        for _ in 0..2 {
            store.queue_routine_outputs("run", "routine", std::slice::from_ref(&output), &json!({}), "report").unwrap();
        }
        let jobs = store.pending_routine_outputs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(store.claim_routine_output(&jobs[0]).unwrap());
        assert!(!store.claim_routine_output(&jobs[0]).unwrap());
        store.finish_routine_output(&jobs[0], true).unwrap();
        store.queue_routine_outputs("run", "routine", &[output], &json!({}), "report").unwrap();
        assert!(store.pending_routine_outputs().unwrap().is_empty());
        assert_eq!(store.routine_output_status("run", 0).unwrap().as_deref(), Some("delivered"));
    }

    #[test]
    fn hermes_delivery_definitely_unsent_retries_preserve_payload_and_cancel_cannot_requeue() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let output =
            crate::routines::Output { kind: "slack".into(), target: "C123".into(), account: None, value: None };
        store.queue_routine_outputs("run", "routine", &[output], &json!({}), "report").unwrap();
        let job = store.pending_routine_outputs().unwrap().remove(0);
        assert!(store.claim_routine_output(&job).unwrap());
        store.retry_unsent_routine_output(&job).unwrap();
        assert!(store.pending_routine_outputs().unwrap().is_empty(), "retry must respect its delay");
        store
            .with_connection(|c| {
                c.execute("UPDATE routine_deliveries SET not_before=0", [])?;
                Ok(())
            })
            .unwrap();
        assert_eq!(store.pending_routine_outputs().unwrap()[0].content, "report");
        assert!(store.claim_routine_output(&job).unwrap());
        store.cancel_routine_outputs("routine").unwrap();
        store.retry_unsent_routine_output(&job).unwrap();
        assert_eq!(store.routine_output_status("run", 0).unwrap().as_deref(), Some("unknown"));
        assert!(store.pending_routine_outputs().unwrap().is_empty());
    }

    #[test]
    fn hermes_delivery_restart_preserves_unsent_jobs_but_never_retries_ambiguous_sends() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let output =
            crate::routines::Output { kind: "slack".into(), target: "C123".into(), account: None, value: None };
        store.queue_routine_outputs("run", "routine", &[output.clone(), output], &json!({}), "secret report").unwrap();
        let jobs = store.pending_routine_outputs().unwrap();
        store.claim_routine_output(&jobs[0]).unwrap();
        store.recover_routine_outputs().unwrap();
        assert_eq!(store.routine_output_status("run", 0).unwrap().as_deref(), Some("unknown"));
        let pending = store.pending_routine_outputs().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].position, 1);
        store.suppress_routine_output(&pending[0]).unwrap();
        assert!(store.pending_routine_outputs().unwrap().is_empty());
        store
            .with_connection(|c| {
                let content: String =
                    c.query_row("SELECT content FROM routine_deliveries WHERE position=0", [], |r| r.get(0))?;
                assert_eq!(content, "");
                Ok(())
            })
            .unwrap();
    }
}
