//! Tasks that come from elsewhere than the bot's own `add_task`: a run of one of its routines, and another bot's
//! `message_bot` request. They wait and start like any task, so the chat stays free and they count under
//! `maxThreads`. Their result takes no turn of the bot's: a run's last message is the routine's message in the chat,
//! as a run in the chat posted it, and a request's is the reply the asking bot gets.

use super::tasks::{self, FolderKind, Setup, Source, Status, Task};
use super::{Failure, MAX_BRIEF, MAX_TITLE, NOT_STARTED, dispatch, end, ended};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::errors::host_error;
use crate::analytics::{self, Event};
use crate::chat::team::{BotRequest, no_reply};
use crate::hub::Hub;
use crate::routines;
use crate::store::{BotConfig, EntryKind, Lane, Store};
use anyhow::{Context as _, Result, anyhow, bail, ensure};
use codync_network::Access;
use serde_json::json;
use std::sync::Arc;

/// Whether `cfg`'s routine runs and the requests other bots send it are tasks in its box. A bot that uses the
/// computer or manages the team keeps them as turns in its chat: a thread has neither (one screen and one team stay
/// with the chat), so a run or a request that needs them would stop working.
pub fn boxed(cfg: &BotConfig) -> bool {
    !(cfg.is_thread() || cfg.is_group() || cfg.computer || cfg.manage_team)
}

/// Where the task thread `cfg` works on came from; the bot's own for anything that isn't a thread.
pub fn source_of(store: &Store, cfg: &BotConfig) -> Source {
    if !cfg.is_thread() {
        return Source::Bot;
    }
    super::task_of(store, cfg).map(|task| task.source).unwrap_or_default()
}

/// The ceiling a network rule holds thread `cfg`'s turns to: its request's, on another computer's request (`Allow`
/// for anything else). A request the box no longer holds is held to Ask, so it never works more freely than it got in.
pub fn access_of(hub: &Hub, cfg: &BotConfig) -> Access {
    match super::task_of(&hub.store, cfg) {
        Some(task) if matches!(task.source, Source::Request { .. }) => {
            hub.work.requests.locked().get(&task.id).map_or(Access::Ask, BotRequest::access)
        }
        // An outside app's thread works at the level it was given, kept on its task across restarts.
        Some(task) => task.started_by.map_or(Access::Allow, |by| by.level.ceiling()),
        None => Access::Allow,
    }
}

/// `work_added` for a task that came from `source`.
fn added(hub: &Hub, source: &Source) {
    let properties = json!({"outcome": "added", "after": false, "source": source.kind()});
    analytics::capture(hub, Event::WorkAdded, properties.as_object().cloned().unwrap_or_default());
}

/// Puts `task` last in `bot`'s box.
fn put(hub: &Hub, bot: &str, mut task: Task) -> Result<()> {
    tasks::change(hub, bot, |tasks| {
        task.order = tasks::last(tasks);
        tasks.push(task);
        Ok(())
    })
}

/// A run of `bot`'s routine `routine` (named `name`) waits in its box. Its prompt is written when it starts, from
/// the routine as it is then. A run already there isn't added twice.
pub fn add_routine(hub: &Arc<Hub>, bot: &str, routine: &str, run: &str, name: &str) -> Result<()> {
    let known = |task: &Task| matches!(&task.source, Source::Routine { run: r, .. } if r == run);
    if !tasks::load(&hub.store, bot).iter().any(known) {
        let source = Source::Routine { id: routine.to_owned(), run: run.to_owned() };
        added(hub, &source);
        put(hub, bot, Task { source, ..Task::waiting(&title(name), "", None, None, 0) })?;
    }
    dispatch(hub, bot);
    Ok(())
}

/// A thread the user starts with `bot` themselves (`startThread`): `text`, their first message, waits in its box as
/// a task of its own, named by its first line, and starts when a place is free. `setup`: what its thread runs with
/// in place of the bot's own, checked as a bot's setup is. Answers the task's id.
pub fn add_thread(hub: &Arc<Hub>, bot: &str, text: &str, uploads: &[String], setup: Setup) -> Result<String> {
    add_thread_by(hub, bot, text, uploads, setup, None)
}

