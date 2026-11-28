//! A bot's event box as the host sends it (`docs/features/work.md`): its tasks, how they fall into the
//! Details page's sections, and the lines that say what they do.

use serde::Deserialize;
use serde_json::Value;

use super::super::l10n::{fill, tr};
use super::model::{Entry, Kind};
pub use crate::chat::work::Status as WorkStatus;
use crate::chat::work::Wait;
use crate::store::DEFAULT_THREADS;
pub use crate::store::MAX_THREADS;

/// The status in a word (a catalog phrase).
pub fn word(status: WorkStatus) -> &'static str {
    match status {
        WorkStatus::Waiting => "Waiting",
        WorkStatus::Working => "Working",
        WorkStatus::NeedsYou => "Needs you",
        WorkStatus::Done => "Done",
        WorkStatus::Failed => "Failed",
        WorkStatus::Stopped => "Stopped",
    }
}

/// Where a task came from when the bot didn't add it itself.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Source {
    /// A run of one of the bot's routines.
    Routine,
    /// Another bot asked for it: that bot's id, when it is one of this computer's.
    Request(Option<String>),
}

/// One piece of work in a bot's box (`bot.work`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WorkTask {
    pub id: String,
    pub title: String,
    pub status: WorkStatus,
    /// What its thread is doing now; empty until it says.
    pub activity: String,
    /// The thread that has or had it: whose transcript its row opens.
    pub thread: Option<String>,
    /// Why it waits; none on a task that doesn't, or for a reason this terminal doesn't know.
    pub wait: Option<Wait>,
    /// It gave way to another task once, and starts again from its brief.
    pub cut_in: bool,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// What its thread reported, or why it failed.
    pub result: String,
    /// None for one the bot added, or from a source this terminal doesn't know.
    pub source: Option<Source>,
    /// The branch of the git worktree its thread works in, while there is one.
    pub branch: Option<String>,
    /// Where its thread works; none before a thread took it, and from a host that doesn't say.
    pub folder: Option<Folder>,
    /// The user may write to its thread: the bot's own work, whose thread is still there.
    pub talk: bool,
    /// Its thread takes files too; a host from before them doesn't say, and the terminal types a dropped path there.
    pub talk_files: bool,
    /// It takes one of the bot's seats, as the host says (one waiting on the user gives its seat up); none from an
    /// older host, where every live task takes one ([`Self::takes_seat`]).
    pub seat: Option<bool>,
    /// What its thread's agent runs in the background, as the capsule says it, and since when the first piece runs.
    pub background: Option<(String, i64)>,
}

/// A task's `background`, as its capsule says it: what the one piece does, or the names of several; none while nothing
/// runs there.
fn background(v: &Value) -> Option<(String, i64)> {
    let tasks: Vec<&Value> = v.as_array()?.iter().filter(|t| t["id"].is_string()).collect();
    let first = tasks.first()?;
    let words = |key: &str, t: &Value| t[key].as_str().filter(|w| !w.is_empty()).map(str::to_owned);
    let text = if let [one] = tasks.as_slice() {
        [words("name", one), words("activity", one)].into_iter().flatten().collect::<Vec<_>>().join(" · ")
    } else {
        tasks.iter().filter_map(|t| words("name", t)).collect::<Vec<_>>().join(", ")
    };
    Some((text, first["startedAt"].as_i64().unwrap_or_default()))
}

/// Where a task's thread works.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Folder {
    /// The folder Codync made for a thread without a bot (`ownFolder`): shown as "Own folder", not its id.
    Own,
    Path(String),
}

