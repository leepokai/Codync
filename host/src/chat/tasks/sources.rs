//! Items that come from elsewhere than the bot's own `add_work`: a run of one of its routines, and another bot's
//! `message_bot` request. They wait and start like any item, so the chat stays free and they count under
//! `maxAgents`. Their result takes no turn of the bot's: a run's last message is the routine's message in the chat,
//! as a run in the chat posted it, and a request's is the reply the asking bot gets.

use super::items::{self, Item, Source, Status};
use super::{Failure, MAX_TITLE, NOT_STARTED, dispatch, end, ended};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::chat::team::{BotRequest, no_reply};
use crate::hub::Hub;
use crate::routines;
use crate::store::{BotConfig, EntryKind, Lane, Store};
use anyhow::{Result, anyhow};
use codync_network::Access;
use serde_json::json;
use std::sync::Arc;

/// Whether `cfg`'s routine runs and the requests other bots send it are items in its box. A bot that uses the
/// computer or manages the team keeps them as turns in its chat: a task has neither (one screen and one team stay
/// with the chat), so a run or a request that needs them would stop working.
pub fn boxed(cfg: &BotConfig) -> bool {
    !(cfg.is_task() || cfg.is_group() || cfg.computer || cfg.manage_team)
}

/// Where the item task `cfg` works on came from; the bot's own for anything that isn't a task.
pub fn source_of(store: &Store, cfg: &BotConfig) -> Source {
    if !cfg.is_task() {
        return Source::Bot;
    }
    super::item_of(store, cfg).map(|item| item.source).unwrap_or_default()
}

/// The ceiling a network rule holds task `cfg`'s turns to: its request's, on another computer's request (`Allow`
/// for anything else). A request the box no longer holds is held to Ask, so it never works more freely than it got in.
pub fn access_of(hub: &Hub, cfg: &BotConfig) -> Access {
    match super::item_of(&hub.store, cfg) {
        Some(item) if matches!(item.source, Source::Request { .. }) => {
            hub.tasks.requests.locked().get(&item.id).map_or(Access::Ask, BotRequest::access)
        }
        _ => Access::Allow,
    }
}

/// Whether a task that takes an item from `source` starts from the chat (a fork of its session, else the chat so
/// far). A routine's run doesn't: in the chat it had a session of its own with only its prompt, and it costs and
/// sees the same here.
pub fn from_chat(source: &Source) -> bool {
    !matches!(source, Source::Routine { .. })
}

/// `work_added` for an item that came from `source`.
fn added(hub: &Hub, source: &Source) {
    let properties = json!({"outcome": "added", "after": false, "source": source.kind()});
    analytics::capture(hub, Event::WorkAdded, properties.as_object().cloned().unwrap_or_default());
}

/// Puts `item` last in `bot`'s box.
fn put(hub: &Hub, bot: &str, mut item: Item) -> Result<()> {
    items::change(hub, bot, |items| {
        item.order = items::last(items);
        items.push(item);
        Ok(())
    })
}

/// A run of `bot`'s routine `routine` (named `name`) waits in its box. Its prompt is written when it starts, from
/// the routine as it is then. A run already there isn't added twice.
pub fn add_routine(hub: &Arc<Hub>, bot: &str, routine: &str, run: &str, name: &str) -> Result<()> {
    let known = |item: &Item| matches!(&item.source, Source::Routine { run: r, .. } if r == run);
    if !items::load(&hub.store, bot).iter().any(known) {
        let source = Source::Routine { id: routine.to_owned(), run: run.to_owned() };
        added(hub, &source);
        put(hub, bot, Item { source, ..Item::waiting(&title(name), "", None, None, 0) })?;
    }
    dispatch(hub, bot);
    Ok(())
}

