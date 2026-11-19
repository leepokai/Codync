//! The dispatcher: which waiting task a thread takes next, which thread ends, and why a task still waits.

use super::sources::{self, Begin};
use super::tasks::{self, Status, Task};
use super::worktree::{self, Place, Worktree};
use super::{Outcome, finished, of, prompts};
use crate::LockExt;
use crate::analytics::errors::host_error;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, THREAD_PREFIX, now_ms};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// What a bot's waiting tasks can count on.
#[derive(Clone, Copy, Debug)]
pub(super) struct Capacity {
    /// New threads the bot may start now, under its ceiling.
    pub free: usize,
    /// The bot's threads at work.
    pub running: usize,
    /// The bot's agent is near its usage limit: one thread at a time.
    pub near_limit: bool,
    /// The bot's own turn is over: a new thread forks the chat's session, which a running turn would leave with an
    /// unanswered tool call.
    pub chat_free: bool,
}

/// A thread bot, to a task that follows one of its tasks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ThreadState {
    /// Its actor doesn't run: it ended, or was deleted.
    Gone,
    /// A turn of its runs.
    Busy,
    Idle,
}

/// Why a task waits (`work[].wait`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Wait {
    /// It starts when a thread is free.
    Next,
    /// So many tasks are ahead of it.
    Behind { ahead: usize },
    /// It follows a task that isn't finished.
    After,
    /// The agent is near its usage limit.
    Usage,
}

/// What the dispatcher does with a waiting task now.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Standing {
    /// The thread of the task it follows takes it, in the same session.
    Join(String),
    /// A new thread starts for it.
    Start,
    Wait(Wait),
}

enum Follows<'a> {
    /// An ordinary task: it follows nothing, or what it followed is gone, failed or was stopped.
    Nothing,
    /// What it follows isn't finished, or that thread is on another task.
    Unfinished,
    Thread(&'a str),
}

fn follows<'a>(tasks: &'a [Task], task: &Task, thread: &impl Fn(&str) -> ThreadState) -> Follows<'a> {
    let Some(before) = task.after.as_deref().and_then(|id| tasks.iter().find(|i| i.id == id)) else {
        return Follows::Nothing;
    };
    match (before.status, before.thread.as_deref()) {
        (Status::Waiting | Status::Working | Status::NeedsYou, _) => Follows::Unfinished,
        (Status::Done, Some(id)) if tasks::of_thread(tasks, id).is_some() => Follows::Unfinished,
        (Status::Done, Some(id)) => match thread(id) {
            ThreadState::Idle => Follows::Thread(id),
            ThreadState::Busy => Follows::Unfinished,
            ThreadState::Gone => Follows::Nothing,
        },
        _ => Follows::Nothing,
    }
}

/// Where each waiting task of `tasks` stands, lowest `order` first. Tasks that follow a finished one go to its
/// thread first: that thread already runs.
pub(super) fn standings(
    tasks: &[Task],
    capacity: &Capacity,
    thread: impl Fn(&str) -> ThreadState,
) -> Vec<(String, Standing)> {
    let mut waiting: Vec<&Task> = tasks.iter().filter(|i| i.status == Status::Waiting).collect();
    waiting.sort_by_key(|i| i.order);
    let (mut free, mut running) = (capacity.free, capacity.running);
    let mut taken: Vec<&str> = Vec::new();
    let mut found: Vec<Option<Standing>> = waiting
        .iter()
        .map(|task| match follows(tasks, task, &thread) {
            Follows::Nothing => None,
            Follows::Thread(id) if !taken.contains(&id) => {
                taken.push(id);
                free = free.saturating_sub(1);
                running += 1;
                Some(Standing::Join(id.to_owned()))
            }
            Follows::Thread(_) | Follows::Unfinished => Some(Standing::Wait(Wait::After)),
        })
        .collect();
    let mut ahead = 0;
    for standing in found.iter_mut().filter(|s| s.is_none()) {
        *standing = Some(if capacity.near_limit && running > 0 {
            Standing::Wait(Wait::Usage)
        } else if free > 0 && capacity.chat_free {
            free -= 1;
            running += 1;
            Standing::Start
        } else {
            ahead += 1;
            Standing::Wait(if ahead == 1 { Wait::Next } else { Wait::Behind { ahead: ahead - 1 } })
        });
    }
    waiting.iter().zip(found).filter_map(|(task, standing)| Some((task.id.clone(), standing?))).collect()
}