impl WorkTask {
    /// A task with an id and a status this terminal knows; any other is left out.
    pub(in crate::tui) fn parse(v: &Value) -> Option<Self> {
        let text = |key: &str| v[key].as_str().unwrap_or_default().to_owned();
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            title: text("title"),
            status: WorkStatus::deserialize(&v["status"]).ok()?,
            activity: text("activity"),
            thread: v["thread"].as_str().map(str::to_owned),
            wait: Wait::deserialize(&v["wait"]).ok(),
            cut_in: v["cutIn"] == true,
            started_at: v["startedAt"].as_i64(),
            ended_at: v["endedAt"].as_i64(),
            result: text("result"),
            source: match v["source"]["kind"].as_str() {
                Some("routine") => Some(Source::Routine),
                Some("request") => Some(Source::Request(v["source"]["id"].as_str().map(str::to_owned))),
                _ => None,
            },
            branch: v["worktree"]["branch"].as_str().filter(|b| !b.is_empty()).map(str::to_owned),
            folder: v["folder"]
                .as_str()
                .filter(|f| !f.is_empty())
                .map(|f| if v["ownFolder"] == true { Folder::Own } else { Folder::Path(f.to_owned()) }),
            talk: v["talk"] == true,
            talk_files: v["talkFiles"] == true,
            seat: v["seat"].as_bool(),
            background: background(&v["background"]),
        })
    }

    /// It takes one of the bot's seats (`maxThreads`): the host's word, else a live task.
    pub fn takes_seat(&self) -> bool {
        self.seat.unwrap_or_else(|| self.status.live())
    }

    /// A bot's `work`; a host from before the box sends none, and the feature doesn't show.
    pub(in crate::tui) fn list(v: &Value) -> Vec<Self> {
        v.as_array().into_iter().flatten().filter_map(Self::parse).collect()
    }

    /// Why a waiting task waits.
    fn wait_text(&self) -> String {
        if self.cut_in {
            return tr("Was cut in on; starts again").into_owned();
        }
        match self.wait {
            Some(Wait::Next) => tr("Next up").into_owned(),
            Some(Wait::Behind { ahead }) => fill(&tr("{count} ahead"), &[("count", &ahead.to_string())]),
            Some(Wait::After) => tr("Follows what it works on").into_owned(),
            Some(Wait::Usage) => tr("Near the usage limit: one at a time").into_owned(),
            None => tr("Waiting").into_owned(),
        }
    }

    /// What its row says after the title: why it waits, what its thread does, or how it ended (a failed or stopped one
    /// says why).
    pub fn line(&self) -> String {
        match self.status {
            WorkStatus::Waiting => self.wait_text(),
            WorkStatus::Working if !self.activity.is_empty() => tr(&self.activity).into_owned(),
            WorkStatus::Failed | WorkStatus::Stopped if !self.result.is_empty() => self.result.clone(),
            status => tr(word(status)).into_owned(),
        }
    }

    /// Still in the box's hands: a thread has it, or it waits for one.
    pub fn open(&self) -> bool {
        self.status.live() || self.status == WorkStatus::Waiting
    }

    /// Try again is offered on a task that failed or was stopped, unless another bot asked for it: that bot has
    /// its answer already. A routine's task runs the routine again.
    pub fn can_retry(&self) -> bool {
        matches!(self.status, WorkStatus::Failed | WorkStatus::Stopped)
            && !matches!(self.source, Some(Source::Request(_)))
    }

    /// A thread has it and it may give way to a task that cuts in: only work the bot added that holds a seat. A
    /// routine's run or another bot's request is stopped, never set aside, and a thread the user started holds no
    /// seat (`seat` false), so it never gives way.
    pub fn gives_way(&self) -> bool {
        self.takes_seat() && self.source.is_none()
    }
}

/// A bot's `maxThreads`, within 1 to 6; none from a host before the box.
pub(in crate::tui) fn max_threads(v: &Value) -> Option<u8> {
    v.as_u64().map(|n| u8::try_from(n.clamp(1, u64::from(MAX_THREADS))).unwrap_or(MAX_THREADS))
}

/// How many threads a bot with this `maxThreads` may run at once.
pub fn threads_at_once(max_threads: Option<u8>) -> usize {
    usize::from(max_threads.unwrap_or(DEFAULT_THREADS))
}

