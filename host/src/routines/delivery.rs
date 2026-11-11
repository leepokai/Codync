//! Source reference: Hermes cron/delivery_queue.py and delivery_outcome.py (MIT, 10d25f8a).
//! Adaptation: reuse host connections, bounded workers and never retry a possibly completed send.

use super::{Status, outputs};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::routine_delivery::Delivery;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::Duration;

/// This marker is only applied before a single-request output has attempted a remote send.
/// Never attach it to a helper that may already have delivered an earlier text chunk.
#[derive(Debug)]
pub(super) struct NotSent(anyhow::Error);

impl std::fmt::Display for NotSent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Delivery was not sent: {}", self.0)
    }
}

impl std::error::Error for NotSent {}

pub(super) fn before_send<T>(result: Result<T>) -> Result<T> {
    result.map_err(|error| anyhow::Error::new(NotSent(error)))
}

/// The queued row proves the flow completed. Display history may rotate without revoking it.
/// Pausing/deleting cancels ordinary deliveries; explicit test runs may run while paused.
fn eligible(hub: &Hub, job: &Delivery) -> bool {
    let state = hub.routines.state.locked();
    let routine = state.routines.iter().find(|r| r.id == job.routine);
    let run = state.runs.iter().find(|r| r.id == job.run && r.routine_id == job.routine);
    routine.is_some_and(|r| !r.deleted && super::may_run(r, &job.event))
        && run.is_none_or(|r| r.status == Status::Succeeded)
}

impl super::Routines {
    pub(super) fn flow_allowed(&self, start: &super::flow::Start) -> bool {
        self.state
            .locked()
            .routines
            .iter()
            .any(|r| r.id == start.routine_id && !r.deleted && super::may_run(r, &start.event))
    }

    pub(super) fn stage_flow(&self, store: &crate::store::Store, start: super::flow::Start) -> Result<()> {
        self.change(store, |state| {
            if !state.pending_flows.iter().any(|flow| flow.run_id == start.run_id) {
                state.pending_flows.push(start);
            }
            Ok(())
        })
    }

    pub(super) fn ack_flow(&self, store: &crate::store::Store, run: &str) -> Result<()> {
        self.change(store, |state| {
            state.pending_flows.retain(|flow| flow.run_id != run);
            Ok(())
        })
    }

    pub(super) fn cancel_flow_deliveries(&self, store: &crate::store::Store, routine: &str) -> Result<()> {
        self.change(store, |state| {
            store.cancel_routine_outputs(routine)?;
            state.pending_flows.retain(|flow| flow.routine_id != routine);
            Ok(())
        })
    }

    pub(super) fn queue_flow_outputs(
        &self,
        store: &crate::store::Store,
        start: &super::flow::Start,
        text: &str,
    ) -> Result<()> {
        if text.trim().is_empty() || start.outputs.is_empty() {
            return Ok(());
        }
        // Share the state lock with Stop, so cancellation cannot pass between validation and
        // publication. A persisted intent is required, including for explicit test flows.
        let state = self.state.locked();
        if state.pending_flows.iter().any(|flow| flow.run_id == start.run_id)
            && state.routines.iter().any(|r| r.id == start.routine_id && !r.deleted && super::may_run(r, &start.event))
        {
            store.queue_routine_outputs(&start.run_id, &start.routine_id, &start.outputs, &start.event, text)?;
        }
        Ok(())
    }
}

/// Recovery marks interrupted steps failed first. Replaying snapshots reads recorded results
/// instead of executing steps twice, then idempotently enqueues the original destinations.
pub(super) fn resume_flows(hub: &Arc<Hub>) {
    let pending = hub.routines.state.locked().pending_flows.clone();
    for start in pending {
        super::flow::spawn(hub, start);
    }
}

/// Only failures known to precede sending are retryable. Once a request has left the host,
/// a network error is ambiguous: automatic retry could duplicate a report or create two issues.
fn connected(hub: &Hub, job: &Delivery) -> Result<bool> {
    let store = &hub.store;
    Ok(match job.output.kind.as_str() {
        "slack" => crate::slack::credentials(store)?.is_some(),
        "discord" | "discordReact" => crate::discord::credentials(store)?.is_some(),
        "whatsapp" => crate::whatsapp::delivery::connected(hub, job.output.account.as_deref()),
        "telegram" => {
            let name = job.output.account.as_deref().filter(|n| !n.is_empty());
            let name = match name {
                Some(name) => Some(name.to_owned()),
                None => crate::telegram::first_name(store)?,
            };
            match name {
                Some(name) => crate::telegram::token(store, &name)?.is_some(),
                None => false,
            }
        }
        _ => crate::github::connection(store)?.is_some(),
    })
}

