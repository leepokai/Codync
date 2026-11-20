//! What the user (the API) and the bot (`steer_task`) do to a task: move it first, let it cut in, replace its
//! brief, stop it, drop it, try it again.

use super::dispatch::{Capacity, Standing};
use super::tasks::{self, Status, Task, find};
use super::{dispatch, ended, sources};
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// What is done to a task: `work_steered`'s `how`.
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Steer {
    First,
    Now,
    /// The bot's alone (`steer_task`).
    Replace,
    Stop,
    /// The user's alone, as is `Retry`.
    Drop,
    Retry,
}

impl Steer {
    /// The steer an API method asks for.
    pub fn of_method(method: &str) -> Option<Self> {
        Some(match method {
            "workFirst" => Self::First,
            "workCutIn" => Self::Now,
            "stopWork" => Self::Stop,
            "dropWork" => Self::Drop,
            "retryWork" => Self::Retry,
            _ => return None,
        })
    }
}

fn waiting<'a>(tasks: &'a mut [Task], id: &str) -> Result<&'a mut Task> {
    let task = find(tasks, id)?;
    if task.status != Status::Waiting {
        bail!("\"{}\" isn't waiting", task.title);
    }
    Ok(task)
}

/// Closes the work of the threads that gave their task up, then looks over the box.
fn settle(hub: &Arc<Hub>, bot: &str, released: Option<String>) {
    if let Some(thread) = released {
        // Its turn's end clears it away; with no turn running, the dispatcher does at once.
        let _ = hub.send_cmd(&thread, Cmd::Stop);
    }
    dispatch(hub, bot);
}

/// A waiting task moves to the front. Order only.
pub fn first(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    tasks::change(hub, bot, |tasks| {
        let front = tasks::front(tasks);
        waiting(tasks, id)?.order = front;
        Ok(())
    })?;
    settle(hub, bot, None);
    Ok(())
}

/// `task` gives its thread up to start again later. What the user wrote to its thread and no turn took yet goes with
/// it, as what they said since (the closed thread's Stop takes those messages back). The thread, to close.
fn release(hub: &Hub, task: &mut Task, front: i64) -> Option<String> {
    let thread = task.thread.clone();
    if let Some(thread) = &thread {
        super::thread::carry(&hub.store, task, thread);
    }
    task.set_aside(front);
    thread
}

/// Sets `gives_way` aside, if any, and puts `id` first. The thread that gave its task up, to close.
fn give_way(hub: &Hub, tasks: &mut [Task], id: &str, gives_way: Option<&str>) -> Result<Option<String>> {
    let mut released = None;
    if let Some(gives_way) = gives_way {
        let front = tasks::front(tasks);
        let task = find(tasks, gives_way)?;
        released = release(hub, task, front);
        task.cut_in = true;
    }
    let front = tasks::front(tasks);
    find(tasks, id)?.order = front;
    Ok(released)
}

/// A waiting task starts now. With every thread busy a working task gives way (`instead_of`, else the one that
/// started last): its thread is closed, and it waits at the front to start again in a new session. With a thread free,
/// or when the task couldn't start anyway (it follows unfinished work, or waits for usage), this is
/// [`first`].
pub fn cut_in(hub: &Arc<Hub>, bot: &str, id: &str, instead_of: Option<&str>) -> Result<()> {
    let cfg = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    let released = tasks::change(hub, bot, |tasks| {
        waiting(tasks, id)?;
        let full = dispatch::capacity(hub, &cfg, tasks, None).free == 0;
        // Only the bot's own work gives way: it starts again later, which a routine run or a request never does.
        let gives_way = if let Some(named) = instead_of {
            let task = find(tasks, named)?;
            if task.status != Status::Working {
                bail!("\"{}\" isn't working: nothing to give way", task.title);
            }
            if !task.source.is_bot() {
                bail!(
                    "\"{}\" is a routine run or another bot's request: it can't give way, stop it instead",
                    task.title
                );
            }
            Some(named.to_owned())
        } else {
            let working = tasks.iter().filter(|i| i.status == Status::Working && i.source.is_bot());
            working.max_by_key(|i| i.started_at).map(|i| i.id.clone())
        };
        let gives_way = gives_way.filter(|_| full);
        // Tried on a copy first: a session is only closed for a task that then starts.
        let mut freed = tasks.clone();
        let closed = give_way(hub, &mut freed, id, gives_way.as_deref())?;
        // The bot's own turn (it may be the one steering) only delays the start, and so does the closed thread's
        // while it stops.
        let capacity = Capacity { chat_free: true, ..dispatch::capacity(hub, &cfg, &freed, closed.as_deref()) };
        let standings = dispatch::standings(&freed, &capacity, |thread| dispatch::thread_state(hub, thread, None));
        let starts = standings.iter().any(|(task, standing)| task == id && *standing == Standing::Start);
        give_way(hub, tasks, id, gives_way.as_deref().filter(|_| starts))
    })?;
    settle(hub, bot, released);
    Ok(())
}

