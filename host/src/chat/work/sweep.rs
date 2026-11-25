//! The hourly look at a box that still holds work, for what no event said: a dispatcher pass that couldn't save,
//! a thread whose end never arrived. It runs in the host alone and wakes no agent unless something is lost.

use super::dispatch::{self, ThreadState};
use super::worktree;
use super::{Failure, Outcome, Status, close, end_claimed, tasks};
use crate::LockExt;
use crate::analytics::errors::host_error;
use crate::hub::Hub;
use std::sync::Arc;
use std::time::Duration;

/// How long after the last look at a box the next one comes, while the box holds work.
const EVERY: Duration = Duration::from_hours(1);
/// Why a task whose thread is gone, and that already started again once, didn't finish.
pub const LOST: &str = "its agent stopped without finishing";

/// Sets `bot`'s next look, or none once its box holds nothing waiting or working and keeps no worktree the user may
/// still merge. Called after every dispatcher pass, so the hour counts from the last time anything happened.
pub(super) fn arm(hub: &Arc<Hub>, bot: &str) {
    let mut sweeps = hub.work.sweeps.locked();
    if let Some(old) = sweeps.remove(bot) {
        old.abort();
    }
    let tasks = tasks::load(&hub.store, bot);
    if tasks.iter().all(|i| i.status.ended()) && !worktree::any_kept(&tasks) {
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

/// Looks `bot`'s box over: a working task whose thread runs no turn waits again once, then fails (as after a host
/// restart), a kept worktree goes once the user merged it, and the dispatcher takes a pass, which sets the next look
/// if work is left.
pub(super) fn sweep(hub: &Arc<Hub>, bot: &str) {
    if hub.store.bot(bot).ok().flatten().is_none_or(|row| row.deleted) {
        return;
    }
    let lost = tasks::change(hub, bot, |tasks| {
        let kept = worktree::kept(tasks);
        let threads: Vec<String> = tasks
            .iter()
            .filter(|i| i.status == Status::Working)
            .filter_map(|i| i.thread.clone())
            .filter(|thread| is_lost(hub, thread))
            .collect();
        let failed = threads
            .iter()
            .filter_map(|thread| {
                tracing::warn!(bot, thread, "a thread stopped without ending its task");
                end_claimed(tasks, thread, Outcome::Failed(Failure::Lost, LOST.into()))
            })
            .collect::<Vec<_>>();
        Ok((failed, kept))
    });
    match lost {
        Ok((failed, kept)) => {
            for over in failed {
                close(hub, bot, over, |_| {});
            }
            for (task, worktree) in kept {
                let owner = bot.to_owned();
                worktree::settle_then(hub, bot, &task, worktree, move |hub| dispatch::dispatch(hub, &owner));
            }
        }
        Err(error) => host_error!(hub, "couldn't look a bot's work over", bot, error = format!("{error:#}")),
    }
    dispatch::dispatch(hub, bot);
}

/// Whether nothing of `thread` runs: no turn of its (however long and quiet a running one is), none its actor is
/// about to resume, and none about to start for what the user wrote to its thread (a task they just reopened).
fn is_lost(hub: &Hub, thread: &str) -> bool {
    !matches!(dispatch::thread_state(hub, thread, None), ThreadState::Busy)
        && !crate::agent::bot::resumable(&hub.store, thread)
        && !super::thread::unheard(&hub.store, thread)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use serde_json::json;

    #[test]
    fn a_working_task_whose_thread_is_gone_waits_again_once_then_fails() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        // The bot's own turn runs, so the dispatcher starts nothing here: only the look itself is under test.
        hub.set_runtime("b1", |rt| rt.status = crate::hub::BotStatus::Working);
        crate::chat::work::put_working_for_test(&hub.store, "b1", "thread-1");

        sweep(&hub, "b1");
        let task = tasks::load(&hub.store, "b1").remove(0);
        assert_eq!((task.status, task.thread, task.restarted), (Status::Waiting, None, true));

        let again = json!({"id": "i1", "title": "Work", "brief": "x", "status": "working", "order": 1,
            "createdAt": 0, "thread": "thread-2", "restarted": true});
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &json!([again]).to_string()).expect("saved");
        sweep(&hub, "b1");
        let task = tasks::load(&hub.store, "b1").remove(0);
        assert_eq!((task.status, task.result.as_deref()), (Status::Failed, Some(LOST)));
        // Nothing left to look at: no next look is set.
        arm(&hub, "b1");
        assert!(hub.work.sweeps.locked().is_empty());
    }

    #[tokio::test]
    async fn a_box_that_keeps_a_worktree_is_looked_at_again() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let worktree = json!({"branch": "codync/t", "path": "/w", "cwd": "/w", "base": "abc", "left": "kept"});
        let ended = json!({"id": "i1", "title": "Work", "brief": "x", "status": "done", "order": 1, "createdAt": 0,
            "worktree": worktree});
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &json!([ended]).to_string()).expect("saved");
        arm(&hub, "b1");
        assert!(hub.work.sweeps.locked().contains_key("b1"), "the user may still merge it");
    }

    #[test]
    fn a_task_the_user_just_wrote_to_is_not_lost_before_its_turn_begins() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        hub.set_runtime("b1", |rt| rt.status = crate::hub::BotStatus::Working);
        crate::chat::work::put_working_for_test(&hub.store, "b1", "thread-1");
        let thread = crate::store::Lane::main("thread-1");
        let queued = json!({"text": "one more thing", "status": "queued"});
        hub.store.insert_entry(&thread, crate::store::EntryKind::User, 1, &queued).expect("a message");

        sweep(&hub, "b1");
        let task = tasks::load(&hub.store, "b1").remove(0);
        assert_eq!((task.status, task.restarted), (Status::Working, false), "its turn is about to start");
    }
}