/// Called by the scheduler. Claims happen before spawning, so later ticks cannot launch the
/// same send. Four permits bound both platform requests and the resident report payloads.
pub(super) fn dispatch(hub: &Arc<Hub>) -> Result<()> {
    if hub.routines.stopping() {
        return Ok(());
    }
    for job in hub.store.pending_routine_outputs()? {
        if !eligible(hub, &job) || job.content.trim().is_empty() || outputs::problem(&job.output).is_some() {
            hub.store.suppress_routine_output(&job)?;
            continue;
        }
        if job.output.kind == "whatsapp" {
            match crate::whatsapp::delivery::authorized(hub, &job.output, &job.event) {
                Ok(true) => {}
                Ok(false) => {
                    hub.store.suppress_routine_output(&job)?;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(%error, "WhatsApp routine authorization could not be read");
                    hub.store.defer_routine_output(&job)?;
                    continue;
                }
            }
        }
        match connected(hub, &job) {
            Ok(true) => {}
            result => {
                if let Err(error) = result {
                    tracing::warn!(run = job.run, %error, "routine delivery connection could not be read");
                }
                hub.store.defer_routine_output(&job)?;
                continue;
            }
        }
        let Ok(permit) = hub.routines.output_workers.clone().try_acquire_owned() else { break };
        if !hub.store.claim_routine_output(&job)? {
            continue;
        }
        let hub = hub.clone();
        tokio::spawn(async move {
            let _permit = permit;
            send(&hub, job).await;
        });
    }
    Ok(())
}