/// Another bot's request waits in `bot`'s box. One that can't be saved is dropped, which tells the asking bot it
/// was cancelled.
pub fn add_request(hub: &Arc<Hub>, bot: &str, request: BotRequest) {
    let source = Source::Request { id: request.sender().map(str::to_owned) };
    let item = Item { source, ..Item::waiting(&title(request.quote()), &request.prompt, None, None, 0) };
    let (id, source) = (item.id.clone(), item.source.clone());
    hub.tasks.requests.locked().insert(id.clone(), request);
    if let Err(error) = put(hub, bot, item) {
        tracing::error!(bot, error = format!("{error:#}"), "couldn't put a bot's request in the box");
        hub.tasks.requests.locked().remove(&id);
        return;
    }
    added(hub, &source);
    dispatch(hub, bot);
}

/// A row's title from `text`: its first line, as long as a title may be.
fn title(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("Request");
    line.chars().take(MAX_TITLE).collect()
}

/// A routine was paused, deleted or changed: runs of it that waited in the box and were cancelled leave it.
pub fn routines_changed(hub: &Arc<Hub>, bot: &str) {
    dispatch(hub, bot);
}

/// Whether what a waiting `item` came from no longer wants it: its run was cancelled (the routine paused or
/// deleted), or its request went with the last host process.
pub(super) fn withdrawn(hub: &Hub, item: &Item) -> bool {
    match &item.source {
        Source::Bot => false,
        Source::Routine { run, .. } => !hub.routines.waits(run),
        Source::Request { .. } => !hub.tasks.requests.locked().contains_key(&item.id),
    }
}

/// What the dispatcher does with an item it was about to start.
pub(super) enum Begin {
    Go,
    /// Not now: it stays waiting.
    Later,
    /// It left the box: its source no longer wants it.
    Gone,
}

/// A task is about to take `item`: its source is told, and a routine run gets its prompt and its line in the chat.
pub(super) fn begin(hub: &Hub, bot: &str, item: &mut Item) -> Result<Begin> {
    match &item.source {
        Source::Bot => {}
        // The host is stopping: the run stays queued for the next start.
        Source::Routine { .. } if hub.routines.stopping() => return Ok(Begin::Later),
        Source::Routine { run, .. } => {
            let Some(prepared) = hub.routines.begin(hub, bot, run)? else { return Ok(Begin::Gone) };
            if let Err(error) = hub.routines.mark_started(&hub.store, run) {
                tracing::warn!(run, error = format!("{error:#}"), "couldn't mark a routine run started");
            }
            item.brief = prepared.prompt;
        }
        Source::Request { .. } => match hub.tasks.requests.locked().get_mut(&item.id) {
            Some(request) => request.mark_started(),
            None => return Ok(Begin::Gone),
        },
    }
    Ok(Begin::Go)
}

/// `task` took a routine run: when the routine's time limit passes it is stopped and the run fails, as a run in
/// the chat did.
pub(super) fn watch(hub: &Arc<Hub>, bot: &str, task: &str, item: &Item) {
    let Source::Routine { run, .. } = &item.source else { return };
    // Unit tests drive the dispatcher without a runtime.
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let (limit, weak, bot, task) = (hub.routines.timeout(run), Arc::downgrade(hub), bot.to_owned(), task.to_owned());
    let its_item = item.id.clone();
    runtime.spawn(async move {
        tokio::time::sleep(limit).await;
        let Some(hub) = weak.upgrade() else { return };
        match items::change(&hub, &bot, |items| Ok(time_out(items, &its_item, &task))) {
            Ok(Some(item)) => {
                ended(&hub, &bot, &item);
                // Its turn's end clears the task away.
                let _ = hub.send_cmd(&task, Cmd::Stop);
                dispatch(&hub, &bot);
            }
            // It ended in time.
            Ok(None) => {}
            Err(error) => tracing::error!(task, error = format!("{error:#}"), "couldn't end a run past its limit"),
        }
    });
}

/// Ends the routine run `item`, which `task` took, as past its time limit. None when it ended in time: the task
/// may be on an item that followed the run by now, which the limit isn't for.
fn time_out(items: &mut [Item], item: &str, task: &str) -> Option<Item> {
    let works = items.iter().any(|i| i.id == item && i.status == Status::Working);
    let timed_out = super::Outcome::Failed(Failure::TimedOut, routines::TIMED_OUT.into());
    works.then(|| end(items, task, timed_out)).flatten()
}

