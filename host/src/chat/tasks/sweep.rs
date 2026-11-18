//! The hourly look at a box that still holds work, for what no event said: a dispatcher pass that couldn't save,
//! a task whose end never arrived. It runs in the host alone and wakes no agent unless something is lost.

use super::dispatch::{self, TaskState};
use super::{Failure, Outcome, Status, close, end_claimed, items};
use crate::LockExt;
use crate::hub::Hub;
use std::sync::Arc;
use std::time::Duration;

/// How long after the last look at a box the next one comes, while the box holds work.
const EVERY: Duration = Duration::from_hours(1);
/// Why an item whose task is gone, and that already started again once, didn't finish.
pub const LOST: &str = "its agent stopped without finishing";

/// Sets `bot`'s next look, or none once its box holds nothing waiting or working. Called after every dispatcher
/// pass, so the hour counts from the last time anything happened.
pub(super) fn arm(hub: &Arc<Hub>, bot: &str) {
    let mut sweeps = hub.tasks.sweeps.locked();
    if let Some(old) = sweeps.remove(bot) {
        old.abort();
    }
    if items::load(&hub.store, bot).iter().all(|i| i.status.ended()) {
        return;
    }
    // Unit tests drive the dispatcher without a runtime: they have no later.
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let (hub, bot) = (Arc::downgrade(hub), bot.to_owned());
    let look = runtime.spawn({
        let bot = bot.clone();
        async move {
            tokio::time::sleep(EVERY).await;
            if let Some(hub) = hub.upgrade() {
                sweep(&hub, &bot);
            }
        }
    });
    sweeps.insert(bot, look);
}

/// Looks `bot`'s box over: a working item whose task runs no turn waits again once, then fails (as after a host
/// restart), and the dispatcher takes a pass, which sets the next look if work is left.
pub(super) fn sweep(hub: &Arc<Hub>, bot: &str) {
    if hub.store.bot(bot).ok().flatten().is_none_or(|row| row.deleted) {
        return;
    }
    let lost = items::change(hub, bot, |items| {
        let tasks: Vec<String> = items
            .iter()
            .filter(|i| i.status == Status::Working)
            .filter_map(|i| i.task.clone())
            .filter(|task| is_lost(hub, task))
            .collect();
        Ok(tasks
            .iter()
            .filter_map(|task| {
                tracing::warn!(bot, task, "a task stopped without ending its item");
                end_claimed(items, task, Outcome::Failed(Failure::Lost, LOST.into()))
            })
            .collect::<Vec<_>>())
    });
    match lost {
        Ok(failed) => failed.into_iter().for_each(|over| close(hub, bot, over, |_| {})),
        Err(error) => tracing::error!(bot, error = format!("{error:#}"), "couldn't look a bot's work over"),
    }
    dispatch::dispatch(hub, bot);
}

/// Whether nothing of `task` runs: no turn of its (however long and quiet a running one is), none its actor is
/// about to resume, and none about to start for what the user wrote to its thread (an item they just reopened).
fn is_lost(hub: &Hub, task: &str) -> bool {
    !matches!(dispatch::task_state(hub, task, None), TaskState::Busy)
        && !crate::agent::bot::resumable(&hub.store, task)
        && !super::thread::unheard(&hub.store, task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use serde_json::json;

    #[test]
    fn a_working_item_whose_task_is_gone_waits_again_once_then_fails() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        // The bot's own turn runs, so the dispatcher starts nothing here: only the look itself is under test.
        hub.set_runtime("b1", |rt| rt.status = crate::hub::BotStatus::Working);
        crate::chat::tasks::put_working_for_test(&hub.store, "b1", "task-1");

        sweep(&hub, "b1");
        let item = items::load(&hub.store, "b1").remove(0);
        assert_eq!((item.status, item.task, item.restarted), (Status::Waiting, None, true));

        let again = json!({"id": "i1", "title": "Work", "brief": "x", "status": "working", "order": 1,
            "createdAt": 0, "task": "task-2", "restarted": true});
        hub.store.kv_set(&format!("{}b1", items::PREFIX), &json!([again]).to_string()).expect("saved");
        sweep(&hub, "b1");
        let item = items::load(&hub.store, "b1").remove(0);
        assert_eq!((item.status, item.result.as_deref()), (Status::Failed, Some(LOST)));
        // Nothing left to look at: no next look is set.
        arm(&hub, "b1");
        assert!(hub.tasks.sweeps.locked().is_empty());
    }

    #[test]
    fn an_item_the_user_just_wrote_to_is_not_lost_before_its_turn_begins() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        hub.set_runtime("b1", |rt| rt.status = crate::hub::BotStatus::Working);
        crate::chat::tasks::put_working_for_test(&hub.store, "b1", "task-1");
        let thread = crate::store::Lane::main("task-1");
        let queued = json!({"text": "one more thing", "status": "queued"});
        hub.store.insert_entry(&thread, crate::store::EntryKind::User, 1, &queued).expect("a message");

        sweep(&hub, "b1");
        let item = items::load(&hub.store, "b1").remove(0);
        assert_eq!((item.status, item.restarted), (Status::Working, false), "its turn is about to start");
    }
}