fn count(work: &[WorkTask], status: WorkStatus) -> usize {
    work.iter().filter(|i| i.status == status).count()
}

/// `2 working · 3 waiting`, a part left out at none; empty with neither.
pub fn counts(work: &[WorkTask]) -> String {
    let part = |phrase: &str, status| {
        let n = count(work, status);
        (n > 0).then(|| fill(&tr(phrase), &[("count", &n.to_string())]))
    };
    let parts = [part("{count} working", WorkStatus::Working), part("{count} waiting", WorkStatus::Waiting)];
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// Every thread is busy, so the task first in line can only start now if a thread gives way, and one may. Not one
/// that follows unfinished work or waits for usage: a freed thread wouldn't start it.
pub fn can_cut_in(work: &[WorkTask], max_threads: Option<u8>) -> bool {
    let first = work.iter().find(|i| i.status == WorkStatus::Waiting);
    first.is_some_and(|first| !matches!(first.wait, Some(Wait::After | Wait::Usage)))
        && work.iter().any(WorkTask::gives_way)
        && work.iter().filter(|i| i.takes_seat()).count() >= threads_at_once(max_threads)
}

/// The Details page's sections of tasks, each in the order its rows show.
pub struct Sections<'a> {
    /// The threads that run now: tasks that need the user, then the working ones.
    pub threads: Vec<&'a WorkTask>,
    /// What waits for a thread, in the order it will be taken.
    pub waiting: Vec<&'a WorkTask>,
    /// Tasks that ended in the last day, newest first.
    pub recent: Vec<&'a WorkTask>,
}

impl<'a> Sections<'a> {
    pub fn of(work: &'a [WorkTask]) -> Self {
        let of = |status| work.iter().filter(move |i| i.status == status);
        Self {
            threads: of(WorkStatus::NeedsYou).chain(of(WorkStatus::Working)).collect(),
            waiting: of(WorkStatus::Waiting).collect(),
            recent: work.iter().filter(|i| i.status.ended()).collect(),
        }
    }
}

/// A bot's live and waiting tasks in one line: what the line over the composer says after `Work`.
#[derive(PartialEq, Eq, Debug)]
pub struct Background {
    pub what: String,
    /// When the one working task began; none when the line counts tasks.
    pub since: Option<i64>,
    pub needs_you: bool,
    /// The one task a thread has, which `X` stops; none with no such task or several.
    pub stops: Option<String>,
}

impl Background {
    /// None while nothing is live or waiting.
    pub fn of(work: &[WorkTask]) -> Option<Self> {
        let live: Vec<&WorkTask> = work.iter().filter(|i| i.status.live()).collect();
        let waiting = count(work, WorkStatus::Waiting);
        if live.is_empty() && waiting == 0 {
            return None;
        }
        let only = if let [only] = live[..] { Some(only) } else { None };
        let stops = only.map(|i| i.id.clone());
        let needs_you = live.iter().any(|i| i.status == WorkStatus::NeedsYou);
        Some(match only {
            _ if needs_you => {
                let parts = [tr("Needs you").into_owned(), counts(work)];
                let what = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");
                Self { what, since: None, needs_you, stops }
            }
            Some(one) if waiting == 0 => {
                let what = if one.activity.is_empty() { one.title.clone() } else { tr(&one.activity).into_owned() };
                Self { what, since: one.started_at, needs_you, stops }
            }
            _ => Self { what: counts(work), since: None, needs_you, stops },
        })
    }
}

