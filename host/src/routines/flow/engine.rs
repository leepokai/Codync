//! Running a flow: after a routine's run ends, level by level from the routine's own bot, each
//! bot with the replies of the bots linked to it.

use super::graph;
use crate::chat::team::flow::{FlowCancelled, Handoff, handoff, recipient};
use crate::hub::Hub;
use crate::routines::{Routine, Run, Status};
use crate::store::{FlowLink, FlowStep, NewStep, StepStatus};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinSet;

/// How the routine's own run ended.
pub enum Root {
    Replied(String),
    Failed(String),
    Cancelled(String),
}

/// One routine run whose flow should run.
pub struct Start {
    pub run_id: String,
    pub routine_id: String,
    pub routine_name: String,
    pub owner: String,
    /// When the run began: only the links made before apply.
    pub run_at: i64,
    /// What each step may take, inherited from the routine.
    pub timeout: Duration,
    /// How many hops from the routine's own bot the flow may reach.
    pub max_depth: usize,
    /// The routine's instruction, the root step's input.
    pub instruction: String,
    pub root: Root,
}

/// What a bot's step left for the bots after it.
#[derive(Clone)]
enum Outcome {
    Replied(String),
    /// Nothing to pass on: the bot had nothing to report, or the step didn't run or finish.
    Nothing,
}

/// A step ready to be put to its bot.
struct Pending {
    step: FlowStep,
    from: crate::store::BotConfig,
    bot: String,
    body: String,
    hops: u8,
}

enum Begin {
    Send(Box<Pending>),
    Done(Outcome),
}

pub async fn execute(hub: Arc<Hub>, start: Start) {
    let (run, routine) = (start.run_id.clone(), start.routine_id.clone());
    if let Err(error) = run_flow(&hub, Arc::new(start)).await {
        tracing::error!(run, routine, error = format!("{error:#}"), "routine flow failed");
    }
}

/// Saves a step (or finds the one a retry already made: `None`) and tells the clients.
pub(super) fn record(hub: &Hub, new: &NewStep) -> Result<Option<FlowStep>> {
    hub.emit_flow_change(|| {
        Ok(match hub.store.flow_add_step(new)? {
            Some((step, change)) => (Some(step), Some(change)),
            None => (None, None),
        })
    })
}

fn settle(hub: &Hub, id: &str, status: StepStatus, output: Option<&str>, error: Option<&str>) {
    if let Err(e) = hub.emit_flow_change(|| Ok(((), hub.store.flow_settle_step(id, status, output, error)?))) {
        tracing::error!(step = id, error = format!("{e:#}"), "couldn't save a routine flow step");
    }
}

fn new_step<'a>(start: &'a Start, bot: &'a str, input: &'a str, status: StepStatus) -> NewStep<'a> {
    NewStep {
        run_id: &start.run_id,
        routine_id: &start.routine_id,
        owner_bot_id: &start.owner,
        bot_id: bot,
        run_at: start.run_at,
        input,
        status,
        output: None,
        error: None,
    }
}

fn record_root(hub: &Hub, start: &Start) -> Result<()> {
    let mut step = new_step(start, &start.owner, &start.instruction, StepStatus::Succeeded);
    match &start.root {
        Root::Replied(text) => step.output = Some(text),
        Root::Failed(error) => (step.status, step.error) = (StepStatus::Failed, Some(error.as_str())),
        Root::Cancelled(error) => (step.status, step.error) = (StepStatus::Cancelled, Some(error.as_str())),
    }
    record(hub, &step).map(drop)
}

/// Ends a step that never ran, with `reason`.
pub(super) fn record_ended(hub: &Hub, start: &Start, bot: &str, status: StepStatus, reason: &str) -> Result<()> {
    let mut step = new_step(start, bot, "", status);
    step.error = Some(reason);
    record(hub, &step).map(drop)
}

fn skip_all<'a>(hub: &Hub, start: &Start, bots: impl IntoIterator<Item = &'a str>, reason: &str) -> Result<()> {
    bots.into_iter().try_for_each(|bot| record_ended(hub, start, bot, StepStatus::Skipped, reason))
}