/// A task's brief is replaced. One at work starts again from it, in a new session.
pub fn replace(hub: &Arc<Hub>, bot: &str, id: &str, brief: &str) -> Result<()> {
    let released = tasks::change(hub, bot, |tasks| {
        let front = tasks::front(tasks);
        let task = find(tasks, id)?;
        if task.status.ended() {
            bail!("\"{}\" has ended: add new work instead", task.title);
        }
        if !task.source.is_bot() {
            bail!("\"{}\" is a routine run or another bot's request: its brief isn't yours to replace", task.title);
        }
        brief.clone_into(&mut task.brief);
        Ok(if task.status == Status::Working { release(hub, task, front) } else { None })
    })?;
    settle(hub, bot, released);
    Ok(())
}

/// Stops one of `bot`'s tasks, or every one that is working or waiting (`None`). What a thread already changed stays.
pub fn stop(hub: &Arc<Hub>, bot: &str, id: Option<&str>) -> Result<()> {
    let stopped = tasks::change(hub, bot, |tasks| {
        if let Some(id) = id
            && find(tasks, id)?.status.ended()
        {
            bail!("that work has already ended");
        }
        let now = now_ms();
        let live = tasks.iter_mut().filter(|i| !i.status.ended() && id.is_none_or(|id| i.id == id));
        Ok(live
            .map(|task| {
                let thread = task.thread.clone().filter(|_| task.status == Status::Working);
                task.status = Status::Stopped;
                task.ended_at = Some(now);
                // Under the box's lock, so it comes before any message the user writes to its thread from here,
                // which reopens it: its Stop drops only what waited in the thread until now.
                if let Some(thread) = thread {
                    let _ = hub.send_cmd(&thread, Cmd::Stop);
                }
                task.clone()
            })
            .collect::<Vec<_>>())
    })?;
    for task in stopped {
        // A routine's run is cancelled; the bot that asked hears its request was stopped.
        ended(hub, bot, &task);
    }
    settle(hub, bot, None);
    Ok(())
}

/// Takes a waiting task, or one that ended, out of the box. A waiting routine run is cancelled, and the bot whose
/// request waited hears it was.
pub fn drop_task(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    let dropped = tasks::change(hub, bot, |tasks| {
        let task = find(tasks, id)?.clone();
        if task.status == Status::Working {
            bail!("\"{}\" is being worked on: stop it first", task.title);
        }
        tasks.retain(|i| i.id != id);
        Ok(task)
    })?;
    if dropped.status == Status::Waiting {
        sources::deliver(hub, bot, &dropped);
    }
    // A worktree it had from before it gave way goes with it, if nothing was done there.
    super::worktree::leave(std::slice::from_ref(&dropped));
    settle(hub, bot, None);
    Ok(())
}

/// A stopped or failed task goes back to waiting, last. A routine's run isn't done again as it was: the routine
/// runs again, as a new task.
pub fn retry(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    let task = tasks::load(&hub.store, bot).into_iter().find(|i| i.id == id).ok_or_else(|| anyhow!("no such task"))?;
    if !matches!(task.status, Status::Stopped | Status::Failed) {
        bail!("only work that was stopped or failed can be tried again");
    }
    if !task.source.is_bot() {
        return sources::run_again(hub, bot, &task);
    }
    tasks::change(hub, bot, |tasks| {
        let last = tasks::last(tasks);
        let task = find(tasks, id)?;
        if !matches!(task.status, Status::Stopped | Status::Failed) {
            bail!("only work that was stopped or failed can be tried again");
        }
        task.set_aside(last);
        task.ended_at = None;
        task.result = None;
        task.report = None;
        task.failure = None;
        task.cut_in = false;
        task.restarted = false;
        Ok(())
    })?;
    settle(hub, bot, None);
    Ok(())
}
