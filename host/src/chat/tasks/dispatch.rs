//! The dispatcher: which waiting item a task takes next, which task ends, and why an item still waits.

use super::items::{self, Item, Status};
use super::sources::{self, Begin};
use super::worktree::{self, Place, Worktree};
use super::{Outcome, finished, of, prompts};
use crate::LockExt;
use crate::analytics::errors::host_error;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, TASK_PREFIX, now_ms};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// What a bot's waiting items can count on.
#[derive(Clone, Copy, Debug)]
pub(super) struct Places {
    /// New tasks the bot may start now, under its ceiling.
    pub free: usize,
    /// The bot's tasks at work.
    pub running: usize,
    /// The bot's agent is near its usage limit: one task at a time.
    pub near_limit: bool,
    /// The bot's own turn is over: a new task forks the chat's session, which a running turn would leave with an
    /// unanswered tool call.
    pub coordinator_free: bool,
}

/// A task bot, to an item that follows one of its items.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TaskState {
    /// Its actor doesn't run: it ended, or was deleted.
    Gone,
    /// A turn of its runs.
    Busy,
    Idle,
}

/// Why an item waits (`work[].wait`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Wait {
    /// It starts when a place is free.
    Next,
    /// So many items are ahead of it.
    Behind { ahead: usize },
    /// It follows an item that isn't finished.
    After,
    /// The agent is near its usage limit.
    Usage,
}

/// What the dispatcher does with a waiting item now.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Standing {
    /// The task of the item it follows takes it, in the same session.
    Join(String),
    /// A new task starts for it.
    Start,
    Wait(Wait),
}

enum Follows<'a> {
    /// An ordinary item: it follows nothing, or what it followed is gone, failed or was stopped.
    Nothing,
    /// What it follows isn't finished, or that task is on another item.
    Unfinished,
    Task(&'a str),
}

fn follows<'a>(items: &'a [Item], item: &Item, task: &impl Fn(&str) -> TaskState) -> Follows<'a> {
    let Some(before) = item.after.as_deref().and_then(|id| items.iter().find(|i| i.id == id)) else {
        return Follows::Nothing;
    };
    match (before.status, before.task.as_deref()) {
        (Status::Waiting | Status::Working | Status::NeedsYou, _) => Follows::Unfinished,
        (Status::Done, Some(id)) if items::of_task(items, id).is_some() => Follows::Unfinished,
        (Status::Done, Some(id)) => match task(id) {
            TaskState::Idle => Follows::Task(id),
            TaskState::Busy => Follows::Unfinished,
            TaskState::Gone => Follows::Nothing,
        },
        _ => Follows::Nothing,
    }
}

/// Where each waiting item of `items` stands, lowest `order` first. Items that follow a finished one go to its
/// task first: that task already has its place.
pub(super) fn standings(items: &[Item], places: &Places, task: impl Fn(&str) -> TaskState) -> Vec<(String, Standing)> {
    let mut waiting: Vec<&Item> = items.iter().filter(|i| i.status == Status::Waiting).collect();
    waiting.sort_by_key(|i| i.order);
    let (mut free, mut running) = (places.free, places.running);
    let mut taken: Vec<&str> = Vec::new();
    let mut found: Vec<Option<Standing>> = waiting
        .iter()
        .map(|item| match follows(items, item, &task) {
            Follows::Nothing => None,
            Follows::Task(id) if !taken.contains(&id) => {
                taken.push(id);
                free = free.saturating_sub(1);
                running += 1;
                Some(Standing::Join(id.to_owned()))
            }
            Follows::Task(_) | Follows::Unfinished => Some(Standing::Wait(Wait::After)),
        })
        .collect();
    let mut ahead = 0;
    for standing in found.iter_mut().filter(|s| s.is_none()) {
        *standing = Some(if places.near_limit && running > 0 {
            Standing::Wait(Wait::Usage)
        } else if free > 0 && places.coordinator_free {
            free -= 1;
            running += 1;
            Standing::Start
        } else {
            ahead += 1;
            Standing::Wait(if ahead == 1 { Wait::Next } else { Wait::Behind { ahead: ahead - 1 } })
        });
    }
    waiting.iter().zip(found).filter_map(|(item, standing)| Some((item.id.clone(), standing?))).collect()
}

/// Whether a turn of `id` runs. `over`: the actor whose turn is ending as this is asked (its status still says it
/// works).
fn turn_runs(hub: &Hub, id: &str, over: Option<&str>) -> bool {
    over != Some(id) && matches!(hub.runtime(id).status, BotStatus::Working | BotStatus::NeedsInput)
}