async fn cancelled(hub: &Hub, job: &Delivery) {
    loop {
        if hub.routines.stopping() || !eligible(hub, job) {
            return;
        }
        match hub.store.routine_output_status(&job.run, job.position) {
            Ok(Some(status)) if status == "sending" => {}
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(run = job.run, %error, "routine delivery cancellation state unavailable");
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn send(hub: &Arc<Hub>, job: Delivery) {
    let operation = async {
        if job.output.kind == "whatsapp" {
            return outputs::one(hub, &job.output, &job.event, &job.content).await;
        }
        // Resolve before entering helpers that can split text into several sends. A failure in
        // this phase is certainly unsent; a failure after the helper starts remains ambiguous.
        let host = match job.output.kind.as_str() {
            "slack" => "slack.com",
            "telegram" => "api.telegram.org",
            "discord" | "discordReact" => "discord.com",
            _ => "api.github.com",
        };
        let url = format!("https://{host}/");
        let policy = match host {
            "slack.com" => crate::egress::Policy::Only(&["slack.com"]),
            "api.telegram.org" => crate::egress::Policy::Only(&["api.telegram.org"]),
            "discord.com" => crate::egress::Policy::Only(&["discord.com"]),
            _ => crate::egress::Policy::Only(&["api.github.com"]),
        };
        let prepared = tokio::time::timeout(Duration::from_secs(10), crate::egress::client(policy, &url)).await;
        before_send(
            prepared
                .map_err(|_| anyhow!("Platform address resolution timed out before sending"))
                .and_then(|client| client),
        )?;
        outputs::one(hub, &job.output, &job.event, &job.content).await
    };
    send_with(hub, &job, operation).await;
}

async fn send_with(hub: &Arc<Hub>, job: &Delivery, operation: impl std::future::Future<Output = Result<()>>) {
    let result = tokio::select! {
        biased;
        () = cancelled(hub, job) => Err(anyhow!("Delivery was cancelled; its remote outcome may be unknown")),
        result = tokio::time::timeout(Duration::from_secs(60), operation) => {
            result.unwrap_or_else(|_| Err(anyhow!("Delivery timed out; its remote outcome is unknown")))
        }
    };
    if result.as_ref().is_err_and(|error| error.is::<NotSent>() || error.is::<crate::whatsapp::delivery::Unsent>()) {
        if let Err(error) = hub.store.retry_unsent_routine_output(job) {
            tracing::error!(run = job.run, %error, "could not defer an unsent routine output");
        }
        return;
    }
    if let Err(error) = &result {
        tracing::warn!(run = job.run, kind = job.output.kind, %error,
            "routine delivery outcome is unknown; it will not be sent twice automatically");
    }
    if let Err(error) = hub.store.finish_routine_output(job, result.is_ok()) {
        // Leave the claim intact. Recovery marks it unknown, never pending.
        tracing::error!(run = job.run, %error, "could not save routine delivery outcome");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::Store;
    use serde_json::json;

    #[test]
    fn hermes_flow_intent_survives_restart_and_stop_revokes_late_enqueue() {
        let root = std::env::temp_dir().join(format!("codync-delivery-intent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "delivery-test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        let mut routine = super::super::tests::routine();
        routine.outputs.push(super::super::Output {
            kind: "telegram".into(),
            target: "123".into(),
            account: None,
            value: None,
        });
        let mut run = super::super::new_run(&routine, json!({}), None);
        run.status = Status::Running;
        let start =
            super::super::flow::start_for(Some(&routine), &run, Status::Succeeded, None, Some("report")).unwrap();
        hub.routines
            .change(&hub.store, |state| {
                state.routines.push(routine);
                state.runs.push(run);
                Ok(())
            })
            .unwrap();
        // No async runtime is present: this models a crash after complete persisted the intent
        // and before its background flow could start or enqueue any platform delivery.
        hub.routines.complete(&hub, &start.run_id, Status::Succeeded, None, Some("report".into()), false).unwrap();
        hub.routines.restore(&hub.store).unwrap();
        assert_eq!(hub.routines.state.locked().pending_flows.len(), 1);
        hub.routines.queue_flow_outputs(&hub.store, &start, "report").unwrap();
        assert_eq!(hub.store.pending_routine_outputs().unwrap().len(), 1);
        hub.routines.cancel_queued(&hub.store, "bot").unwrap();
        hub.routines.queue_flow_outputs(&hub.store, &start, "late report").unwrap();
        assert!(hub.store.pending_routine_outputs().unwrap().is_empty());
        assert_eq!(hub.store.routine_output_status(&start.run_id, 0).unwrap().as_deref(), Some("suppressed"));
        hub.routines.restore(&hub.store).unwrap();
        assert!(hub.routines.state.locked().pending_flows.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn hermes_delivery_worker_distinguishes_unsent_ambiguous_successful_and_cancelled_outcomes() {
        let root = std::env::temp_dir().join(format!("codync-delivery-worker-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "delivery-test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        let routine = super::super::tests::routine();
        let output = super::super::Output { kind: "telegram".into(), target: "123".into(), account: None, value: None };
        hub.routines.state.locked().routines.push(routine);
        for run in ["unsent", "ambiguous", "successful", "cancelled"] {
            hub.store
                .queue_routine_outputs(run, "routine", std::slice::from_ref(&output), &json!({}), "report")
                .unwrap();
        }
        let jobs = hub.store.pending_routine_outputs().unwrap();
        let job = |run: &str| jobs.iter().find(|job| job.run == run).unwrap();
        let unsent = job("unsent");
        assert!(hub.store.claim_routine_output(unsent).unwrap());
        send_with(&hub, unsent, async { before_send(Err(anyhow!("DNS unavailable"))) }).await;
        assert_eq!(hub.store.routine_output_status("unsent", 0).unwrap().as_deref(), Some("pending"));
        hub.store
            .with_connection(|c| {
                let content: String =
                    c.query_row("SELECT content FROM routine_deliveries WHERE run_id='unsent'", [], |r| r.get(0))?;
                assert_eq!(content, "report");
                Ok(())
            })
            .unwrap();
        let ambiguous = job("ambiguous");
        assert!(hub.store.claim_routine_output(ambiguous).unwrap());
        send_with(&hub, ambiguous, async { Err(anyhow!("Response lost after sending")) }).await;
        assert_eq!(hub.store.routine_output_status("ambiguous", 0).unwrap().as_deref(), Some("unknown"));
        let successful = job("successful");
        assert!(hub.store.claim_routine_output(successful).unwrap());
        send_with(&hub, successful, async { Ok(()) }).await;
        assert_eq!(hub.store.routine_output_status("successful", 0).unwrap().as_deref(), Some("delivered"));
        let cancelled_job = job("cancelled");
        assert!(hub.store.claim_routine_output(cancelled_job).unwrap());
        tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(send_with(&hub, cancelled_job, std::future::pending::<Result<()>>()), async {
                hub.routines.cancel_flow_deliveries(&hub.store, "routine").unwrap();
            });
        })
        .await
        .unwrap();
        assert_eq!(hub.store.routine_output_status("cancelled", 0).unwrap().as_deref(), Some("unknown"));
        assert!(hub.store.pending_routine_outputs().unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hermes_delivery_survives_history_rotation_but_obeys_explicit_cancellation_and_pause() {
        let root = std::env::temp_dir().join(format!("codync-delivery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "delivery-test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        let routine = super::super::tests::routine();
        let mut run = super::super::new_run(&routine, json!({}), None);
        run.status = Status::Succeeded;
        let job = Delivery {
            run: run.id.clone(),
            position: 0,
            routine: routine.id.clone(),
            output: super::super::Output { kind: "telegram".into(), target: "123".into(), account: None, value: None },
            event: json!({}),
            content: "report".into(),
        };
        assert!(!eligible(&hub, &job));
        {
            let mut state = hub.routines.state.locked();
            state.routines.push(routine);
            state.runs.push(run);
        }
        assert!(eligible(&hub, &job));
        hub.routines.state.locked().runs[0].status = Status::Cancelled;
        assert!(!eligible(&hub, &job));
        hub.routines.state.locked().runs[0].status = Status::Succeeded;
        hub.routines.state.locked().runs.clear();
        assert!(eligible(&hub, &job));
        hub.routines.state.locked().routines[0].enabled = false;
        assert!(!eligible(&hub, &job));
        let mut explicit_test = job;
        explicit_test.run = "flow-test-synthetic".into();
        explicit_test.event = json!({"source":"test"});
        assert!(eligible(&hub, &explicit_test));
        std::fs::remove_dir_all(root).unwrap();
    }
}
