//! A bot's event box as the host sends it (`docs/features/background-tasks.md`): its items, how they fall into the
//! Details page's sections, and the lines that say what they do.

use serde_json::Value;

use super::super::l10n::{fill, tr};

/// How many agents a bot runs at once unless the user changed it: what a host that sends no `maxAgents` counts as.
const DEFAULT_AGENTS: u8 = 3;
/// The most agents a bot may run at once.
pub const MAX_AGENTS: u8 = 6;

/// The six words for work, the same on the wire and in every client.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorkStatus {
    Waiting,
    Working,
    NeedsYou,
    Done,
    Failed,
    Stopped,
}

impl WorkStatus {
    /// A task has it: working, or waiting on the user's approval.
    pub fn live(self) -> bool {
        matches!(self, Self::Working | Self::NeedsYou)
    }

    pub fn ended(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Stopped)
    }

    /// The status in a word (a catalog phrase).
    pub fn word(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting",
            Self::Working => "Working",
            Self::NeedsYou => "Needs you",
            Self::Done => "Done",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
        }
    }
}

/// Why an item waits: a place to come free, items ahead of it, the item it follows, or the agent's usage limit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wait {
    Next,
    Behind(u64),
    After,
    Usage,
}

/// One piece of work in a bot's box (`bot.work`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub status: WorkStatus,
    /// What its task is doing now; empty until it says.
    pub activity: String,
    /// The task that has or had it: whose transcript its row opens.
    pub task: Option<String>,
    /// Why it waits; none on an item that doesn't, or for a reason this terminal doesn't know.
    pub wait: Option<Wait>,
    /// It gave way to another item once, and starts again from its brief.
    pub cut_in: bool,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// What its task reported, or why it failed.
    pub result: String,
}

impl WorkItem {
    /// An item with an id and a status this terminal knows; any other is left out.
    fn parse(v: &Value) -> Option<Self> {
        let text = |key: &str| v[key].as_str().unwrap_or_default().to_owned();
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            title: text("title"),
            status: match v["status"].as_str()? {
                "waiting" => WorkStatus::Waiting,
                "working" => WorkStatus::Working,
                "needsYou" => WorkStatus::NeedsYou,
                "done" => WorkStatus::Done,
                "failed" => WorkStatus::Failed,
                "stopped" => WorkStatus::Stopped,
                _ => return None,
            },
            activity: text("activity"),
            task: v["task"].as_str().map(str::to_owned),
            wait: match v["wait"]["kind"].as_str() {
                Some("next") => Some(Wait::Next),
                Some("behind") => Some(Wait::Behind(v["wait"]["ahead"].as_u64().unwrap_or(1))),
                Some("after") => Some(Wait::After),
                Some("usage") => Some(Wait::Usage),
                _ => None,
            },
            cut_in: v["cutIn"] == true,
            started_at: v["startedAt"].as_i64(),
            ended_at: v["endedAt"].as_i64(),
            result: text("result"),
        })
    }

    /// A bot's `work`; a host from before the box sends none, and the feature doesn't show.
    pub(in crate::tui) fn list(v: &Value) -> Vec<Self> {
        v.as_array().into_iter().flatten().filter_map(Self::parse).collect()
    }

    /// Why a waiting item waits.
    fn wait_text(&self) -> String {
        if self.cut_in {
            return tr("Was cut in on; starts again").into_owned();
        }
        match self.wait {
            Some(Wait::Next) => tr("Next up").into_owned(),
            Some(Wait::Behind(ahead)) => fill(&tr("{count} ahead"), &[("count", &ahead.to_string())]),
            Some(Wait::After) => tr("Follows what it works on").into_owned(),
            Some(Wait::Usage) => tr("Waiting for usage").into_owned(),
            None => tr("Waiting").into_owned(),
        }
    }

    /// What its row says after the title: why it waits, what its task does, or how it ended (a failed or stopped one
    /// says why).
    pub fn line(&self) -> String {
        match self.status {
            WorkStatus::Waiting => self.wait_text(),
            WorkStatus::Working if !self.activity.is_empty() => tr(&self.activity).into_owned(),
            WorkStatus::Failed | WorkStatus::Stopped if !self.result.is_empty() => self.result.clone(),
            status => tr(status.word()).into_owned(),
        }
    }
}

