//! The watchdog's look at a bot's own working tasks ([watchdog](../../../../docs/features/work.md#watchdog)): every
//! five minutes, code alone reads each one's thread. A thread whose turn repeats one tool call, keeps failing or has
//! shown nothing new for an hour gets its bot a turn in its chat to look at it (`Cmd::Watchdog`), at most once an
//! hour; a card of its that has waited on the user for two hours reminds them, once. A routine's run and another
//! bot's request aren't watched: their result has an owner other than the bot's chat.

use super::tasks::{self, Task};
use super::{Status, prompts};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::agent::fallback;
use crate::analytics::{self, Event};
use crate::hub::{BotStatus, Hub};
use crate::remote::push::{self, AlertKind};
use crate::store::{BotConfig, Entry, EntryKind, Store, now_ms};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

/// How often the watchdog looks; `CODYNC_WORK_WATCH_SECS` sets it for the end-to-end test.
const EVERY: Duration = Duration::from_mins(5);
/// The same tool call this many times in a row in a turn is a loop.
const LOOP: usize = 6;
/// Failed tool calls in a row in a turn.
const FAILS: usize = 5;
const MINUTE: i64 = 60_000;
/// A turn with no new or changed step this long is quiet.
const QUIET: i64 = 60 * MINUTE;
/// A card that has waited on the user this long reminds them, once.
const CARD: i64 = 120 * MINUTE;
/// A bot is woken for one thread at most this often.
const COOLDOWN: i64 = 60 * MINUTE;
/// A thread's newest entries a look reads.
const RECENT: i64 = 50;
/// How much of a tool call's title the bot is told.
const TITLE: usize = 120;

/// What looks wrong with a working thread's turn.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Signal {
    /// Its turn's last [`LOOP`] tool calls are one call with one result, of this title.
    Looping(String),
    /// Its turn's last [`FAILS`] tool calls failed: their titles, oldest first.
    Failing(Vec<String>),
    /// Its turn has shown no new step for so many minutes, with no tool call running.
    Quiet(i64),
}

impl Signal {
    /// `watchdog_flagged`'s `signal`.
    fn name(&self) -> &'static str {
        match self {
            Self::Looping(_) => "looping",
            Self::Failing(_) => "failing",
            Self::Quiet(_) => "quiet",
        }
    }

    /// What shows it, in words for the bot.
    fn evidence(&self) -> String {
        match self {
            Self::Looping(title) => format!("it has called \"{title}\" {LOOP} times in a row"),
            Self::Failing(titles) => {
                let titles: Vec<String> = titles.iter().map(|title| format!("\"{title}\"")).collect();
                format!("its last {} tool calls failed ({})", titles.len(), titles.join(", "))
            }
            Self::Quiet(minutes) => format!("its turn has shown no new step for {minutes} minutes"),
        }
    }
}

/// Why a flag woke no one (`watchdog_flagged`'s `reason`).
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum Skip {
    /// The bot was woken for this thread less than [`COOLDOWN`] ago.
    Cooldown,
    /// The chat's own agent rests (`agent::fallback::rest`).
    Rests,
    /// The bot's agent is near its usage limit.
    NearLimit,
}

/// What a flag did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    /// A turn in the bot's chat.
    Woke,
    /// A push to the user.
    Reminded,
    /// Neither, for this reason.
    Skipped(Skip),
}

impl Action {
    /// `watchdog_flagged`'s properties for a flag of `signal` that did this.
    fn properties(self, signal: &str) -> Map<String, Value> {
        let (action, reason) = match self {
            Self::Woke => ("woke", None),
            Self::Reminded => ("reminded", None),
            Self::Skipped(reason) => ("skipped", Some(reason)),
        };
        let mut properties = Map::from_iter([("signal".into(), signal.into()), ("action".into(), action.into())]);
        if let Some(reason) = reason {
            properties.insert("reason".into(), json!(reason));
        }
        properties
    }
}

/// What the watchdog remembers while the host runs (`Work::watch`).
#[derive(Default)]
pub(super) struct Memory {
    /// Thread → when its bot was last woken for it.
    woke: HashMap<String, i64>,
    /// Thread → why its last flag woke no one, so each reason is counted once in a row.
    skipped: HashMap<String, Skip>,
    /// The cards the user was reminded of.
    reminded: HashSet<String>,
}