/// Whether the chat entry `e`, which a thread put in its bot's chat, is one its thread's page shows (`App::shared`): a
/// card, answered or not, its agent's sign-in card, a file, a voice note, an artifact.
pub fn is_shared(e: &Entry) -> bool {
    e.data["connectionRequest"]["kind"] == "agent"
        || match e.kind {
            Kind::Permission | Kind::Decision => true,
            Kind::Agent => {
                e.data["artifact"]["title"].is_string()
                    || e.data["voiceNote"] == true
                    || e.data["files"].as_array().is_some_and(|f| f.iter().any(|f| f["name"].is_string()))
            }
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A task `id` in `status`, titled `Fix login`.
    fn task(id: &str, status: &str) -> Value {
        json!({"id": id, "title": "Fix login", "status": status, "thread": format!("thread-{id}"), "wait": null})
    }

    fn work(statuses: &[&str]) -> Vec<WorkTask> {
        let tasks = statuses.iter().enumerate().map(|(i, status)| task(&i.to_string(), status)).collect();
        WorkTask::list(&Value::Array(tasks))
    }

    #[test]
    fn work_and_max_threads_parse_and_an_unknown_status_is_left_out() {
        assert!(WorkTask::list(&Value::Null).is_empty(), "a host from before the box");
        let listed = WorkTask::list(&json!([
            {"title": "no id", "status": "working"},
            {"id": "later", "title": "A status of a newer host", "status": "paused"},
            {"id": "a", "title": "Fix login", "status": "needsYou", "activity": "Editing", "thread": "thread-1",
                "wait": null, "cutIn": false, "createdAt": 1, "startedAt": 5, "endedAt": null, "result": ""},
            {"id": "b", "status": "waiting", "thread": null, "wait": {"kind": "behind", "ahead": 2}, "cutIn": true},
            {"id": "c", "status": "failed", "thread": "thread-2", "startedAt": 5, "endedAt": 9, "result": "it could not start"},
        ]));
        assert_eq!(listed.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        let [a, b, c] = &listed[..] else { panic!("three tasks") };
        assert_eq!(
            (a.status, a.thread.as_deref(), a.wait, a.started_at),
            (WorkStatus::NeedsYou, Some("thread-1"), None, Some(5))
        );
        assert_eq!(
            (b.status, b.thread.as_deref(), b.wait, b.cut_in),
            (WorkStatus::Waiting, None, Some(Wait::Behind { ahead: 2 }), true)
        );
        assert_eq!((c.status, c.ended_at, c.result.as_str()), (WorkStatus::Failed, Some(9), "it could not start"));
        assert!(a.status.live() && !b.status.live() && c.status.ended());

        assert_eq!(max_threads(&Value::Null), None);
        assert_eq!(
            (max_threads(&json!(2)), max_threads(&json!(0)), max_threads(&json!(9))),
            (Some(2), Some(1), Some(6))
        );
        assert_eq!((threads_at_once(None), threads_at_once(Some(1))), (3, 1), "no maxThreads from the host counts 3");
        let three = work(&["working", "needsYou", "working", "waiting"]);
        assert!(can_cut_in(&three, None) && !can_cut_in(&three, Some(4)) && can_cut_in(&three[1..], Some(2)));
        assert!(!can_cut_in(&three[..3], None), "nothing waits");
    }

    #[test]
    fn a_source_and_a_worktree_parse_and_set_what_a_task_may_do() {
        let with = |status: &str, more: Value| {
            let mut v = task("a", status);
            v.as_object_mut().expect("an object").extend(more.as_object().cloned().unwrap_or_default());
            WorkTask::parse(&v).expect("a task")
        };
        let source = |source: Value| with("working", json!({"source": source})).source;
        assert_eq!(source(json!({"kind": "routine", "id": "r1"})), Some(Source::Routine));
        assert_eq!(source(json!({"kind": "request", "id": "mika"})), Some(Source::Request(Some("mika".into()))));
        assert_eq!(source(json!({"kind": "request"})), Some(Source::Request(None)), "a bot elsewhere");
        assert_eq!(
            (source(Value::Null), source(json!({"kind": "webhook"}))),
            (None, None),
            "absent or unknown: the bot's own"
        );
        let branch = |worktree: Value| with("working", json!({"worktree": worktree})).branch;
        assert_eq!(branch(json!({"branch": "codync/fix-login", "path": "/w"})).as_deref(), Some("codync/fix-login"));
        assert_eq!(branch(Value::Null), None);

        let routine = json!({"source": {"kind": "routine", "id": "r1"}});
        let request = json!({"source": {"kind": "request", "id": "mika"}});
        let retries = |status: &str, more: &Value| with(status, more.clone()).can_retry();
        assert!(retries("failed", &Value::Null) && retries("stopped", &routine), "a routine's task runs it again");
        assert!(!retries("failed", &request) && !retries("stopped", &request), "the asking bot has its answer");
        assert!(!retries("done", &Value::Null) && !retries("working", &Value::Null));
        let gives = |status: &str, more: &Value| with(status, more.clone()).gives_way();
        assert!(gives("working", &Value::Null) && gives("needsYou", &Value::Null));
        assert!(!gives("working", &routine) && !gives("needsYou", &request) && !gives("waiting", &Value::Null));

        // Cutting in needs every thread busy, a task that gives way, and a first in line a freed thread would start.
        let boxed = |tasks: Vec<Value>| WorkTask::list(&Value::Array(tasks));
        let own = || task("w", "working");
        let run = || {
            let mut v = task("r", "working");
            v["source"] = json!({"kind": "routine", "id": "r1"});
            v
        };
        let waits = |kind: &str| {
            let mut v = task("q", "waiting");
            v["wait"] = json!({"kind": kind});
            v
        };
        assert!(can_cut_in(&boxed(vec![own(), waits("next")]), Some(1)));
        assert!(!can_cut_in(&boxed(vec![run(), waits("next")]), Some(1)), "a routine's run never gives way");
        assert!(can_cut_in(&boxed(vec![run(), own(), waits("next")]), Some(2)));
        assert!(!can_cut_in(&boxed(vec![own(), waits("after")]), Some(1)), "it follows unfinished work");
        assert!(!can_cut_in(&boxed(vec![own(), waits("usage")]), Some(1)), "it waits for usage");
        assert!(!can_cut_in(&boxed(vec![own(), waits("next")]), Some(2)), "a thread is free");
        assert!(boxed(vec![waits("next")])[0].open() && !with("done", Value::Null).open());
    }

    #[test]
    fn a_waiting_task_says_why_it_waits() {
        let waits = |wait: Value, cut_in: bool| {
            let mut v = task("a", "waiting");
            v["wait"] = wait;
            v["cutIn"] = cut_in.into();
            WorkTask::parse(&v).expect("a task").line()
        };
        assert_eq!(waits(json!({"kind": "next"}), false), "Next up");
        assert_eq!(waits(json!({"kind": "behind", "ahead": 3}), false), "3 ahead");
        assert_eq!(waits(json!({"kind": "after"}), false), "Follows what it works on");
        assert_eq!(waits(json!({"kind": "usage"}), false), "Near the usage limit: one at a time");
        assert_eq!(waits(Value::Null, false), "Waiting", "no reason");
        assert_eq!(waits(json!({"kind": "tomorrow"}), false), "Waiting", "a reason of a newer host");
        assert_eq!(waits(json!({"kind": "next"}), true), "Was cut in on; starts again");

        let ended = |status: &str, result: &str| {
            let mut v = task("a", status);
            v["result"] = result.into();
            WorkTask::parse(&v).expect("a task").line()
        };
        assert_eq!(ended("failed", "it could not start"), "it could not start");
        assert_eq!((ended("stopped", ""), ended("done", "All fixed.")), ("Stopped".to_owned(), "Done".to_owned()));
    }

    #[test]
    fn the_line_says_what_the_capsule_says() {
        let line = |statuses: &[&str]| Background::of(&work(statuses)).map(|b| (b.what, b.needs_you));
        assert_eq!(Background::of(&[]), None);
        assert_eq!(line(&["done", "failed", "stopped"]), None, "nothing live or waiting");
        assert_eq!(line(&["working"]), Some(("Fix login".into(), false)), "one working, no step yet: its title");
        assert_eq!(line(&["working", "working"]), Some(("2 working".into(), false)));
        assert_eq!(line(&["working", "waiting"]), Some(("1 working · 1 waiting".into(), false)));
        assert_eq!(
            line(&["working", "working", "waiting", "waiting", "waiting", "done"]),
            Some(("2 working · 3 waiting".into(), false))
        );
        assert_eq!(line(&["waiting", "waiting"]), Some(("2 waiting".into(), false)), "all of them wait");
        assert_eq!(line(&["needsYou"]), Some(("Needs you".into(), true)));
        assert_eq!(line(&["working", "needsYou"]), Some(("Needs you · 1 working".into(), true)));
        assert_eq!(line(&["waiting", "needsYou"]), Some(("Needs you · 1 waiting".into(), true)));
        assert_eq!(
            line(&["working", "needsYou", "waiting", "working"]),
            Some(("Needs you · 2 working · 1 waiting".into(), true))
        );

        let mut one = task("a", "working");
        one["activity"] = "Reading files".into();
        one["startedAt"] = 5.into();
        let alone = Background::of(&WorkTask::list(&json!([one, task("old", "done")]))).expect("one working task");
        assert_eq!((alone.what.as_str(), alone.since, alone.stops.as_deref()), ("Reading files", Some(5), Some("a")));
        let stops = |statuses: &[&str]| Background::of(&work(statuses)).and_then(|b| b.stops);
        assert_eq!(stops(&["waiting", "needsYou"]).as_deref(), Some("1"), "the one task a thread has");
        assert_eq!((stops(&["working", "working"]), stops(&["waiting"])), (None, None), "several, or none");
        assert_eq!(Background::of(&work(&["working", "waiting"])).and_then(|b| b.since), None, "counted: no time");
    }

    #[test]
    fn the_thread_shows_its_cards_sign_in_files_voice_notes_and_artifacts() {
        let shared = |kind: &str, data: Value| {
            let e = Entry::parse(&json!({"id": "c", "botId": "b", "seq": 1, "kind": kind, "turn": 1, "data": data}));
            is_shared(&e.expect("an entry").1)
        };
        assert!(shared("permission", json!({"status": "answered", "title": "npm test"})), "answered or not");
        assert!(shared("notice", json!({"decision": true, "status": "expired", "question": "Squash?"})));
        let sign_in = |kind: &str| {
            let request = json!({"kind": kind, "status": "pending", "title": "Kimi Code"});
            shared("notice", json!({"text": "Needs you to sign in.", "connectionRequest": request}))
        };
        assert!(sign_in("agent"));
        assert!(!sign_in("oauth"), "a connector's card is not a thread's");
        let file = json!({"id": "f", "name": "log.txt", "size": 3});
        assert!(shared("agent", json!({"final": true, "files": [file]})));
        assert!(shared("agent", json!({"final": true, "voiceNote": true, "text": "Hi"})));
        assert!(shared("agent", json!({"final": true, "artifact": {"id": "a", "title": "Plan"}})));
        assert!(!shared("agent", json!({"text": "Just words", "final": true})));
        assert!(!shared("notice", json!({"text": "A notice"})));
    }

    #[test]
    fn the_sections_put_what_needs_you_first_and_keep_the_hosts_order() {
        let listed = work(&["working", "needsYou", "waiting", "done", "waiting", "stopped"]);
        let sections = Sections::of(&listed);
        let ids = |tasks: &[&WorkTask]| tasks.iter().map(|i| i.id.clone()).collect::<Vec<_>>().join(" ");
        assert_eq!(
            (ids(&sections.threads), ids(&sections.waiting), ids(&sections.recent)),
            ("1 0".into(), "2 4".into(), "3 5".into())
        );
    }
}