/// A bot's `maxAgents`, within 1 to 6; none from a host before the box.
pub(in crate::tui) fn max_agents(v: &Value) -> Option<u8> {
    v.as_u64().map(|n| u8::try_from(n.clamp(1, u64::from(MAX_AGENTS))).unwrap_or(MAX_AGENTS))
}

/// How many agents a bot with this `maxAgents` may run at once.
pub fn places(max_agents: Option<u8>) -> usize {
    usize::from(max_agents.unwrap_or(DEFAULT_AGENTS))
}

fn count(work: &[WorkItem], status: WorkStatus) -> usize {
    work.iter().filter(|i| i.status == status).count()
}

/// `2 working · 3 waiting`, a part left out at none; empty with neither.
pub fn counts(work: &[WorkItem]) -> String {
    let part = |phrase: &str, status| {
        let n = count(work, status);
        (n > 0).then(|| fill(&tr(phrase), &[("count", &n.to_string())]))
    };
    let parts = [part("{count} working", WorkStatus::Working), part("{count} waiting", WorkStatus::Waiting)];
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// Every place is taken, so the item first in line can only start now if a task gives way.
pub fn can_cut_in(work: &[WorkItem], max_agents: Option<u8>) -> bool {
    work.iter().filter(|i| i.status.live()).count() >= places(max_agents)
}

/// The Details page's sections of items, each in the order its rows show.
pub struct Sections<'a> {
    /// The agent sessions that run now: items that need the user, then the working ones.
    pub threads: Vec<&'a WorkItem>,
    /// What waits for a thread, in the order it will be taken.
    pub waiting: Vec<&'a WorkItem>,
    /// Items that ended in the last day, newest first.
    pub recent: Vec<&'a WorkItem>,
}

impl<'a> Sections<'a> {
    pub fn of(work: &'a [WorkItem]) -> Self {
        let of = |status| work.iter().filter(move |i| i.status == status);
        Self {
            threads: of(WorkStatus::NeedsYou).chain(of(WorkStatus::Working)).collect(),
            waiting: of(WorkStatus::Waiting).collect(),
            recent: work.iter().filter(|i| i.status.ended()).collect(),
        }
    }
}

/// A bot's live and waiting items in one line: what the line over the composer says after `In the background`.
#[derive(PartialEq, Eq, Debug)]
pub struct Background {
    pub what: String,
    /// When the one working item began; none when the line counts items.
    pub since: Option<i64>,
    pub needs_you: bool,
    /// The one item a task has, which `X` stops; none with no such item or several.
    pub stops: Option<String>,
}