pub(super) fn task_state(hub: &Hub, task: &str, over: Option<&str>) -> TaskState {
    if !hub.has_actor(task) {
        TaskState::Gone
    } else if turn_runs(hub, task, over) {
        TaskState::Busy
    } else {
        TaskState::Idle
    }
}

/// What `cfg`'s waiting items can count on now. `items`: its box.
pub(super) fn places(hub: &Hub, cfg: &BotConfig, items: &[Item], over: Option<&str>) -> Places {
    // A task told to stop (its item stopped, gave way or was replaced) still works until its turn ends.
    let stopping = of(&hub.store, &cfg.id)
        .iter()
        .filter(|task| items::of_task(items, &task.id).is_none() && turn_runs(hub, &task.id, over))
        .count();
    let running = items::working(items) + stopping;
    // A turn the last host process left cut off is resumed by the bot's actor: it runs, though nothing says so yet.
    let coordinator_free = over == Some(cfg.id.as_str())
        || !(turn_runs(hub, &cfg.id, None) || crate::agent::bot::resumable(&hub.store, &cfg.id));
    // The limit of the agent a new task starts on: past a resting one, its fallback's.
    let starts_on = crate::agent::fallback::starting(&hub.store, cfg, now_ms());
    Places {
        free: usize::from(cfg.max_agents).saturating_sub(running),
        running,
        near_limit: crate::usage::near_limit(hub, &starts_on),
        coordinator_free,
    }
}

/// What a pass decided, done once the box is saved.
#[derive(Default)]
struct Plan {
    /// A new task and the item it starts on.
    start: Vec<(BotConfig, Item)>,
    /// A task and the item it takes next.
    join: Vec<(String, Item)>,
    /// Tasks with no item left to work on: their actor stops, their transcript stays.
    end: Vec<String>,
    /// Task bots no item names any more.
    delete: Vec<String>,
    /// An item left the box as it was about to start, so its place may go to the next: another pass.
    again: bool,
    /// Worktrees of items that are over, which nothing works in any more: each goes if it is untouched.
    settle: Vec<(String, Worktree)>,
}

/// Looks over `bot`'s box: waiting items go to tasks where they can, tasks with nothing left end. Run whenever the
/// box or one of its tasks changes.
pub fn dispatch(hub: &Arc<Hub>, bot: &str) {
    run(hub, bot, None);
}

/// [`dispatch`], from the end of `over`'s turn (the bot's own, or one of its tasks').
pub(super) fn run(hub: &Arc<Hub>, bot: &str, over: Option<&str>) {
    let Some(cfg) = hub.store.bot(bot).ok().flatten().filter(|r| !r.deleted).map(|r| r.config) else { return };
    let plan = match items::change(hub, bot, |items| plan(hub, &cfg, items, over)) {
        Ok(plan) => plan,
        Err(error) => {
            host_error!(hub, "couldn't look over a bot's work", bot, error = format!("{error:#}"));
            // The hourly look tries again.
            super::sweep::arm(hub, bot);
            return;
        }
    };
    for (task, item) in plan.start {
        begin(hub, bot, &task, &item, true);
    }
    for (task, item) in plan.join {
        if let Ok(Some(row)) = hub.store.bot(&task) {
            begin(hub, bot, &row.config, &item, false);
        } else {
            host_error!(hub, "a task that took more work is gone", task);
        }
    }
    for task in plan.end {
        // Looked at again under the box's lock: a message the user wrote to its thread since reopened its item
        // (`thread::message`, which holds the lock from there to the actor).
        let _g = hub.tasks.boxes.locked();
        if items::of_task(&items::load(&hub.store, bot), &task).is_none() {
            hub.stop_actor(&task);
        }
    }
    for task in plan.delete {
        if let Err(error) = hub.delete_bot(&task) {
            tracing::warn!(task, error = format!("{error:#}"), "couldn't remove a task");
        }
    }
    for (item, left) in plan.settle {
        let bot = bot.to_owned();
        worktree::settle_then(hub, &bot.clone(), &item, left, move |hub| dispatch(hub, &bot));
    }
    if plan.again {
        return run(hub, bot, over);
    }
    hub.emit_bot(bot);
    // What follows the bot from outside its chat counts it working while its box holds work.
    hub.outside_changed(bot);
    super::sweep::arm(hub, bot);
}