/// One of a bot's tasks that looks stuck, for a turn in its chat (`Cmd::Watchdog`).
pub struct Stuck {
    task: String,
    thread: String,
    /// What shows it, in words for the bot.
    evidence: String,
}

impl Stuck {
    /// The thread that looks stuck.
    pub fn thread(&self) -> &str {
        &self.thread
    }

    /// The prompt of `bot`'s turn for it and the hops a task added there carries on; none once the task no longer
    /// works on that thread (it ended, or was set aside), which leaves nothing to look at.
    pub fn turn(&self, store: &Store, bot: &str) -> Option<(String, u8)> {
        let tasks = tasks::load(store, bot);
        let task = tasks::of_thread(&tasks, &self.thread).filter(|task| task.id == self.task)?;
        Some((prompts::stuck(task, &self.evidence), task.hops))
    }
}

/// Looks at every bot's working tasks every [`EVERY`], for as long as the host runs.
pub(super) fn start(hub: &Arc<Hub>) {
    // Unit tests run without a runtime: no timer.
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let every =
        std::env::var("CODYNC_WORK_WATCH_SECS").ok().and_then(|s| s.parse().ok()).map_or(EVERY, Duration::from_secs);
    let hub = Arc::downgrade(hub);
    runtime.spawn(async move {
        let mut awake = now_ms();
        let mut last = awake;
        loop {
            tokio::time::sleep(every).await;
            let Some(hub) = hub.upgrade() else { return };
            let now = now_ms();
            // A turn shows nothing while the computer sleeps.
            if crate::watchdog::slept(last, now, every) {
                awake = now;
            }
            last = now;
            if let Err(error) = tokio::task::spawn_blocking(move || look(&hub, awake)).await {
                tracing::warn!(error = format!("{error:#}"), "the watchdog's look failed");
            }
        }
    });
}

/// One look at every bot's working tasks; `awake`: since when the computer has been awake.
fn look(hub: &Hub, awake: i64) {
    let now = now_ms();
    let (mut threads, mut cards) = (HashSet::new(), HashSet::new());
    for bot in tasks::bots(&hub.store) {
        let Some(cfg) = hub.store.bot(&bot).ok().flatten().filter(|row| !row.deleted).map(|row| row.config) else {
            continue;
        };
        let watched =
            tasks::load(&hub.store, &bot).into_iter().filter(|i| i.status == Status::Working && i.source.is_bot());
        for task in watched {
            let Some(thread) = task.thread.clone() else { continue };
            let open = hub.store.open_cards(&bot, &thread).unwrap_or_else(|error| {
                tracing::warn!(thread, error = format!("{error:#}"), "couldn't read a thread's cards");
                Vec::new()
            });
            cards.extend(open.iter().map(|card| card.id.clone()));
            let open: Vec<Entry> = {
                let memory = hub.work.watch.locked();
                open.into_iter().filter(|card| !memory.reminded.contains(&card.id)).collect()
            };
            if let Some((card, asks)) = overdue_card(&open, now) {
                remind(hub, &cfg, card, &asks);
            } else {
                act(hub, &cfg, &task, &thread, see(hub, &task, &thread, awake, now), now);
            }
            threads.insert(thread);
        }
    }
    // What is no longer watched is forgotten.
    let mut memory = hub.work.watch.locked();
    memory.woke.retain(|thread, _| threads.contains(thread));
    memory.skipped.retain(|thread, _| threads.contains(thread));
    memory.reminded.retain(|card| cards.contains(card));
}

/// What looks wrong with `task`'s `thread` now, if anything.
fn see(hub: &Hub, task: &Task, thread: &str, awake: i64, now: i64) -> Option<Signal> {
    let rt = hub.runtime(thread);
    let turn = rt.started_at.filter(|_| matches!(rt.status, BotStatus::Working | BotStatus::NeedsInput));
    let entries = hub
        .store
        .history(thread, i64::MAX, RECENT)
        .inspect_err(|error| tracing::warn!(thread, error = format!("{error:#}"), "couldn't read a thread's steps"))
        .ok()?;
    signal(&entries, turn, tasks::asks_user(hub, task), awake, now)
}