/// Task `cfg` resumes the turn a host restart cut off: a routine run's time limit counts again from here.
pub(super) fn resumed(hub: &Arc<Hub>, cfg: &BotConfig) {
    if let (Some(bot), Some(item)) = (cfg.task_of.as_deref(), super::item_of(&hub.store, cfg)) {
        watch(hub, bot, &cfg.id, &item);
    }
}

/// Sends what `item` (ended, or stopped while it waited) came to where its source reads it. Whether it arrived:
/// if not, its report stays for the next host start.
pub(super) fn deliver(hub: &Hub, bot: &str, item: &Item) -> bool {
    match &item.source {
        Source::Bot => true,
        Source::Routine { run, .. } => routine_result(hub, bot, item, run)
            .inspect_err(|error| tracing::error!(run, error = format!("{error:#}"), "couldn't save a run's result"))
            .is_ok(),
        Source::Request { .. } => {
            let request = hub.tasks.requests.locked().remove(&item.id);
            if let Some(request) = request {
                answer(request, item);
            }
            true
        }
    }
}

/// A routine run's end as its routine records it: the reply goes in the chat in the turn the run's line took, and
/// the run's outcome is saved, which alerts, retries or offers a retry as for a run in the chat.
fn routine_result(hub: &Hub, bot: &str, item: &Item, run: &str) -> Result<()> {
    use routines::Status as Run;
    let outcome = item.report.as_ref().map(|report| &report.outcome);
    let (status, detail, text) = match (item.status, item.failure, outcome) {
        (Status::Stopped, ..) | (.., None) => (Run::Cancelled, Some(routines::STOPPED_BY_USER.to_owned()), None),
        (_, _, Some(Ok(text))) => (Run::Succeeded, None, Some(text.clone()).filter(|text| !text.is_empty())),
        (_, Some(Failure::TimedOut), Some(Err(_))) => (Run::Failed, Some(routines::TIMED_OUT.to_owned()), None),
        (_, Some(Failure::Restart | Failure::Lost), Some(Err(why))) => (Run::Interrupted, Some(why.clone()), None),
        (_, Some(Failure::Start), Some(Err(why))) => {
            (Run::Failed, Some(routines::could_not_start(why.strip_prefix(NOT_STARTED).unwrap_or(why))), None)
        }
        (_, _, Some(Err(why))) => (Run::Failed, Some(routines::did_not_finish(why)), None),
    };
    if let Some(text) = &text {
        let line = hub.store.entry(&routines::notice_id(run));
        let turn = line.map_or_else(|| hub.store.max_turn(bot), |line| line.turn);
        let data = json!({"text": text, "final": true, "author": bot});
        // Once: a host restart between this and the run's saved outcome sends the result again.
        hub.add_entry_once(&routines::reply_id(run), &Lane::main(bot), EntryKind::Agent, turn, &data)?;
    }
    let ran = routines::Ran { timed_out: item.failure == Some(Failure::TimedOut), task: item.task.as_deref() };
    hub.routines.complete(hub, run, status, detail, text, &ran)
}

/// Gives the asking bot its reply: the item's last message, or why there is none.
fn answer(request: BotRequest, item: &Item) {
    let outcome = item.report.as_ref().map(|report| &report.outcome);
    match (item.status, outcome) {
        // Never started: dropping it says it was cancelled before it ran.
        (Status::Stopped, _) | (_, None) if item.task.is_none() => drop(request),
        (Status::Stopped, _) | (_, None) => request.complete(Err(no_reply::stopped()), true),
        (_, Some(Ok(text))) => request.complete(Ok(text.clone()), false),
        (_, Some(Err(_))) if item.failure == Some(Failure::NoReport) => {
            request.complete(Err(no_reply::no_text()), false);
        }
        (_, Some(Err(why))) if item.failure == Some(Failure::Start) => {
            request.complete(Err(no_reply::not_started(why.strip_prefix(NOT_STARTED).unwrap_or(why))), false);
        }
        (_, Some(Err(why))) => request.complete(Err(no_reply::did_not_finish(why)), false),
    }
}

