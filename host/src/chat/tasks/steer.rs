//! What the user (the API) and the bot (`steer_work`) do to an item: move it first, let it cut in, replace its
//! brief, stop it, drop it, try it again.

use super::items::{self, Item, Status};
use super::{dispatch, finished_properties};
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use std::sync::Arc;

fn find<'a>(items: &'a mut [Item], id: &str) -> Result<&'a mut Item> {
    items.iter_mut().find(|i| i.id == id).ok_or_else(|| anyhow!("no such work"))
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

/// A waiting item starts now. With every place taken a working item gives way (`instead_of`, else the one that
/// started last): its task is closed, and it waits at the front to start again in a new session. With a place free
/// this is [`first`].
pub fn cut_in(hub: &Arc<Hub>, bot: &str, id: &str, instead_of: Option<&str>) -> Result<()> {
    let cfg = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    let released = items::change(hub, bot, |items| {
        waiting(items, id)?;
        let full = dispatch::places(hub, &cfg, items, None).free == 0;
        let gives_way = if let Some(named) = instead_of {
            let item = find(items, named)?;
            if item.status != Status::Working {
                bail!("\"{}\" isn't working: nothing to give way", item.title);
            }
            Some(named.to_owned())
        } else {
            let working = items.iter().filter(|i| i.status == Status::Working);
            working.max_by_key(|i| i.started_at).map(|i| i.id.clone())
        };
        let mut released = None;
        if let Some(gives_way) = gives_way.filter(|_| full) {
            let front = items::front(items);
            let item = find(items, &gives_way)?;
            released.clone_from(&item.task);
            item.set_aside(front);
            item.cut_in = true;
        }
        let front = items::front(items);
        find(items, id)?.order = front;
        Ok(released)
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
        brief.clone_into(&mut item.brief);
        let released = item.task.clone().filter(|_| item.status == Status::Working);
        if released.is_some() {
            item.set_aside(front);
        }
        Ok(released)
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
                (task, item.clone())
            })
            .collect::<Vec<_>>())
    })?;
    for (task, item) in stopped {
        analytics::capture(hub, Event::WorkFinished, finished_properties(&item, None));
        if let Some(task) = task {
            let _ = hub.send_cmd(&task, Cmd::Stop);
        }
    }
    settle(hub, bot, None);
    Ok(())
}

/// Takes a waiting item, or one that ended, out of the box.
pub fn drop_item(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
    items::change(hub, bot, |items| {
        let item = find(items, id)?;
        if item.status == Status::Working {
            bail!("\"{}\" is being worked on: stop it first", item.title);
        }
        items.retain(|i| i.id != id);
        Ok(())
    })?;
    settle(hub, bot, None);
    Ok(())
}

/// A stopped or failed item goes back to waiting, last.
pub fn retry(hub: &Arc<Hub>, bot: &str, id: &str) -> Result<()> {
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
        item.cut_in = false;
        item.restarted = false;
        Ok(())
    })?;
    settle(hub, bot, None);
    Ok(())
}
