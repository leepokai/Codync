//! The dispatcher: which waiting item a task takes next, which task ends, and why an item still waits.

use super::items::{self, Item, Status};
use super::{Failure, Outcome, finished, of, prompts};
use crate::LockExt;
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
    let running = items::working(items);
    // A turn the last host process left cut off is resumed by the bot's actor: it runs, though nothing says so yet.
    let coordinator_free = over == Some(cfg.id.as_str())
        || !(turn_runs(hub, &cfg.id, None) || crate::agent::bot::resumable(&hub.store, &cfg.id));
    Places {
        free: usize::from(cfg.max_agents).saturating_sub(running),
        running,
        near_limit: crate::usage::near_limit(hub, cfg),
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
            tracing::error!(bot, error = format!("{error:#}"), "couldn't look over a bot's work");
            // The hourly look tries again.
            super::sweep::arm(hub, bot);
            return;
        }
    };
    for (task, item) in plan.start {
        hub.spawn_bot(task.clone());
        begin(hub, &task, &item, true);
    }
    for (task, item) in plan.join {
        if let Ok(Some(row)) = hub.store.bot(&task) {
            begin(hub, &row.config, &item, false);
        } else {
            tracing::error!(task, "a task that took more work is gone");
        }
    }
    for task in plan.end {
        hub.stop_actor(&task);
    }
    for task in plan.delete {
        if let Err(error) = hub.delete_bot(&task) {
            tracing::warn!(task, error = format!("{error:#}"), "couldn't remove a task");
        }
    }
    hub.emit_bot(bot);
    super::sweep::arm(hub, bot);
}

fn plan(hub: &Hub, cfg: &BotConfig, items: &mut [Item], over: Option<&str>) -> Result<Plan> {
    let mut plan = Plan::default();
    let places = places(hub, cfg, items, over);
    for (id, standing) in standings(items, &places, |task| task_state(hub, task, over)) {
        if matches!(standing, Standing::Wait(_)) {
            continue;
        }
        // What it follows began its task's session.
        let before = items.iter().find(|i| i.id == id).and_then(|i| i.after.clone());
        let began = before.and_then(|id| items.iter().find(|i| i.id == id)).and_then(|i| i.start);
        let Some(item) = items.iter_mut().find(|i| i.id == id) else { continue };
        item.status = Status::Working;
        item.started_at = Some(now_ms());
        if let Standing::Join(task) = standing {
            item.start = began;
            item.task = Some(task.clone());
            plan.join.push((task, item.clone()));
        } else {
            let task = task_bot(cfg);
            item.start = None;
            item.fork_failed = false;
            item.fork_from = fork_source(hub, &cfg.id);
            item.task = Some(task.id.clone());
            {
                let _g = hub.emit_lock.locked();
                hub.store.save_bot(&task)?;
            }
            plan.start.push((task, item.clone()));
        }
    }
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

/// Hands `item` to `task` as its next turn. `first`: the task is new.
fn begin(hub: &Arc<Hub>, task: &BotConfig, item: &Item, first: bool) {
    let text = prompts::item_prompt(item, first);
    if let Err(error) = crate::chat::messages::post(hub, task, json!({"text": text, "from": "host"})) {
        tracing::error!(task = %task.id, error = format!("{error:#}"), "couldn't hand work to a task");
        finished(hub, task, Outcome::Failed(Failure::Start, "it could not start".into()));
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
    fn why_an_item_waits_reads_as_the_wire_says() {
        assert_eq!(json!(Wait::Next), json!({"kind": "next"}));
        assert_eq!(json!(Wait::Behind { ahead: 2 }), json!({"kind": "behind", "ahead": 2}));
        assert_eq!(json!(Wait::After), json!({"kind": "after"}));
        assert_eq!(json!(Wait::Usage), json!({"kind": "usage"}));
    }
}