/// [`add_thread`], for the outside app `by` when one starts it (`external`).
pub(super) fn add_thread_by(
    hub: &Arc<Hub>,
    bot: &str,
    text: &str,
    uploads: &[String],
    mut setup: Setup,
    by: Option<&super::external::StartedBy>,
) -> Result<String> {
    let changes =
        (setup.model.is_some(), setup.approval.is_some(), setup.workflow.is_some(), setup.login_profile.is_some());
    let place = setup.place;
    let cfg = hub.store.bot(bot)?.filter(|row| !row.deleted).map(|row| row.config);
    let plain = cfg.as_ref().is_some_and(|cfg| cfg.plain);
    let outcome = |outcome: &str, folder: Option<FolderKind>| {
        let properties = json!({
            "outcome": outcome, "after": false, "source": Source::User.kind(),
            "folder": folder, "model": changes.0, "approval": changes.1, "workflow": changes.2, "login": changes.3, "bot": !plain,
            "files": !uploads.is_empty(), "place": place, "mcp": by.is_some(),
        });
        analytics::capture(hub, Event::WorkAdded, properties.as_object().cloned().unwrap_or_default());
    };
    let text = text.trim();
    // A workflow's chat works in that workflow's own folder unless the user picked another.
    if let Some(workflow) = setup.workflow.as_deref().filter(|_| setup.cwd.is_none()) {
        uuid::Uuid::parse_str(workflow).context("a workflow's id")?;
        ensure!(crate::workflows::exists(&hub.store, workflow)?, "unknown workflow");
        let dir = crate::agent::workspace::workflow_path(workflow);
        std::fs::create_dir_all(&dir).context("creating the workflow's folder")?;
        setup.cwd = Some(dir.to_string_lossy().into_owned());
    }
    let checked = (|| {
        let Some(cfg) = cfg.as_ref().filter(|cfg| !cfg.is_thread() && !cfg.is_group()) else { bail!("unknown bot") };
        ensure!(
            (!text.is_empty() || !uploads.is_empty()) && text.len() <= MAX_BRIEF,
            "a thread starts with a message of at most {MAX_BRIEF} bytes"
        );
        ensure!(!cfg.plain || setup.backend.is_some(), "a thread without a bot names its agent");
        let full = |cwd: &str| std::path::Path::new(cwd).is_absolute();
        ensure!(setup.cwd.as_deref().is_none_or(full), "a thread's folder is a full path");
        if let Some(workflow) = &setup.workflow {
            ensure!(crate::workflows::exists(&hub.store, workflow)?, "unknown workflow");
        }
        // As its thread will run it (`dispatch`): in Advisor mode on the threads' agent.
        let run = setup.over(&crate::agent::fallback::threads(cfg));
        let login = crate::agent::login_profiles::of_bot(&hub.store, &run.backend, &run);
        ensure!(run.login_profile.as_deref() == login, "unknown login profile");
        crate::hub::validate(&run)?;
        // Its files, as they were uploaded to the bot (`upload`).
        crate::chat::uploads::resolve_all(&crate::chat::uploads::root(cfg), uploads)
    })();
    let files: Vec<serde_json::Value> = match checked {
        Ok(files) => files,
        Err(error) => {
            outcome("invalid", None);
            return Err(error);
        }
    };
    // Files alone are named by the first of them.
    let named =
        title(if text.is_empty() { files.first().and_then(|f| f["name"].as_str()).unwrap_or_default() } else { text });
    let mut task = Task {
        source: Source::User,
        setup,
        files,
        started_by: by.cloned(),
        ..Task::waiting(&named, text, None, None, 0)
    };
    let id = task.id.clone();
    // A thread without a bot works in a folder of its own unless the user picked one.
    if plain && task.setup.cwd.is_none() {
        let dir = super::plain::own_folder(&id)?;
        std::fs::create_dir_all(&dir).context("creating the thread's own folder")?;
        task.setup.cwd = Some(dir.to_string_lossy().into_owned());
    }
    let folder = match cfg
        .as_ref()
        .map(|cfg| task.existing_folder(cfg).map(|folder| FolderKind::of(cfg, &folder)))
        .transpose()
    {
        Ok(folder) => folder,
        Err(error) => {
            outcome("invalid", None);
            return Err(error);
        }
    };
    if let Err(error) = put(hub, bot, task.clone()) {
        super::plain::forget_folders(std::slice::from_ref(&task));
        return Err(error);
    }
    outcome("added", folder);
    dispatch(hub, bot);
    Ok(id)
}