fn plan(hub: &Hub, cfg: &BotConfig, items: &mut Vec<Item>, over: Option<&str>) -> Result<Plan> {
    let mut plan = Plan::default();
    // A run whose routine was paused or deleted while it waited, a request the last host process took with it.
    items.retain(|i| i.status != Status::Waiting || !sources::withdrawn(hub, i));
    let places = places(hub, cfg, items, over);
    for (id, standing) in standings(items, &places, |task| task_state(hub, task, over)) {
        if matches!(standing, Standing::Wait(_)) {
            continue;
        }
        // What it follows began its task's session.
        let before = items.iter().find(|i| i.id == id).and_then(|i| i.after.clone());
        let before = before.and_then(|id| items.iter().position(|i| i.id == id));
        let began = before.and_then(|at| items[at].start);
        // Another task of the bot works in its folder now: a new one beside it gets a worktree, in a repository.
        let side_by_side = items::working(items) > 0;
        let Some(at) = items.iter().position(|i| i.id == id) else { continue };
        // Tried again while the worktree it had is looked over: it starts once that is done, which looks again.
        if items[at].worktree.as_ref().is_some_and(|w| w.left == worktree::Left::Settling) {
            continue;
        }
        match sources::begin(hub, &cfg.id, &mut items[at])? {
            Begin::Go => {}
            Begin::Later => continue,
            Begin::Gone => {
                items.remove(at);
                plan.again = true;
                continue;
            }
        }
        // It goes on where the task it joins works: a worktree that task is in is this item's from here.
        let inherited = before
            .filter(|_| matches!(standing, Standing::Join(_)))
            .and_then(|before| items[before].worktree.take_if(|w| w.left == worktree::Left::InUse));
        let item = &mut items[at];
        item.status = Status::Working;
        item.started_at = Some(now_ms());
        if let Standing::Join(task) = standing {
            item.start = began;
            item.task = Some(task.clone());
            item.worktree = inherited.or(item.worktree.take());
            plan.join.push((task, item.clone()));
        } else {
            let task = task_bot(cfg);
            item.start = None;
            item.fork_failed = false;
            // One it has from before (it gave way, or is tried again) is where its work is.
            item.worktree.take_if(|w| !worktree::usable(w));
            // A bot's own folder keeps all its work in one place, with its versions (`agent::versions`).
            let beside = cfg.task_worktrees
                && !crate::agent::workspace::is_managed(cfg)
                && item.source.is_bot()
                && side_by_side
                && worktree::in_repo(&cfg.cwd);
            item.place = if item.worktree.is_some() || beside { Place::Worktree } else { Place::Folder };
            // A routine's run starts as it did in the chat: a new session that knows only its own prompt.
            // ponytail: a task in a worktree starts from a brief, not a fork: a fork is taken in another folder
            // than its session's, which no agent is checked to honor. Fork there once one is (stage 10's spikes).
            let forks = sources::from_chat(&item.source) && item.place == Place::Folder;
            item.fork_from = forks.then(|| fork_source(hub, &cfg.id)).flatten();
            item.task = Some(task.id.clone());
            {
                let _g = hub.emit_lock.locked();
                hub.store.save_bot(&task)?;
            }
            plan.start.push((task, item.clone()));
        }
    }
    plan.settle = worktree::over(items, |task| turn_runs(hub, task, over));
    for task in of(&hub.store, &cfg.id) {
        if items::of_task(items, &task.id).is_some() || turn_runs(hub, &task.id, over) {
            continue;
        }
        if !items.iter().any(|i| i.task.as_deref() == Some(&task.id)) {
            plan.delete.push(task.id);
        } else if hub.has_actor(&task.id) {
            plan.end.push(task.id);
        }
    }
    Ok(plan)
}

/// Hands `item` of `bot` to `task` as its next turn. `first`: the task is new, and starts here. A routine run's
/// prompt and another bot's request go as they are: they say themselves what the turn is and where its reply goes.
/// Under the box's lock, and only while the item is still on that task: a Stop since the plan found no turn to stop.
fn begin(hub: &Arc<Hub>, bot: &str, task: &BotConfig, item: &Item, first: bool) {
    let posted = {
        let _g = hub.tasks.boxes.locked();
        if items::of_task(&items::load(&hub.store, bot), &task.id).is_none_or(|on| on.id != item.id) {
            return;
        }
        if first {
            hub.spawn_bot(task.clone());
            sources::watch(hub, bot, &task.id, item);
        }
        let text = if item.source.is_bot() { prompts::item_prompt(item, first) } else { item.brief.clone() };
        crate::chat::messages::post(hub, task, json!({"text": text, "from": "host"}))
    };
    if let Err(error) = posted {
        host_error!(hub, "couldn't hand work to a task", task = %task.id, error = format!("{error:#}"));
        finished(hub, task, Outcome::not_dispatched(&error));
    }
}

