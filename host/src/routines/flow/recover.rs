//! At startup: the steps a restart interrupted fail, and what hung below them is skipped.

use super::{engine::record, graph};
use crate::hub::Hub;
use crate::store::{FlowRun, NewStep, StepStatus};
use anyhow::Result;
use std::collections::HashSet;

const RESTARTED: &str = "The host restarted while this step was running; its result was lost";
const INTERRUPTED: &str = "Skipped: the host restarted before the flow reached this bot";

pub fn recover(hub: &Hub) -> Result<()> {
    let orphans = hub.emit_flow_change(|| hub.store.flow_fail_orphans(RESTARTED))?;
    if !orphans.is_empty() {
        tracing::warn!(steps = orphans.len(), "routine flow steps interrupted by a restart were marked failed");
    }
    for run in hub.store.flow_runs()? {
        skip_unreached(hub, &run)?;
    }
    Ok(())
}

/// A flow the restart cut short: the bots its links reach that never got a step are skipped.
fn skip_unreached(hub: &Hub, run: &FlowRun) -> Result<()> {
    let links: Vec<_> =
        hub.store.flow_links(&run.routine_id)?.into_iter().filter(|l| l.created_at <= run.run_at).collect();
    let done: HashSet<String> = hub.store.flow_run_steps(&run.run_id)?.into_iter().map(|s| s.bot_id).collect();
    for bot in graph::reached(&run.owner_bot_id, &links).iter().filter(|b| !done.contains(*b)) {
        let step = NewStep {
            run_id: &run.run_id,
            routine_id: &run.routine_id,
            owner_bot_id: &run.owner_bot_id,
            bot_id: bot,
            run_at: run.run_at,
            input: "",
            status: StepStatus::Skipped,
            output: None,
            error: Some(INTERRUPTED),
        };
        record(hub, &step)?;
    }
    Ok(())
}