/// Another bot's request waits in `bot`'s box. One that can't be saved is dropped, which tells the asking bot it
/// was cancelled.
pub fn add_request(hub: &Arc<Hub>, bot: &str, request: BotRequest) {
    let source = Source::Request { id: request.sender().map(str::to_owned) };
    let task = Task { source, ..Task::waiting(&title(request.quote()), &request.prompt, None, None, 0) };
    let (id, source) = (task.id.clone(), task.source.clone());
    hub.work.requests.locked().insert(id.clone(), request);
    if let Err(error) = put(hub, bot, task) {
        host_error!(hub, "couldn't put a bot's request in the box", bot, error = format!("{error:#}"));
        hub.work.requests.locked().remove(&id);
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

/// Whether what a waiting `task` came from no longer wants it: its run was cancelled (the routine paused or
/// deleted), or its request went with the last host process.
pub(super) fn withdrawn(hub: &Hub, task: &Task) -> bool {
    match &task.source {
        Source::Bot | Source::User => false,
        Source::Routine { run, .. } => !hub.routines.waits(run),
        Source::Request { .. } => !hub.work.requests.locked().contains_key(&task.id),
    }
}

/// What the dispatcher does with a task it was about to start.
pub(super) enum Begin {
    Go,
    /// Not now: it stays waiting.
    Later,
    /// It left the box: its source no longer wants it.
    Gone,
}

/// Whether waiting `task` can't start now, whatever seats are free: the host is stopping (a routine run stays queued
/// for the next start), or a run of its routine is under way.
pub(super) fn held(hub: &Hub, task: &Task) -> bool {
    matches!(&task.source, Source::Routine { run, .. } if hub.routines.stopping() || hub.routines.waits_its_turn(run))
}

/// A thread is about to take `task`: its source is told, and a routine run gets its prompt and its line in the chat.
pub(super) fn begin(hub: &Hub, bot: &str, task: &mut Task) -> Result<Begin> {
    match &task.source {
        Source::Bot | Source::User => {}
        // The host is stopping: the run stays queued for the next start. Or a run of its routine has started.
        Source::Routine { .. } if held(hub, task) => {
            return Ok(Begin::Later);
        }
        Source::Routine { run, .. } => {
            let Some(prepared) = hub.routines.begin(hub, bot, run)? else { return Ok(Begin::Gone) };
            // Its folder went since it was saved: the run fails and says so, rather than work somewhere else.
            if let Some(folder) = prepared.folder.as_deref().filter(|f| !std::path::Path::new(f).is_dir()) {
                let detail = routines::could_not_start(format!("its folder {folder} is gone"));
                let ran = routines::Ran { timed_out: false, thread: None };
                hub.routines.complete(hub, run, routines::Status::Failed, Some(detail), None, &ran)?;
                return Ok(Begin::Gone);
            }
            if let Err(error) = hub.routines.mark_started(&hub.store, run) {
                tracing::warn!(run, error = format!("{error:#}"), "couldn't mark a routine run started");
            }
            task.brief = prepared.prompt;
            task.setup.cwd = prepared.folder;
        }
        Source::Request { .. } => match hub.work.requests.locked().get_mut(&task.id) {
            Some(request) => request.mark_started(),
            None => return Ok(Begin::Gone),
        },
    }
    Ok(Begin::Go)
}

/// `thread` took a routine run: when the routine's time limit passes it is stopped and the run fails, as a run in
/// the chat did.
pub(super) fn watch(hub: &Arc<Hub>, bot: &str, thread: &str, task: &Task) {
    let Source::Routine { run, .. } = &task.source else { return };
    // Unit tests drive the dispatcher without a runtime.
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let (limit, weak, bot, thread) =
        (hub.routines.timeout(run), Arc::downgrade(hub), bot.to_owned(), thread.to_owned());
    let its_task = task.id.clone();
    runtime.spawn(async move {
        tokio::time::sleep(limit).await;
        let Some(hub) = weak.upgrade() else { return };
        match tasks::change(&hub, &bot, |tasks| Ok(time_out(tasks, &its_task, &thread))) {
            Ok(Some(task)) => {
                ended(&hub, &bot, &task);
                // Its turn's end clears the thread away.
                let _ = hub.send_cmd(&thread, Cmd::Stop);
                dispatch(&hub, &bot);
            }
            // It ended in time.
            Ok(None) => {}
            Err(error) => host_error!(&hub, "couldn't end a run past its limit", thread, error = format!("{error:#}")),
        }
    });
}

/// Ends the routine run `task`, which `thread` took, as past its time limit. None when it ended in time: the thread
/// may be on a task that followed the run by now, which the limit isn't for.
fn time_out(tasks: &mut [Task], task: &str, thread: &str) -> Option<Task> {
    let works = tasks.iter().any(|i| i.id == task && i.status == Status::Working);
    let timed_out = super::Outcome::Failed(Failure::TimedOut, routines::TIMED_OUT.into());
    works.then(|| end(tasks, thread, timed_out)).flatten()
}

/// Thread `cfg` resumes the turn a host restart cut off: a routine run's time limit counts again from here.
pub(super) fn resumed(hub: &Arc<Hub>, cfg: &BotConfig) {
    if let (Some(bot), Some(task)) = (cfg.thread_of.as_deref(), super::task_of(&hub.store, cfg)) {
        watch(hub, bot, &cfg.id, &task);
    }
}

/// Sends what `task` (ended, or stopped while it waited) came to where its source reads it. Whether it arrived:
/// if not, its report stays for the next host start.
pub(super) fn deliver(hub: &Hub, bot: &str, task: &Task) -> bool {
    match &task.source {
        Source::Bot | Source::User => true,
        Source::Routine { run, .. } => routine_result(hub, bot, task, run)
            .inspect_err(|error| host_error!(hub, "couldn't save a run's result", run, error = format!("{error:#}")))
            .is_ok(),
        Source::Request { .. } => {
            let request = hub.work.requests.locked().remove(&task.id);
            if let Some(request) = request {
                answer(request, task);
            }
            true
        }
    }
}

/// A routine run's end as its routine records it: the reply goes in the chat in the turn the run's line took, and
/// the run's outcome is saved, which alerts, retries or offers a retry as for a run in the chat.
fn routine_result(hub: &Hub, bot: &str, task: &Task, run: &str) -> Result<()> {
    use routines::Status as Run;
    let outcome = task.report.as_ref().map(|report| &report.outcome);
    let (status, detail, text) = match (task.status, task.failure, outcome) {
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
    let ran = routines::Ran { timed_out: task.failure == Some(Failure::TimedOut), thread: task.thread.as_deref() };
    hub.routines.complete(hub, run, status, detail, text, &ran)
}

/// Gives the asking bot its reply: the task's last message, or why there is none.
fn answer(request: BotRequest, task: &Task) {
    let outcome = task.report.as_ref().map(|report| &report.outcome);
    match (task.status, outcome) {
        // Never started: dropping it says it was cancelled before it ran.
        (Status::Stopped, _) | (_, None) if task.thread.is_none() => drop(request),
        (Status::Stopped, _) | (_, None) => request.complete(Err(no_reply::stopped()), true),
        (_, Some(Ok(text))) => request.complete(Ok(text.clone()), false),
        (_, Some(Err(_))) if task.failure == Some(Failure::NoReport) => {
            request.complete(Err(no_reply::no_text()), false);
        }
        (_, Some(Err(why))) if task.failure == Some(Failure::Start) => {
            request.complete(Err(no_reply::not_started(why.strip_prefix(NOT_STARTED).unwrap_or(why))), false);
        }
        (_, Some(Err(why))) => request.complete(Err(no_reply::did_not_finish(why)), false),
    }
}

/// `tasks` leave their box unanswered (their bot was deleted): the bots that asked hear it was cancelled.
pub(super) fn forget_requests(hub: &Hub, tasks: &[Task]) {
    let mut requests = hub.work.requests.locked();
    let dropped: Vec<BotRequest> = tasks.iter().filter_map(|task| requests.remove(&task.id)).collect();
    drop(requests);
    drop(dropped);
}

/// Try again on an ended `task` that isn't the bot's own: a routine runs again, as a new run and a new task; a
/// request can't, since the bot that asked already has its answer.
pub(super) fn run_again(hub: &Arc<Hub>, bot: &str, task: &Task) -> Result<()> {
    match &task.source {
        Source::Bot | Source::User => Ok(()),
        Source::Routine { id, run } => {
            let again = hub.routines.run_again(hub, bot, id, run)?;
            tasks::change(hub, bot, |tasks| {
                tasks.retain(|i| i.id != task.id);
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
        let tasks = tasks::load(&hub.store, &bot);
        let of_run = |task: &&Task| matches!(&task.source, Source::Routine { run: r, .. } if *r == run);
        let held = match tasks.iter().find(of_run) {
            Some(task) if waits => task.status == Status::Waiting,
            Some(task) => task.status == Status::Working,
            None => false,
        };
        if held {
            continue;
        }
        let settled = if waits {
            hub.routines.unbox(&hub.store, &run)
        } else {
            let ran = routines::Ran { timed_out: false, thread: None };
            hub.routines.complete(hub, &run, routines::Status::Interrupted, Some(super::CUT_OFF.into()), None, &ran)
        };
        if let Err(error) = settled {
            host_error!(hub, "couldn't settle a routine run the box lost", run, error = format!("{error:#}"));
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
    fn a_runs_time_limit_ends_the_run_and_never_the_task_that_followed_it() {
        let task = |id: &str, status: &str, source: serde_json::Value| -> Task {
            serde_json::from_value(json!({"id": id, "title": id, "brief": "x", "status": status, "order": 1,
                "createdAt": 0, "thread": "t1", "source": source}))
            .expect("a task")
        };
        let run = json!({"kind": "routine", "id": "r", "run": "run"});
        let mut late = [task("a", "working", run.clone())];
        let ended = time_out(&mut late, "a", "t1").expect("the run ends");
        assert_eq!((ended.status, ended.failure), (Status::Failed, Some(Failure::TimedOut)));

        // The run finished early and its thread went on to a task that follows it.
        let mut early = [task("a", "done", run), task("b", "working", json!({"kind": "bot"}))];
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
        assert!(!boxed(&BotConfig { thread_of: Some("b1".into()), ..plain.clone() }));
        assert_eq!(source_of(&store, &plain), Source::Bot);
    }
}