async fn run_flow(hub: &Arc<Hub>, start: Arc<Start>) -> Result<()> {
    let links: Vec<FlowLink> =
        hub.store.flow_links(&start.routine_id)?.into_iter().filter(|l| l.created_at <= start.run_at).collect();
    if links.is_empty() {
        return Ok(());
    }
    let running = hub.routines.flows.register(&start.run_id, &start.routine_id, &start.owner);
    record_root(hub, &start)?;
    let Root::Replied(reply) = &start.root else {
        let reached = graph::reached(&start.owner, &links);
        return skip_all(
            hub,
            &start,
            reached.iter().map(String::as_str),
            "Skipped: the routine's own run didn't finish",
        );
    };
    let Ok(depths) = graph::depths(&start.owner, &links) else {
        // Connecting refuses cycles, so this means the stored links were changed behind the host's back.
        tracing::error!(
            run = start.run_id,
            routine = start.routine_id,
            "routine flow links form a cycle; nothing was handed on"
        );
        let reached = graph::reached(&start.owner, &links);
        return skip_all(hub, &start, reached.iter().map(String::as_str), "Skipped: the routine's links form a loop");
    };
    let mut results = HashMap::from([(start.owner.clone(), Outcome::Replied(reply.clone()))]);
    let order = graph::order(&depths);
    let deepest = order.last().map_or(0, |(_, depth)| *depth);
    for depth in 1..=deepest {
        let mut tasks = JoinSet::new();
        for (bot, _) in order.iter().filter(|(_, d)| *d == depth) {
            match begin(hub, &start, &links, &results, bot, depth, &running.cancel)? {
                Begin::Done(outcome) => {
                    results.insert((*bot).to_owned(), outcome);
                }
                Begin::Send(pending) => {
                    let (hub, start, cancel) = (hub.clone(), start.clone(), running.cancel.clone());
                    tasks.spawn(async move { (pending.bot.clone(), send(&hub, &start, *pending, cancel).await) });
                }
            }
        }
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok((bot, outcome)) => {
                    results.insert(bot, outcome);
                }
                Err(error) => tracing::error!(error = %error, "a routine flow step's task ended abnormally"),
            }
        }
    }
    Ok(())
}

/// Decides one bot's step: its inputs are the replies of the bots linked to it that replied.
#[allow(clippy::too_many_arguments)]
fn begin(
    hub: &Hub,
    start: &Start,
    links: &[FlowLink],
    results: &HashMap<String, Outcome>,
    bot: &str,
    depth: usize,
    cancel: &watch::Receiver<bool>,
) -> Result<Begin> {
    let inputs: Vec<(&FlowLink, &str)> = links
        .iter()
        .filter(|l| l.to_bot_id == bot)
        .filter_map(|l| match results.get(&l.from_bot_id)? {
            Outcome::Replied(text) => Some((l, text.as_str())),
            Outcome::Nothing => None,
        })
        .collect();
    let end = |status, reason: &str| -> Result<Begin> {
        record_ended(hub, start, bot, status, reason)?;
        Ok(Begin::Done(Outcome::Nothing))
    };
    if depth > start.max_depth {
        return end(StepStatus::Skipped, &format!("Skipped: the flow reaches at most {} bots deep", start.max_depth));
    }
    let Some((first, _)) = inputs.first() else {
        return end(StepStatus::Skipped, "Skipped: no bot before this one had anything to hand on");
    };
    if *cancel.borrow() {
        return end(StepStatus::Cancelled, "The flow was cancelled before this step started");
    }
    if let Err(error) = recipient(hub, bot) {
        return end(StepStatus::Failed, &format!("{error:#}"));
    }
    let Some(from) = hub.store.bot(&first.from_bot_id)?.filter(|b| !b.deleted) else {
        return end(StepStatus::Failed, "The bot that handed work on is gone");
    };
    let names: Vec<(String, &str, &str)> =
        inputs.iter().map(|(link, text)| (hub.bot_name(&link.from_bot_id), *text, link.instruction.as_str())).collect();
    let body = handoff_text(&start.routine_name, &names);
    match record(hub, &new_step(start, bot, &body, StepStatus::Running))? {
        Some(step) => {
            let hops = u8::try_from(depth).unwrap_or(u8::MAX);
            Ok(Begin::Send(Box::new(Pending { step, from: from.config, bot: bot.to_owned(), body, hops })))
        }
        // A retry of the run finds the step the first attempt made, and doesn't send it twice.
        None => Ok(Begin::Done(earlier_outcome(hub, &start.run_id, bot)?)),
    }
}