impl Background {
    /// None while nothing is live or waiting.
    pub fn of(work: &[WorkItem]) -> Option<Self> {
        let live: Vec<&WorkItem> = work.iter().filter(|i| i.status.live()).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An item `id` in `status`, titled `Fix login`.
    fn item(id: &str, status: &str) -> Value {
        json!({"id": id, "title": "Fix login", "status": status, "task": format!("task-{id}"), "wait": null})
    }

    fn work(statuses: &[&str]) -> Vec<WorkItem> {
        let items = statuses.iter().enumerate().map(|(i, status)| item(&i.to_string(), status)).collect();
        WorkItem::list(&Value::Array(items))
    }

    #[test]
    fn work_and_max_agents_parse_and_an_unknown_status_is_left_out() {
        assert!(WorkItem::list(&Value::Null).is_empty(), "a host from before the box");
        let listed = WorkItem::list(&json!([
            {"title": "no id", "status": "working"},
            {"id": "later", "title": "A status of a newer host", "status": "paused"},
            {"id": "a", "title": "Fix login", "status": "needsYou", "activity": "Editing", "task": "task-1",
                "wait": null, "cutIn": false, "createdAt": 1, "startedAt": 5, "endedAt": null, "result": ""},
            {"id": "b", "status": "waiting", "task": null, "wait": {"kind": "behind", "ahead": 2}, "cutIn": true},
            {"id": "c", "status": "failed", "task": "task-2", "startedAt": 5, "endedAt": 9, "result": "it could not start"},
        ]));
        assert_eq!(listed.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        let [a, b, c] = &listed[..] else { panic!("three items") };
        assert_eq!(
            (a.status, a.task.as_deref(), a.wait, a.started_at),
            (WorkStatus::NeedsYou, Some("task-1"), None, Some(5))
        );
        assert_eq!(
            (b.status, b.task.as_deref(), b.wait, b.cut_in),
            (WorkStatus::Waiting, None, Some(Wait::Behind(2)), true)
        );
        assert_eq!((c.status, c.ended_at, c.result.as_str()), (WorkStatus::Failed, Some(9), "it could not start"));
        assert!(a.status.live() && !b.status.live() && c.status.ended());

        assert_eq!(max_agents(&Value::Null), None);
        assert_eq!((max_agents(&json!(2)), max_agents(&json!(0)), max_agents(&json!(9))), (Some(2), Some(1), Some(6)));
        assert_eq!((places(None), places(Some(1))), (3, 1), "no maxAgents from the host counts 3");
        let three = work(&["working", "needsYou", "working", "waiting"]);
        assert!(can_cut_in(&three, None) && !can_cut_in(&three, Some(4)) && can_cut_in(&three[1..], Some(2)));
    }

    #[test]
    fn a_waiting_item_says_why_it_waits() {
        let waits = |wait: Value, cut_in: bool| {
            let mut v = item("a", "waiting");
            v["wait"] = wait;
            v["cutIn"] = cut_in.into();
            WorkItem::parse(&v).expect("an item").line()
        };
        assert_eq!(waits(json!({"kind": "next"}), false), "Next up");
        assert_eq!(waits(json!({"kind": "behind", "ahead": 3}), false), "3 ahead");
        assert_eq!(waits(json!({"kind": "after"}), false), "Follows what it works on");
        assert_eq!(waits(json!({"kind": "usage"}), false), "Waiting for usage");
        assert_eq!(waits(Value::Null, false), "Waiting", "no reason");
        assert_eq!(waits(json!({"kind": "tomorrow"}), false), "Waiting", "a reason of a newer host");
        assert_eq!(waits(json!({"kind": "next"}), true), "Was cut in on; starts again");

        let ended = |status: &str, result: &str| {
            let mut v = item("a", status);
            v["result"] = result.into();
            WorkItem::parse(&v).expect("an item").line()
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

        let mut one = item("a", "working");
        one["activity"] = "Reading files".into();
        one["startedAt"] = 5.into();
        let alone = Background::of(&WorkItem::list(&json!([one, item("old", "done")]))).expect("one working item");
        assert_eq!((alone.what.as_str(), alone.since, alone.stops.as_deref()), ("Reading files", Some(5), Some("a")));
        let stops = |statuses: &[&str]| Background::of(&work(statuses)).and_then(|b| b.stops);
        assert_eq!(stops(&["waiting", "needsYou"]).as_deref(), Some("1"), "the one item a task has");
        assert_eq!((stops(&["working", "working"]), stops(&["waiting"])), (None, None), "several, or none");
        assert_eq!(Background::of(&work(&["working", "waiting"])).and_then(|b| b.since), None, "counted: no time");
    }

    #[test]
    fn the_sections_put_what_needs_you_first_and_keep_the_hosts_order() {
        let listed = work(&["working", "needsYou", "waiting", "done", "waiting", "stopped"]);
        let sections = Sections::of(&listed);
        let ids = |items: &[&WorkItem]| items.iter().map(|i| i.id.clone()).collect::<Vec<_>>().join(" ");
        assert_eq!(
            (ids(&sections.threads), ids(&sections.waiting), ids(&sections.recent)),
            ("1 0".into(), "2 4".into(), "3 5".into())
        );
    }
}