/// What looks wrong with a working thread at `now`, if anything. `entries`: its newest transcript entries, oldest
/// first; `turn`: when its running turn began (none while no turn runs); `waits`: it waits on the user (a card, a
/// sign-in); `awake`: since when the computer has been awake.
fn signal(entries: &[Entry], turn: Option<i64>, waits: bool, awake: i64, now: i64) -> Option<Signal> {
    // Waiting on the user is no sign of anything, and a thread between turns has nothing running to look at.
    let start = turn.filter(|_| !waits)?;
    let steps: Vec<&Entry> = entries.iter().filter(|e| e.created_at >= start).collect();
    let calls: Vec<&Entry> = steps.iter().copied().filter(|e| e.kind == EntryKind::Tool.as_str()).collect();
    let last = |n: usize| calls.get(calls.len().saturating_sub(n)..).filter(|last| last.len() == n);
    let title = |e: &Entry| codync_agent::acp::truncate(e.data["title"].as_str().unwrap_or_default(), TITLE);
    // One call with one result: a title can leave the call's arguments out (an MCP tool's is its name alone), and a
    // call with no title of its own (`agent::bot::updates` names it "Tool") is no call in particular.
    // ponytail: a call run again whose output changes each time (a timing, a temporary path) isn't seen looping; a hash
    // of the call's `rawInput` kept on its entry would tell the same call by its arguments.
    let same =
        |a: &Entry, b: &Entry| ["title", "output", "diffs", "locations"].iter().all(|k| a.data[*k] == b.data[*k]);
    if let Some(repeated) = last(LOOP)
        && repeated[0].data["title"] != "Tool"
        && repeated.iter().all(|e| same(e, repeated[0]))
    {
        return Some(Signal::Looping(title(repeated[0])));
    }
    if let Some(failed) = last(FAILS)
        && failed.iter().all(|e| e.data["status"] == "failed")
    {
        return Some(Signal::Failing(failed.iter().map(|e| title(e)).collect()));
    }
    // A tool call still running (a long build) is not silence.
    let running = calls.iter().any(|e| matches!(e.data["status"].as_str(), Some("pending" | "in_progress")));
    let shown = steps.iter().map(|e| e.updated_at).max().unwrap_or(start).max(start).max(awake);
    (!running && now - shown >= QUIET).then(|| Signal::Quiet((now - shown) / MINUTE))
}

/// The first of a thread's `cards` (in its bot's chat, waiting on the user, not reminded of yet, oldest first) that
/// has waited for [`CARD`] at `now`: its id, and what it asks.
fn overdue_card(cards: &[Entry], now: i64) -> Option<(&str, String)> {
    let card = cards.iter().find(|card| now - card.created_at >= CARD)?;
    let asks = card.data["title"].as_str().or_else(|| card.data["question"].as_str()).unwrap_or_default();
    Some((&card.id, codync_agent::acp::truncate(asks, TITLE)))
}

/// Reminds the user, once, of card `id` (asking `asks`), from the bot's chat, where the card is: a thread alerts no
/// one itself.
fn remind(hub: &Hub, cfg: &BotConfig, id: &str, asks: &str) {
    hub.work.watch.locked().reminded.insert(id.to_owned());
    push::notify(hub, cfg, None, &format!("{} needs you", cfg.name), asks, AlertKind::NeedsInput);
    analytics::capture(hub, Event::WatchdogFlagged, Action::Reminded.properties("card"));
}

/// Acts on what the look at `task`'s `thread` saw: something wrong wakes the bot.
fn act(hub: &Hub, cfg: &BotConfig, task: &Task, thread: &str, signal: Option<Signal>, now: i64) {
    let Some(signal) = signal else {
        // Fine again: a later flag's reason to wait is counted anew.
        hub.work.watch.locked().skipped.remove(thread);
        return;
    };
    if let Some(action) = wake(hub, cfg, task, thread, &signal, now) {
        analytics::capture(hub, Event::WatchdogFlagged, action.properties(signal.name()));
    }
}