/// Whether a turn of `id` runs. `over`: the actor whose turn is ending as this is asked (its status still says it
/// works).
fn turn_runs(hub: &Hub, id: &str, over: Option<&str>) -> bool {
    over != Some(id) && matches!(hub.runtime(id).status, BotStatus::Working | BotStatus::NeedsInput)
}

pub(super) fn thread_state(hub: &Hub, thread: &str, over: Option<&str>) -> ThreadState {
    if !hub.has_actor(thread) {
        ThreadState::Gone
    } else if turn_runs(hub, thread, over) {
        ThreadState::Busy
    } else {
        ThreadState::Idle
    }
}

/// What `cfg`'s waiting tasks can count on now. `tasks`: its box.
pub(super) fn capacity(hub: &Hub, cfg: &BotConfig, tasks: &[Task], over: Option<&str>) -> Capacity {
    // A thread told to stop (its task stopped, gave way or was replaced) still works until its turn ends.
    let stopping = of(&hub.store, &cfg.id)
        .iter()
        .filter(|thread| tasks::of_thread(tasks, &thread.id).is_none() && turn_runs(hub, &thread.id, over))
        .count();
    let running = tasks::working(tasks) + stopping;
    // A turn the last host process left cut off is resumed by the bot's actor: it runs, though nothing says so yet.
    let chat_free = over == Some(cfg.id.as_str())
        || !(turn_runs(hub, &cfg.id, None) || crate::agent::bot::resumable(&hub.store, &cfg.id));
    // The limit of the agent a new thread starts on: past a resting one, its fallback's.
    let starts_on = crate::agent::fallback::starting(&hub.store, cfg, now_ms());
    Capacity {
        free: usize::from(cfg.max_threads).saturating_sub(running),
        running,
        near_limit: crate::usage::near_limit(hub, &starts_on),
        chat_free,
    }
}

/// What a pass decided, done once the box is saved.
#[derive(Default)]
struct Plan {
    /// A new thread and the task it starts on.
    start: Vec<(BotConfig, Task)>,
    /// A thread and the task it takes next.
    join: Vec<(String, Task)>,
    /// Threads with no task left to work on: their actor stops, their transcript stays.
    end: Vec<String>,
    /// Thread bots no task names any more.
    delete: Vec<String>,
    /// A task left the box as it was about to start, so the next may start: another pass.
    again: bool,
    /// Worktrees of tasks that are over, which nothing works in any more: each goes if it is untouched.
    settle: Vec<(String, Worktree)>,
}

/// Looks over `bot`'s box: waiting tasks go to threads where they can, threads with nothing left end. Run whenever the
/// box or one of its threads changes.
pub fn dispatch(hub: &Arc<Hub>, bot: &str) {
    run(hub, bot, None);
}

/// [`dispatch`], from the end of `over`'s turn (the bot's own, or one of its threads').
pub(super) fn run(hub: &Arc<Hub>, bot: &str, over: Option<&str>) {
    let Some(cfg) = hub.store.bot(bot).ok().flatten().filter(|r| !r.deleted).map(|r| r.config) else { return };
    let plan = match tasks::change(hub, bot, |tasks| plan(hub, &cfg, tasks, over)) {
        Ok(plan) => plan,
        Err(error) => {
            host_error!(hub, "couldn't look over a bot's work", bot, error = format!("{error:#}"));
            // The hourly look tries again.
            super::sweep::arm(hub, bot);
            return;
        }
    };
    for (thread, task) in plan.start {
        begin(hub, bot, &thread, &task, true);
    }
    for (thread, task) in plan.join {
        if let Ok(Some(row)) = hub.store.bot(&thread) {
            begin(hub, bot, &row.config, &task, false);
        } else {
            host_error!(hub, "a thread that took more work is gone", thread);
        }
    }
    for thread in plan.end {
        // Looked at again under the box's lock: a message the user wrote to its thread since reopened its task
        // (`thread::message`, which holds the lock from there to the actor).
        let _g = hub.work.boxes.locked();
        if tasks::of_thread(&tasks::load(&hub.store, bot), &thread).is_none() {
            hub.stop_actor(&thread);
        }
    }
    for thread in plan.delete {
        if let Err(error) = hub.delete_bot(&thread) {
            tracing::warn!(thread, error = format!("{error:#}"), "couldn't remove a thread");
        }
    }
    for (task, left) in plan.settle {
        let bot = bot.to_owned();
        worktree::settle_then(hub, &bot.clone(), &task, left, move |hub| dispatch(hub, &bot));
    }
    if plan.again {
        return run(hub, bot, over);
    }
    hub.emit_bot(bot);
    // What follows the bot from outside its chat counts it working while its box holds work.
    hub.outside_changed(bot);
    super::sweep::arm(hub, bot);
}