fn earlier_outcome(hub: &Hub, run_id: &str, bot: &str) -> Result<Outcome> {
    let step = hub.store.flow_run_steps(run_id)?.into_iter().find(|s| s.bot_id == bot).context("the step is gone")?;
    Ok(match (step.status, step.output) {
        (StepStatus::Succeeded, Some(text)) => Outcome::Replied(text),
        _ => Outcome::Nothing,
    })
}

async fn send(hub: &Arc<Hub>, start: &Start, pending: Pending, cancel: watch::Receiver<bool>) -> Outcome {
    let step = pending.step.id.clone();
    let request = Handoff {
        routine_name: &start.routine_name,
        from: &pending.from,
        to: &pending.bot,
        body: &pending.body,
        hops: pending.hops,
        timeout: start.timeout,
        cancel,
    };
    let attach = |delivery: &str| {
        if let Err(e) = hub.store.flow_attach_delivery(&step, delivery) {
            tracing::warn!(step, error = format!("{e:#}"), "couldn't record a routine flow step's request");
        }
    };
    match handoff(hub, request, attach).await {
        Ok(text) if crate::chat::group::is_pass(&text) => {
            settle(hub, &step, StepStatus::Succeeded, None, None);
            Outcome::Nothing
        }
        Ok(text) => {
            settle(hub, &step, StepStatus::Succeeded, Some(&text), None);
            Outcome::Replied(text)
        }
        Err(e) if e.is::<FlowCancelled>() => {
            settle(hub, &step, StepStatus::Cancelled, None, Some(&e.to_string()));
            Outcome::Nothing
        }
        Err(e) => {
            settle(hub, &step, StepStatus::Failed, None, Some(&format!("{e:#}")));
            Outcome::Nothing
        }
    }
}

/// What a bot is asked: the replies before it and what the links say to do with them.
fn handoff_text(routine: &str, inputs: &[(String, &str, &str)]) -> String {
    let answers: Vec<String> = inputs
        .iter()
        .map(|(from, answer, _)| {
            format!("{from} answered:\n\n{}", if answer.is_empty() { "(no answer text)" } else { answer })
        })
        .collect();
    let mut instructions: Vec<&str> = inputs.iter().map(|(_, _, i)| i.trim()).filter(|i| !i.is_empty()).collect();
    instructions.dedup();
    let task = if instructions.is_empty() {
        "Do your part of the routine with this.".to_owned()
    } else {
        instructions.join("\n\n")
    };
    format!("The routine \"{routine}\" continues with you.\n\n{}\n\n{task}", answers.join("\n\n---\n\n"))
}

/// The flow to run for a routine run that just ended: none when the routine is gone or the run
/// replied with nothing to pass on.
pub fn start_for(
    routine: Option<&Routine>,
    run: &Run,
    status: Status,
    detail: Option<&str>,
    text: Option<&str>,
) -> Option<Start> {
    let routine = routine?;
    let root = match status {
        Status::Succeeded => Root::Replied(text.filter(|t| !crate::chat::group::is_pass(t))?.to_owned()),
        Status::Failed | Status::Interrupted => Root::Failed(detail.unwrap_or("The routine's run failed").to_owned()),
        Status::Cancelled => Root::Cancelled("The routine's run was cancelled".to_owned()),
        Status::Pending | Status::Starting | Status::Running | Status::Recovering => return None,
    };
    Some(Start {
        run_id: run.id.clone(),
        routine_id: routine.id.clone(),
        routine_name: routine.name.clone(),
        owner: routine.bot_id.clone(),
        run_at: run.created_at,
        timeout: Duration::from_secs(routine.timeout_seconds),
        max_depth: super::max_depth(),
        instruction: routine.instruction.clone(),
        root,
    })
}

/// Runs the flow in the background; a run that ended outside a runtime (a unit test) has none.
pub fn spawn(hub: &Arc<Hub>, start: Start) {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(execute(hub.clone(), start));
    }
}
