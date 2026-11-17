//! What the user (the API) and the bot (`steer_work`) do to an item: move it first, let it cut in, replace its
//! brief, stop it, drop it, try it again.

use super::dispatch::{Places, Standing};
use super::items::{self, Item, Status, find};
use super::{dispatch, ended, sources};
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// What is done to an item: `work_steered`'s `how`.
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Steer {
    First,
    Now,
    /// The bot's alone (`steer_work`).
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

fn waiting<'a>(items: &'a mut [Item], id: &str) -> Result<&'a mut Item> {
    let item = find(items, id)?;
    if item.status != Status::Waiting {
        bail!("\"{}\" isn't waiting", item.title);
    }
    Ok(item)
}

/// Closes the work of the tasks that gave their item up, then looks over the box.
fn settle(hub: &Arc<Hub>, bot: &str, released: Option<String>) {
    if let Some(task) = released {
        // Its turn's end clears it away; with no turn running, the dispatcher does at once.
        let _ = hub.send_cmd(&task, Cmd::Stop);
    }
    dispatch(hub, bot);
}

/// A waiting item moves to the front. Order only.
pub fn first(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    items::change(hub, bot, |items| {
        let front = items::front(items);
        waiting(items, id)?.order = front;
        Ok(())
    })?;
    settle(hub, bot, None);
    Ok(())
}

/// `item` gives its task up to start again later. What the user wrote to its thread and no turn took yet goes with
/// it, as what they said since (the closed task's Stop takes those messages back). The task, to close.
fn release(hub: &Hub, item: &mut Item, front: i64) -> Option<String> {
    let task = item.task.clone();
    if let Some(task) = &task {
        super::thread::carry(&hub.store, item, task);
    }
    item.set_aside(front);
    task
}

/// Sets `gives_way` aside, if any, and puts `id` first. The task that gave its item up, to close.
fn give_way(hub: &Hub, items: &mut [Item], id: &str, gives_way: Option<&str>) -> Result<Option<String>> {
    let mut released = None;
    if let Some(gives_way) = gives_way {
        let front = items::front(items);
        let item = find(items, gives_way)?;
        released = release(hub, item, front);
        item.cut_in = true;
    }
    let front = items::front(items);
    find(items, id)?.order = front;
    Ok(released)
}

/// A waiting item starts now. With every place taken a working item gives way (`instead_of`, else the one that
/// started last): its task is closed, and it waits at the front to start again in a new session. With a place free,
/// or when the item couldn't take the place anyway (it follows unfinished work, or waits for usage), this is
/// [`first`].
pub fn cut_in(hub: &Arc<Hub>, bot: &str, id: &str, instead_of: Option<&str>) -> Result<()> {
    let cfg = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    let released = items::change(hub, bot, |items| {
        waiting(items, id)?;
        let full = dispatch::places(hub, &cfg, items, None).free == 0;
        // Only the bot's own work gives way: it starts again later, which a routine run or a request never does.
        let gives_way = if let Some(named) = instead_of {
            let item = find(items, named)?;
            if item.status != Status::Working {
                bail!("\"{}\" isn't working: nothing to give way", item.title);
            }
            if !item.source.is_bot() {
                bail!(
                    "\"{}\" is a routine run or another bot's request: it can't give way, stop it instead",
                    item.title
                );
            }
            Some(named.to_owned())
        } else {
            let working = items.iter().filter(|i| i.status == Status::Working && i.source.is_bot());
            working.max_by_key(|i| i.started_at).map(|i| i.id.clone())
        };
        let gives_way = gives_way.filter(|_| full);
        // Tried on a copy first: a session is only closed for an item that then starts.
        let mut freed = items.clone();
        let closed = give_way(hub, &mut freed, id, gives_way.as_deref())?;
        // The bot's own turn (it may be the one steering) only delays the start, and so does the closed task's
        // while it stops.
        let places = Places { coordinator_free: true, ..dispatch::places(hub, &cfg, &freed, closed.as_deref()) };
        let standings = dispatch::standings(&freed, &places, |task| dispatch::task_state(hub, task, None));
        let starts = standings.iter().any(|(item, standing)| item == id && *standing == Standing::Start);
        give_way(hub, items, id, gives_way.as_deref().filter(|_| starts))
    })?;
    settle(hub, bot, released);
    Ok(())
}