fn plan(hub: &Hub, cfg: &BotConfig, tasks: &mut Vec<Task>, over: Option<&str>) -> Result<Plan> {
    let mut plan = Plan::default();
    // A run whose routine was paused or deleted while it waited, a request the last host process took with it.
    tasks.retain(|i| i.status != Status::Waiting || !sources::withdrawn(hub, i));
    let capacity = capacity(hub, cfg, tasks, over);
    for (id, standing) in standings(tasks, &capacity, |thread| thread_state(hub, thread, over)) {
        if matches!(standing, Standing::Wait(_)) {
            continue;
        }
        // What it follows began its thread's session.
        let before = tasks.iter().find(|i| i.id == id).and_then(|i| i.after.clone());
        let before = before.and_then(|id| tasks.iter().position(|i| i.id == id));
        let began = before.and_then(|at| tasks[at].start);
        // Another thread of the bot works in its folder now: a new one beside it gets a worktree, in a repository.
        let side_by_side = tasks::working(tasks) > 0;
        let Some(at) = tasks.iter().position(|i| i.id == id) else { continue };
        // Tried again while the worktree it had is looked over: it starts once that is done, which looks again.
        if tasks[at].worktree.as_ref().is_some_and(|w| w.left == worktree::Left::Settling) {
            continue;
        }
        match sources::begin(hub, &cfg.id, &mut tasks[at])? {
            Begin::Go => {}
            Begin::Later => continue,
            Begin::Gone => {
                tasks.remove(at);
                plan.again = true;
                continue;
            }
        }
        // It goes on where the thread it joins works: a worktree that thread is in is this task's from here.
        let inherited = before
            .filter(|_| matches!(standing, Standing::Join(_)))
            .and_then(|before| tasks[before].worktree.take_if(|w| w.left == worktree::Left::InUse));
        let task = &mut tasks[at];
        task.status = Status::Working;
        task.started_at = Some(now_ms());
        if let Standing::Join(thread) = standing {
            task.start = began;
            task.thread = Some(thread.clone());
            task.worktree = inherited.or(task.worktree.take());
            plan.join.push((thread, task.clone()));
        } else {
            let thread = thread_bot(cfg);
            task.start = None;
            task.fork_failed = false;
            // One it has from before (it gave way, or is tried again) is where its work is.
            task.worktree.take_if(|w| !worktree::usable(w));
            // A bot's own folder keeps all its work in one place, with its versions (`agent::versions`).
            let beside = cfg.thread_worktrees
                && !crate::agent::workspace::is_managed(cfg)
                && task.source.is_bot()
                && side_by_side
                && worktree::in_repo(&cfg.cwd);
            task.place = if task.worktree.is_some() || beside { Place::Worktree } else { Place::Folder };
            // A routine's run starts as it did in the chat: a new session that knows only its own prompt.
            // ponytail: a thread in a worktree starts from a brief, not a fork: a fork is taken in another folder
            // than its session's, which no agent is checked to honor. Fork there once one is (stage 10's spikes).
            let forks = sources::from_chat(&task.source) && task.place == Place::Folder;
            task.fork_from = forks.then(|| fork_source(hub, &cfg.id)).flatten();
            task.thread = Some(thread.id.clone());
            {
                let _g = hub.emit_lock.locked();
                hub.store.save_bot(&thread)?;
            }
            plan.start.push((thread, task.clone()));
        }
    }
    plan.settle = worktree::over(tasks, |thread| turn_runs(hub, thread, over));
    for thread in of(&hub.store, &cfg.id) {
        if tasks::of_thread(tasks, &thread.id).is_some() || turn_runs(hub, &thread.id, over) {
            continue;
        }
        if !tasks.iter().any(|i| i.thread.as_deref() == Some(&thread.id)) {
            plan.delete.push(thread.id);
        } else if hub.has_actor(&thread.id) {
            plan.end.push(thread.id);
        }
    }
    Ok(plan)
}

/// Hands `task` of `bot` to `thread` as its next turn. `first`: the thread is new, and starts here. A routine run's
/// prompt and another bot's request go as they are: they say themselves what the turn is and where its reply goes.
/// Under the box's lock, and only while the task is still on that thread: a Stop since the plan found no turn to stop.
fn begin(hub: &Arc<Hub>, bot: &str, thread: &BotConfig, task: &Task, first: bool) {
    let posted = {
        let _g = hub.work.boxes.locked();
        if tasks::of_thread(&tasks::load(&hub.store, bot), &thread.id).is_none_or(|on| on.id != task.id) {
            return;
        }
        if first {
            hub.spawn_bot(thread.clone());
            sources::watch(hub, bot, &thread.id, task);
        }
        let text = if task.source.is_bot() { prompts::task_prompt(task, first) } else { task.brief.clone() };
        crate::chat::messages::post(hub, thread, json!({"text": text, "from": "host"}))
    };
    if let Err(error) = posted {
        host_error!(hub, "couldn't hand work to a thread", thread = %thread.id, error = format!("{error:#}"));
        finished(hub, thread, Outcome::not_dispatched(&error));
    }
}