/// Wakes `cfg` for `task`, whose `thread` looks stuck: a turn in its chat, behind what the user wrote there.
/// Answers what to count: the wake, or what held it back the first time in a row; none when it couldn't be handed
/// over (logged).
fn wake(hub: &Hub, cfg: &BotConfig, task: &Task, thread: &str, signal: &Signal, now: i64) -> Option<Action> {
    if let Some(skip) = held(hub, cfg, thread, now) {
        let again = hub.work.watch.locked().skipped.insert(thread.to_owned(), skip) == Some(skip);
        return (!again).then_some(Action::Skipped(skip));
    }
    let stuck = Stuck { task: task.id.clone(), thread: thread.to_owned(), evidence: signal.evidence() };
    if let Err(error) = hub.send_cmd(&cfg.id, Cmd::Watchdog(stuck)) {
        let error = format!("{error:#}");
        tracing::warn!(bot = %cfg.id, task = task.id, error, "couldn't wake a bot for a stuck task");
        return None;
    }
    let mut memory = hub.work.watch.locked();
    memory.woke.insert(thread.to_owned(), now);
    memory.skipped.remove(thread);
    Some(Action::Woke)
}

/// Why `cfg` isn't woken for `thread` at `now`, if it isn't: it was within the hour, the chat's own agent rests,
/// or that agent is near its usage limit.
fn held(hub: &Hub, cfg: &BotConfig, thread: &str, now: i64) -> Option<Skip> {
    if hub.work.watch.locked().woke.get(thread).is_some_and(|at| now - at < COOLDOWN) {
        Some(Skip::Cooldown)
    } else if fallback::rest::get(&hub.store, &fallback::login::of(cfg), now).is_some() {
        Some(Skip::Rests)
    } else if crate::usage::near_limit(hub, cfg) {
        Some(Skip::NearLimit)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use crate::store::Lane;

    const HOUR: i64 = 60 * MINUTE;
    /// When the turn under test began, and the look's time.
    const START: i64 = 1_000_000;
    const NOW: i64 = START + 3 * HOUR;

    fn entry(kind: EntryKind, at: i64, data: Value) -> Entry {
        Entry {
            id: format!("e{at}"),
            seq: at,
            bot_id: "thread-1".into(),
            rev: at,
            kind: kind.as_str().into(),
            turn: 1,
            data,
            created_at: at,
            updated_at: at,
        }
    }

    /// Tool calls of the turn, one a minute up to just now, each `(title, status)`.
    fn calls(calls: &[(&str, &str)]) -> Vec<Entry> {
        let first = NOW - i64::try_from(calls.len()).expect("a few") * MINUTE;
        let at = |i: usize| first + i64::try_from(i).expect("a few") * MINUTE;
        calls
            .iter()
            .enumerate()
            .map(|(i, (title, status))| entry(EntryKind::Tool, at(i), json!({"title": title, "status": status})))
            .collect()
    }

    fn look_at(entries: &[Entry]) -> Option<Signal> {
        signal(entries, Some(START), false, START, NOW)
    }

    #[test]
    fn a_turn_that_makes_one_call_again_and_again_loops() {
        let same = [("Read build.log", "completed"); LOOP];
        assert_eq!(look_at(&calls(&same)), Some(Signal::Looping("Read build.log".into())));
        assert_eq!(look_at(&calls(&same[1..])), None, "five times is not yet a loop");
        let mut varied = same;
        varied[2].0 = "Edit main.rs";
        assert_eq!(look_at(&calls(&varied)), None);
        // One tool with other arguments each time: an MCP tool's title is its name alone.
        let mut queries = calls(&[("mcp__posthog__exec", "completed"); LOOP]);
        for (n, query) in queries.iter_mut().enumerate() {
            query.data["output"] = json!(format!("{n} rows"));
        }
        assert_eq!(look_at(&queries), None);
        assert_eq!(look_at(&calls(&[("Tool", "completed"); LOOP])), None, "no title of its own");
        // The same calls in the turn before this one don't count.
        let before = calls(&same);
        assert_eq!(signal(&before, Some(NOW), false, START, NOW), None);
    }

    #[test]
    fn a_turn_whose_calls_keep_failing_is_failing() {
        let failing = [
            ("npm test", "failed"),
            ("npm run build", "failed"),
            ("npm test", "failed"),
            ("tsc", "failed"),
            ("npm test", "failed"),
        ];
        let titles: Vec<String> = failing.iter().map(|(t, _)| (*t).to_owned()).collect();
        assert_eq!(look_at(&calls(&failing)), Some(Signal::Failing(titles)));
        let mut one_passed = failing;
        one_passed[3].1 = "completed";
        assert_eq!(look_at(&calls(&one_passed)), None);
    }

    #[test]
    fn a_turn_silent_for_an_hour_is_quiet_unless_a_call_runs_or_the_user_is_asked() {
        let silent = vec![entry(EntryKind::Agent, NOW - 61 * MINUTE, json!({"text": "Building…"}))];
        assert_eq!(look_at(&silent), Some(Signal::Quiet(61)));
        // Something changed since: a step's update counts as much as a new one.
        let updated = vec![Entry { updated_at: NOW - MINUTE, ..silent[0].clone() }];
        assert_eq!(look_at(&updated), None);
        // A long build is not stuck.
        let building = calls(&[("cargo build", "in_progress")]);
        let building = vec![Entry { created_at: NOW - 2 * HOUR, updated_at: NOW - 2 * HOUR, ..building[0].clone() }];
        assert_eq!(look_at(&building), None);
        // A thread waiting on the user, or between turns, has nothing to say.
        assert_eq!(signal(&silent, Some(START), true, START, NOW), None);
        assert_eq!(signal(&silent, None, false, START, NOW), None);
        // The computer woke 10 minutes ago: the hour counts from there.
        assert_eq!(signal(&silent, Some(START), false, NOW - 10 * MINUTE, NOW), None);
        // A turn that never showed a step is timed from its start.
        assert_eq!(look_at(&[]), Some(Signal::Quiet(180)));
    }

    #[test]
    fn a_card_left_two_hours_is_overdue() {
        let card = |at: i64| entry(EntryKind::Permission, at, json!({"title": "Run the tests", "status": "pending"}));
        let waited = [card(NOW - 2 * HOUR)];
        assert_eq!(overdue_card(&waited, NOW), Some((waited[0].id.as_str(), "Run the tests".into())));
        assert_eq!(overdue_card(&[card(NOW - HOUR)], NOW), None, "an hour is not yet");
        let question = [entry(EntryKind::Notice, NOW - 3 * HOUR, json!({"decision": true, "question": "Which?"}))];
        assert_eq!(overdue_card(&question, NOW).map(|(_, asks)| asks), Some("Which?".into()), "a choice card");
    }

    #[test]
    fn the_bot_is_told_what_shows_its_task_stuck() {
        assert_eq!(Signal::Looping("Read x".into()).evidence(), "it has called \"Read x\" 6 times in a row");
        let failing = Signal::Failing(vec!["a".into(), "b".into()]);
        assert_eq!(failing.evidence(), "its last 2 tool calls failed (\"a\", \"b\")");
        assert_eq!(Signal::Quiet(75).evidence(), "its turn has shown no new step for 75 minutes");
    }

    #[test]
    fn a_flag_counts_its_signal_what_it_did_and_why_not() {
        let counted = |action: Action, signal: &str| Value::Object(action.properties(signal));
        assert_eq!(counted(Action::Woke, "looping"), json!({"signal": "looping", "action": "woke"}));
        assert_eq!(counted(Action::Reminded, "card"), json!({"signal": "card", "action": "reminded"}));
        let held = json!({"signal": "quiet", "action": "skipped", "reason": "near_limit"});
        assert_eq!(counted(Action::Skipped(Skip::NearLimit), "quiet"), held);
    }

    /// Bot `b1` with a working task on `thread-1` (`source`: its kind, none for its own), whose turn has made the
    /// same call six times.
    fn looping(source: Option<Value>) -> Arc<Hub> {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let mut task = json!({"id": "i1", "title": "Work", "brief": "x", "status": "working", "order": 1,
            "createdAt": 0, "thread": "thread-1", "hops": 3});
        if let Some(source) = source {
            task["source"] = source;
        }
        hub.store.kv_set(&format!("{}b1", tasks::PREFIX), &json!([task]).to_string()).expect("saved");
        hub.set_runtime("thread-1", |rt| {
            rt.status = BotStatus::Working;
            rt.started_at = Some(0);
        });
        for _ in 0..LOOP {
            let call = json!({"title": "Read build.log", "status": "completed"});
            hub.store.insert_entry(&Lane::main("thread-1"), EntryKind::Tool, 1, &call).expect("a step");
        }
        hub
    }

    #[test]
    fn only_the_bots_own_work_is_watched() {
        // The chat's agent rests, so a flag is held back and remembered: what was looked at shows.
        let rests = |hub: &Hub| {
            let login = fallback::login::of(&bot("b1", "Mika", "/tmp"));
            fallback::rest::record(&hub.store, &login, fallback::Reason::UsageLimit, now_ms(), None);
        };
        let own = looping(None);
        rests(&own);
        look(&own, 0);
        assert_eq!(own.work.watch.locked().skipped.get("thread-1"), Some(&Skip::Rests));

        let routine = looping(Some(json!({"kind": "routine", "id": "r", "run": "u"})));
        rests(&routine);
        look(&routine, 0);
        assert!(routine.work.watch.locked().skipped.is_empty(), "a routine's run has its owner");
    }

    #[test]
    fn a_threads_card_left_two_hours_in_its_bots_chat_reminds_the_user_once() {
        let hub = looping(None);
        let card = |thread: &str, status: &str| {
            let data = json!({"title": "Run the tests", "status": status, "thread": {"id": thread, "title": "Work"}});
            let card = hub.store.insert_entry(&Lane::main("b1"), EntryKind::Permission, 1, &data).expect("a card");
            hub.store.backdate_entry(&card.id, now_ms() - 3 * HOUR);
            card.id
        };
        let waiting = card("thread-1", "pending");
        card("thread-2", "pending");
        card("thread-1", "allowed");
        hub.set_runtime("thread-1", |rt| rt.status = BotStatus::NeedsInput);
        look(&hub, 0);
        let reminded = || hub.work.watch.locked().reminded.clone();
        assert_eq!(reminded(), HashSet::from([waiting.clone()]), "its own card, still waiting");
        look(&hub, 0);
        assert_eq!(reminded(), HashSet::from([waiting]), "once");
        assert!(hub.work.watch.locked().woke.is_empty(), "a thread waiting on the user wakes no one");
    }

    #[test]
    fn a_thread_wakes_its_bot_at_most_once_an_hour_and_each_hold_counts_once() {
        let hub = looping(None);
        let cfg = bot("b1", "Mika", "/tmp");
        let now = now_ms();
        assert_eq!(held(&hub, &cfg, "thread-1", now), None);
        hub.work.watch.locked().woke.insert("thread-1".into(), now - 59 * MINUTE);
        assert_eq!(held(&hub, &cfg, "thread-1", now), Some(Skip::Cooldown));
        let task = tasks::load(&hub.store, "b1").remove(0);
        let signal = Signal::Looping("Read build.log".into());
        let skipped = Some(Action::Skipped(Skip::Cooldown));
        assert_eq!(wake(&hub, &cfg, &task, "thread-1", &signal, now), skipped);
        assert_eq!(wake(&hub, &cfg, &task, "thread-1", &signal, now), None, "counted once in a row");
        hub.work.watch.locked().woke.insert("thread-1".into(), now - 61 * MINUTE);
        assert_eq!(held(&hub, &cfg, "thread-1", now), None, "an hour on");
        // No actor takes it here: nothing is stamped, so the next look tries again.
        assert_eq!(wake(&hub, &cfg, &task, "thread-1", &signal, now), None);
        assert_eq!(hub.work.watch.locked().woke.get("thread-1"), Some(&(now - 61 * MINUTE)));
    }

    #[test]
    fn the_bots_turn_names_the_task_and_is_dropped_once_the_task_moved_on() {
        let hub = looping(None);
        let stuck = Stuck {
            task: "i1".into(),
            thread: "thread-1".into(),
            evidence: "it has called \"x\" 6 times in a row".into(),
        };
        let (prompt, hops) = stuck.turn(&hub.store, "b1").expect("still working");
        assert_eq!(hops, 3, "a task it adds carries the chain on");
        assert_eq!(
            prompt,
            "[Codync watchdog] Task \"Work\" (i1) looks stuck: it has called \"x\" 6 times in a row. Look at it \
             (work_status, recall_task) and do one thing: tell_task it what to change, steer_task it (stop, or replace \
             it with a better brief), or, if it is fine, reply exactly (pass). This is Codync checking on your work, \
             not the user speaking: if you act, tell the user in a sentence what you did."
        );
        let elsewhere = Stuck { thread: "thread-2".into(), ..stuck };
        assert!(elsewhere.turn(&hub.store, "b1").is_none(), "set aside, or another thread has it now");
    }
}