/// An item's brief is replaced. One at work starts again from it, in a new session.
pub fn replace(hub: &Arc<Hub>, bot: &str, id: &str, brief: &str) -> Result<()> {
    let released = items::change(hub, bot, |items| {
        let front = items::front(items);
        let item = find(items, id)?;
        if item.status.ended() {
            bail!("\"{}\" has ended: add new work instead", item.title);
        }
        if !item.source.is_bot() {
            bail!("\"{}\" is a routine run or another bot's request: its brief isn't yours to replace", item.title);
        }
        brief.clone_into(&mut item.brief);
        Ok(if item.status == Status::Working { release(hub, item, front) } else { None })
    })?;
    settle(hub, bot, released);
    Ok(())
}

/// Stops one of `bot`'s items, or every one that is working or waiting (`None`). What a task already changed stays.
pub fn stop(hub: &Arc<Hub>, bot: &str, id: Option<&str>) -> Result<()> {
    let stopped = items::change(hub, bot, |items| {
        if let Some(id) = id
            && find(items, id)?.status.ended()
        {
            bail!("that work has already ended");
        }
        let now = now_ms();
        let live = items.iter_mut().filter(|i| !i.status.ended() && id.is_none_or(|id| i.id == id));
        Ok(live
            .map(|item| {
                let task = item.task.clone().filter(|_| item.status == Status::Working);
                item.status = Status::Stopped;
                item.ended_at = Some(now);
                // Under the box's lock, so it comes before any message the user writes to its thread from here,
                // which reopens it: its Stop drops only what waited in the thread until now.
                if let Some(task) = task {
                    let _ = hub.send_cmd(&task, Cmd::Stop);
                }
                item.clone()
            })
            .collect::<Vec<_>>())
    })?;
    for item in stopped {
        // A routine's run is cancelled; the bot that asked hears its request was stopped.
        ended(hub, bot, &item);
    }
    settle(hub, bot, None);
    Ok(())
}

/// Takes a waiting item, or one that ended, out of the box. A waiting routine run is cancelled, and the bot whose
/// request waited hears it was.
pub fn drop_item(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    let dropped = items::change(hub, bot, |items| {
        let item = find(items, id)?.clone();
        if item.status == Status::Working {
            bail!("\"{}\" is being worked on: stop it first", item.title);
        }
        items.retain(|i| i.id != id);
        Ok(item)
    })?;
    if dropped.status == Status::Waiting {
        sources::deliver(hub, bot, &dropped);
    }
    // A worktree it had from before it gave way goes with it, if nothing was done there.
    super::worktree::leave(std::slice::from_ref(&dropped));
    settle(hub, bot, None);
    Ok(())
}

/// A stopped or failed item goes back to waiting, last. A routine's run isn't done again as it was: the routine
/// runs again, as a new item.
pub fn retry(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    let item = items::load(&hub.store, bot).into_iter().find(|i| i.id == id).ok_or_else(|| anyhow!("no such work"))?;
    if !matches!(item.status, Status::Stopped | Status::Failed) {
        bail!("only work that was stopped or failed can be tried again");
    }
    if !item.source.is_bot() {
        return sources::run_again(hub, bot, &item);
    }
    items::change(hub, bot, |items| {
        let last = items::last(items);
        let item = find(items, id)?;
        if !matches!(item.status, Status::Stopped | Status::Failed) {
            bail!("only work that was stopped or failed can be tried again");
        }
        item.set_aside(last);
        item.ended_at = None;
        item.result = None;
        item.report = None;
        item.failure = None;
        item.cut_in = false;
        item.restarted = false;
        Ok(())
    })?;
    settle(hub, bot, None);
    Ok(())
}