/// A thread bot for `cfg`: its config, without what stays with the chat.
fn thread_bot(cfg: &BotConfig) -> BotConfig {
    BotConfig {
        id: format!("{THREAD_PREFIX}{}", uuid::Uuid::new_v4()),
        thread_of: Some(cfg.id.clone()),
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
        // As its bot's, wherever the thread works (a worktree isn't the bot's own folder).
        shared_memory: Some(crate::chat::memory::shared(cfg)),
        ..cfg.clone()
    }
}

/// `bot`'s main session, when a thread may start as a fork of it: the bot is on its own agent, and the session has
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

    fn task(id: &str, status: Status, order: i64) -> Task {
        serde_json::from_value(json!({
            "id": id, "title": id, "brief": "x", "status": status, "order": order, "createdAt": 0,
        }))
        .expect("a task")
    }

    fn after(id: &str, order: i64, before: &str) -> Task {
        Task { after: Some(before.into()), ..task(id, Status::Waiting, order) }
    }

    fn on(thread: &str, task: Task) -> Task {
        Task { thread: Some(thread.into()), ..task }
    }

    fn capacity(free: usize, running: usize) -> Capacity {
        Capacity { free, running, near_limit: false, chat_free: true }
    }

    fn stand(tasks: &[Task], capacity: Capacity, thread: ThreadState) -> Vec<(String, Standing)> {
        standings(tasks, &capacity, |_| thread)
    }

    fn of(standings: &[(String, Standing)]) -> Vec<(&str, &Standing)> {
        standings.iter().map(|(id, s)| (id.as_str(), s)).collect()
    }

    #[test]
    fn waiting_tasks_start_in_order_up_to_the_free_places_and_the_rest_say_how_far_back_they_are() {
        let tasks = [
            task("c", Status::Waiting, 3),
            task("a", Status::Waiting, -1),
            on("t1", task("w", Status::Working, 0)),
            task("b", Status::Waiting, 2),
            task("d", Status::Waiting, 4),
            task("old", Status::Done, 1),
        ];
        let standings = stand(&tasks, capacity(1, 1), ThreadState::Gone);
        assert_eq!(
            of(&standings),
            [
                ("a", &Standing::Start),
                ("b", &Standing::Wait(Wait::Next)),
                ("c", &Standing::Wait(Wait::Behind { ahead: 1 })),
                ("d", &Standing::Wait(Wait::Behind { ahead: 2 })),
            ]
        );
        let none = stand(&tasks, capacity(0, 1), ThreadState::Gone);
        assert_eq!(none[0], ("a".into(), Standing::Wait(Wait::Next)), "nothing is refused for being over the ceiling");
    }

    #[test]
    fn a_new_thread_waits_for_the_bots_own_turn_to_end() {
        let tasks = [task("a", Status::Waiting, 1)];
        let busy = Capacity { chat_free: false, ..capacity(3, 0) };
        assert_eq!(stand(&tasks, busy, ThreadState::Gone)[0].1, Standing::Wait(Wait::Next));
    }

    #[test]
    fn near_the_usage_limit_one_thread_runs_at_a_time() {
        let tasks = [task("a", Status::Waiting, 1), task("b", Status::Waiting, 2)];
        let near = Capacity { near_limit: true, ..capacity(3, 0) };
        let standings = stand(&tasks, near, ThreadState::Gone);
        assert_eq!(of(&standings), [("a", &Standing::Start), ("b", &Standing::Wait(Wait::Usage))]);
    }

    #[test]
    fn a_task_follows_another_into_its_thread_once_that_thread_is_idle() {
        let working = [on("t1", task("a", Status::Working, 0)), after("b", 1, "a")];
        assert_eq!(stand(&working, capacity(2, 1), ThreadState::Busy)[0].1, Standing::Wait(Wait::After));

        let done = [on("t1", task("a", Status::Done, 0)), after("b", 1, "a"), after("c", 2, "a")];
        let standings = stand(&done, capacity(3, 0), ThreadState::Idle);
        assert_eq!(
            of(&standings),
            [("b", &Standing::Join("t1".into())), ("c", &Standing::Wait(Wait::After))],
            "one task at a time in a session"
        );
        assert_eq!(stand(&done, capacity(3, 0), ThreadState::Busy)[0].1, Standing::Wait(Wait::After));

        // The thread that would take it is gone, or what it followed failed or was stopped: an ordinary task.
        assert_eq!(stand(&done, capacity(3, 0), ThreadState::Gone)[0].1, Standing::Start);
        let stopped = [on("t1", task("a", Status::Stopped, 0)), after("b", 1, "a")];
        assert_eq!(stand(&stopped, capacity(3, 0), ThreadState::Idle)[0].1, Standing::Start);
        let dropped = [after("b", 1, "a")];
        assert_eq!(stand(&dropped, capacity(3, 0), ThreadState::Idle)[0].1, Standing::Start);
        // What it follows hasn't started yet.
        let waiting = [task("a", Status::Waiting, 5), after("b", 1, "a")];
        let standings = stand(&waiting, capacity(1, 0), ThreadState::Gone);
        assert_eq!(of(&standings), [("b", &Standing::Wait(Wait::After)), ("a", &Standing::Start)]);
    }

    #[test]
    fn an_idle_thread_keeps_its_place_for_the_task_that_follows() {
        // One thread at once: the thread of `a` is idle and `c` follows `a`, so the ordinary `b` ahead of it still waits.
        let tasks = [on("t1", task("a", Status::Done, 0)), task("b", Status::Waiting, 1), after("c", 2, "a")];
        let standings = stand(&tasks, capacity(1, 0), ThreadState::Idle);
        assert_eq!(of(&standings), [("b", &Standing::Wait(Wait::Next)), ("c", &Standing::Join("t1".into()))]);
    }

    #[test]
    fn work_stopped_before_its_thread_started_never_reaches_it() {
        let hub = hub();
        let cfg = bot("b1", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        crate::chat::work::put_working_for_test(&hub.store, "b1", "thread-1");
        let planned = tasks::load(&hub.store, "b1").remove(0);
        // A Stop between the plan and the start: no actor yet to tell.
        tasks::change(&hub, "b1", |tasks| {
            tasks[0].status = Status::Stopped;
            Ok(())
        })
        .expect("stopped");
        let thread = BotConfig { id: "thread-1".into(), thread_of: Some("b1".into()), ..cfg };
        begin(&hub, "b1", &thread, &planned, true);
        assert!(!hub.has_actor("thread-1"), "it never starts");
    }

    #[test]
    fn work_tried_again_waits_while_its_worktree_is_looked_over() {
        let hub = hub();
        hub.store.save_bot(&BotConfig { max_threads: 2, ..bot("b1", "Mika", "/tmp") }).expect("saved");
        let worktree = json!({"branch": "codync/t", "path": "/w", "cwd": "/w", "base": "abc", "left": "settling"});
        let waiting = json!({"id": "i1", "title": "Work", "brief": "x", "status": "waiting", "order": 1,
            "createdAt": 0, "worktree": worktree});
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &json!([waiting]).to_string()).expect("saved");
        run(&hub, "b1", None);
        assert_eq!(tasks::load(&hub.store, "b1")[0].status, Status::Waiting);
    }

    #[test]
    fn a_thread_still_stopping_keeps_its_place() {
        let hub = hub();
        let cfg = BotConfig { max_threads: 1, ..bot("b1", "Mika", "/tmp") };
        hub.store.save_bot(&cfg).expect("saved");
        let thread = BotConfig { id: "thread-1".into(), thread_of: Some("b1".into()), ..cfg.clone() };
        hub.store.save_bot(&thread).expect("saved");
        hub.set_runtime("thread-1", |rt| rt.status = BotStatus::Working);
        let replaced = [on("thread-1", task("a", Status::Stopped, 0)), task("b", Status::Waiting, 1)];
        let p = super::capacity(&hub, &cfg, &replaced, None);
        assert_eq!((p.free, p.running), (0, 1), "the replacement waits for it");
        assert_eq!(super::capacity(&hub, &cfg, &replaced, Some("thread-1")).free, 1, "its turn is over");
    }

    #[test]
    fn why_a_task_waits_reads_as_the_wire_says() {
        assert_eq!(json!(Wait::Next), json!({"kind": "next"}));
        assert_eq!(json!(Wait::Behind { ahead: 2 }), json!({"kind": "behind", "ahead": 2}));
        assert_eq!(json!(Wait::After), json!({"kind": "after"}));
        assert_eq!(json!(Wait::Usage), json!({"kind": "usage"}));
    }
}