/// A task bot for `cfg`: its config, without what stays with the chat.
fn task_bot(cfg: &BotConfig) -> BotConfig {
    BotConfig {
        id: format!("{TASK_PREFIX}{}", uuid::Uuid::new_v4()),
        task_of: Some(cfg.id.clone()),
        created_at: now_ms(),
        // Its alerts are its bot's: the cards it raises in the chat, and the turn its report starts there.
        notify: Some(false),
        lead: false,
        pinned: false,
        position: 0,
        section: None,
        auto_name: false,
        created_by: None,
        offered_to_network: false,
        reachable_by_assistants: false,
        // One screen and one team: those stay with the chat.
        computer: false,
        manage_team: false,
        // As its bot's, wherever the task works (a worktree isn't the bot's own folder).
        shared_memory: Some(crate::chat::memory::shared(cfg)),
        ..cfg.clone()
    }
}

/// `bot`'s main session, when a task may start as a fork of it: the bot is on its own agent, and the session has
/// used under half its window (a fork carries all of it). Whether the agent can fork is its actor's to find out.
fn fork_source(hub: &Hub, bot: &str) -> Option<String> {
    let session = hub.store.bot(bot).ok().flatten()?.session_id.filter(|s| !s.is_empty())?;
    if hub.runtime(bot).agent_step.is_some() {
        return None;
    }
    let chat = crate::agent::bot::conversation(&hub.store, bot, &session);
    (chat.size == 0 || chat.used * 2 < chat.size).then_some(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};

    fn item(id: &str, status: Status, order: i64) -> Item {
        serde_json::from_value(json!({
            "id": id, "title": id, "brief": "x", "status": status, "order": order, "createdAt": 0,
        }))
        .expect("an item")
    }

    fn after(id: &str, order: i64, before: &str) -> Item {
        Item { after: Some(before.into()), ..item(id, Status::Waiting, order) }
    }

    fn on(task: &str, item: Item) -> Item {
        Item { task: Some(task.into()), ..item }
    }

    fn places(free: usize, running: usize) -> Places {
        Places { free, running, near_limit: false, coordinator_free: true }
    }

    fn stand(items: &[Item], places: Places, task: TaskState) -> Vec<(String, Standing)> {
        standings(items, &places, |_| task)
    }

    fn of(standings: &[(String, Standing)]) -> Vec<(&str, &Standing)> {
        standings.iter().map(|(id, s)| (id.as_str(), s)).collect()
    }

    #[test]
    fn waiting_items_start_in_order_up_to_the_free_places_and_the_rest_say_how_far_back_they_are() {
        let items = [
            item("c", Status::Waiting, 3),
            item("a", Status::Waiting, -1),
            on("t1", item("w", Status::Working, 0)),
            item("b", Status::Waiting, 2),
            item("d", Status::Waiting, 4),
            item("old", Status::Done, 1),
        ];
        let standings = stand(&items, places(1, 1), TaskState::Gone);
        assert_eq!(
            of(&standings),
            [
                ("a", &Standing::Start),
                ("b", &Standing::Wait(Wait::Next)),
                ("c", &Standing::Wait(Wait::Behind { ahead: 1 })),
                ("d", &Standing::Wait(Wait::Behind { ahead: 2 })),
            ]
        );
        let none = stand(&items, places(0, 1), TaskState::Gone);
        assert_eq!(none[0], ("a".into(), Standing::Wait(Wait::Next)), "nothing is refused for being over the ceiling");
    }

    #[test]
    fn a_new_task_waits_for_the_bots_own_turn_to_end() {
        let items = [item("a", Status::Waiting, 1)];
        let busy = Places { coordinator_free: false, ..places(3, 0) };
        assert_eq!(stand(&items, busy, TaskState::Gone)[0].1, Standing::Wait(Wait::Next));
    }

    #[test]
    fn near_the_usage_limit_one_task_runs_at_a_time() {
        let items = [item("a", Status::Waiting, 1), item("b", Status::Waiting, 2)];
        let near = Places { near_limit: true, ..places(3, 0) };
        let standings = stand(&items, near, TaskState::Gone);
        assert_eq!(of(&standings), [("a", &Standing::Start), ("b", &Standing::Wait(Wait::Usage))]);
    }

    #[test]
    fn an_item_follows_another_into_its_task_once_that_task_is_idle() {
        let working = [on("t1", item("a", Status::Working, 0)), after("b", 1, "a")];
        assert_eq!(stand(&working, places(2, 1), TaskState::Busy)[0].1, Standing::Wait(Wait::After));

        let done = [on("t1", item("a", Status::Done, 0)), after("b", 1, "a"), after("c", 2, "a")];
        let standings = stand(&done, places(3, 0), TaskState::Idle);
        assert_eq!(
            of(&standings),
            [("b", &Standing::Join("t1".into())), ("c", &Standing::Wait(Wait::After))],
            "one item at a time in a session"
        );
        assert_eq!(stand(&done, places(3, 0), TaskState::Busy)[0].1, Standing::Wait(Wait::After));

        // The task that would take it is gone, or what it followed failed or was stopped: an ordinary item.
        assert_eq!(stand(&done, places(3, 0), TaskState::Gone)[0].1, Standing::Start);
        let stopped = [on("t1", item("a", Status::Stopped, 0)), after("b", 1, "a")];
        assert_eq!(stand(&stopped, places(3, 0), TaskState::Idle)[0].1, Standing::Start);
        let dropped = [after("b", 1, "a")];
        assert_eq!(stand(&dropped, places(3, 0), TaskState::Idle)[0].1, Standing::Start);
        // What it follows hasn't started yet.
        let waiting = [item("a", Status::Waiting, 5), after("b", 1, "a")];
        let standings = stand(&waiting, places(1, 0), TaskState::Gone);
        assert_eq!(of(&standings), [("b", &Standing::Wait(Wait::After)), ("a", &Standing::Start)]);
    }

    #[test]
    fn an_idle_task_keeps_its_place_for_the_item_that_follows() {
        // One place: the task of `a` is idle and `c` follows `a`, so the ordinary `b` ahead of it still waits.
        let items = [on("t1", item("a", Status::Done, 0)), item("b", Status::Waiting, 1), after("c", 2, "a")];
        let standings = stand(&items, places(1, 0), TaskState::Idle);
        assert_eq!(of(&standings), [("b", &Standing::Wait(Wait::Next)), ("c", &Standing::Join("t1".into()))]);
    }

    #[test]
    fn work_stopped_before_its_task_started_never_reaches_it() {
        let hub = hub();
        let cfg = bot("b1", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        crate::chat::tasks::put_working_for_test(&hub.store, "b1", "task-1");
        let planned = items::load(&hub.store, "b1").remove(0);
        // A Stop between the plan and the start: no actor yet to tell.
        items::change(&hub, "b1", |items| {
            items[0].status = Status::Stopped;
            Ok(())
        })
        .expect("stopped");
        let task = BotConfig { id: "task-1".into(), task_of: Some("b1".into()), ..cfg };
        begin(&hub, "b1", &task, &planned, true);
        assert!(!hub.has_actor("task-1"), "it never starts");
    }

    #[test]
    fn work_tried_again_waits_while_its_worktree_is_looked_over() {
        let hub = hub();
        hub.store.save_bot(&BotConfig { max_agents: 2, ..bot("b1", "Mika", "/tmp") }).expect("saved");
        let worktree = json!({"branch": "codync/t", "path": "/w", "cwd": "/w", "base": "abc", "left": "settling"});
        let waiting = json!({"id": "i1", "title": "Work", "brief": "x", "status": "waiting", "order": 1,
            "createdAt": 0, "worktree": worktree});
        hub.store.kv_set(&format!("{}b1", items::PREFIX), &json!([waiting]).to_string()).expect("saved");
        run(&hub, "b1", None);
        assert_eq!(items::load(&hub.store, "b1")[0].status, Status::Waiting);
    }

    #[test]
    fn a_task_still_stopping_keeps_its_place() {
        let hub = hub();
        let cfg = BotConfig { max_agents: 1, ..bot("b1", "Mika", "/tmp") };
        hub.store.save_bot(&cfg).expect("saved");
        let task = BotConfig { id: "task-1".into(), task_of: Some("b1".into()), ..cfg.clone() };
        hub.store.save_bot(&task).expect("saved");
        hub.set_runtime("task-1", |rt| rt.status = BotStatus::Working);
        let replaced = [on("task-1", item("a", Status::Stopped, 0)), item("b", Status::Waiting, 1)];
        let p = super::places(&hub, &cfg, &replaced, None);
        assert_eq!((p.free, p.running), (0, 1), "the replacement waits for it");
        assert_eq!(super::places(&hub, &cfg, &replaced, Some("task-1")).free, 1, "its turn is over");
    }

    #[test]
    fn why_an_item_waits_reads_as_the_wire_says() {
        assert_eq!(json!(Wait::Next), json!({"kind": "next"}));
        assert_eq!(json!(Wait::Behind { ahead: 2 }), json!({"kind": "behind", "ahead": 2}));
        assert_eq!(json!(Wait::After), json!({"kind": "after"}));
        assert_eq!(json!(Wait::Usage), json!({"kind": "usage"}));
    }
}