/// `items` leave their box unanswered (their bot was deleted): the bots that asked hear it was cancelled.
pub(super) fn forget_requests(hub: &Hub, items: &[Item]) {
    let mut requests = hub.tasks.requests.locked();
    let dropped: Vec<BotRequest> = items.iter().filter_map(|item| requests.remove(&item.id)).collect();
    drop(requests);
    drop(dropped);
}

/// Try again on an ended `item` that isn't the bot's own: a routine runs again, as a new run and a new item; a
/// request can't, since the bot that asked already has its answer.
pub(super) fn run_again(hub: &Arc<Hub>, bot: &str, item: &Item) -> Result<()> {
    match &item.source {
        Source::Bot => Ok(()),
        Source::Routine { id, run } => {
            let again = hub.routines.run_again(hub, bot, id, run)?;
            items::change(hub, bot, |items| {
                items.retain(|i| i.id != item.id);
                Ok(())
            })?;
            hub.routines.put_in_box(hub, &again)
        }
        Source::Request { .. } => Err(anyhow!("another bot's request can't be tried again: that bot asks again")),
    }
}

/// At host start, once every box is picked up: a run that the box no longer holds is told so. One that waited is
/// queued again; one that ran is interrupted, which retries it or offers a retry as its routine's rules say.
pub(super) fn settle_runs(hub: &Arc<Hub>) {
    for (bot, run, waits) in hub.routines.boxed_runs() {
        let items = items::load(&hub.store, &bot);
        let of_run = |item: &&Item| matches!(&item.source, Source::Routine { run: r, .. } if *r == run);
        let held = match items.iter().find(of_run) {
            Some(item) if waits => item.status == Status::Waiting,
            Some(item) => item.status == Status::Working,
            None => false,
        };
        if held {
            continue;
        }
        let settled = if waits {
            hub.routines.unbox(&hub.store, &run)
        } else {
            let ran = routines::Ran { timed_out: false, task: None };
            hub.routines.complete(hub, &run, routines::Status::Interrupted, Some(super::CUT_OFF.into()), None, &ran)
        };
        if let Err(error) = settled {
            tracing::error!(run, error = format!("{error:#}"), "couldn't settle a routine run the box lost");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn a_rows_title_is_the_first_line_cut_to_a_titles_length() {
        assert_eq!(title("\n  Review the diff \nand report"), "Review the diff");
        assert_eq!(title(&"x".repeat(200)).chars().count(), MAX_TITLE);
        assert_eq!(title("  \n"), "Request");
    }

    #[test]
    fn a_runs_time_limit_ends_the_run_and_never_the_item_that_followed_it() {
        let item = |id: &str, status: &str, source: serde_json::Value| -> Item {
            serde_json::from_value(json!({"id": id, "title": id, "brief": "x", "status": status, "order": 1,
                "createdAt": 0, "task": "t1", "source": source}))
            .expect("an item")
        };
        let run = json!({"kind": "routine", "id": "r", "run": "run"});
        let mut late = [item("a", "working", run.clone())];
        let ended = time_out(&mut late, "a", "t1").expect("the run ends");
        assert_eq!((ended.status, ended.failure), (Status::Failed, Some(Failure::TimedOut)));

        // The run finished early and its task went on to an item that follows it.
        let mut early = [item("a", "done", run), item("b", "working", json!({"kind": "bot"}))];
        assert!(time_out(&mut early, "a", "t1").is_none());
        assert_eq!(early[1].status, Status::Working);
    }

    #[test]
    fn a_bot_with_the_computer_or_the_team_keeps_runs_and_requests_in_its_chat() {
        let store = Store::open(std::path::Path::new(":memory:")).expect("a store");
        let plain = crate::chat::calls::context::tests::bot("b1", "Mika", "/tmp");
        assert!(boxed(&plain));
        assert!(!boxed(&BotConfig { computer: true, ..plain.clone() }));
        assert!(!boxed(&BotConfig { manage_team: true, ..plain.clone() }));
        assert!(!boxed(&BotConfig { task_of: Some("b1".into()), ..plain.clone() }));
        assert_eq!(source_of(&store, &plain), Source::Bot);
    }
}
