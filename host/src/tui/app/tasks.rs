//! A bot's background tasks as the host sends them (`docs/features/background-tasks.md`), and the one line that says
//! what they do.

use serde_json::Value;

use super::super::l10n::{fill, tr};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskStatus {
    Starting,
    Working,
    NeedsInput,
    Done,
    Failed,
    Stopped,
}

impl TaskStatus {
    /// Still going: a bot's `tasks` lists only these.
    pub fn live(self) -> bool {
        matches!(self, Self::Starting | Self::Working | Self::NeedsInput)
    }

    /// The status in the details' words (a catalog phrase).
    pub fn word(self) -> &'static str {
        match self {
            Self::Starting => "Starting…",
            Self::Working => "Working",
            Self::NeedsInput => "Needs you",
            Self::Done => "Done",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    /// Its current step; empty before it has one.
    pub activity: String,
    pub started_at: Option<i64>,
    /// Set once it is over (`taskEntries` only).
    pub ended_at: Option<i64>,
}

impl Task {
    pub(in crate::tui) fn parse(v: &Value) -> Option<Self> {
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            title: v["title"].as_str().unwrap_or_default().to_owned(),
            status: match v["status"].as_str() {
                Some("starting") => TaskStatus::Starting,
                Some("needsInput") => TaskStatus::NeedsInput,
                Some("done") => TaskStatus::Done,
                Some("failed") => TaskStatus::Failed,
                Some("stopped") => TaskStatus::Stopped,
                _ => TaskStatus::Working,
            },
            activity: v["activity"].as_str().unwrap_or_default().to_owned(),
            started_at: v["startedAt"].as_i64(),
            ended_at: v["endedAt"].as_i64(),
        })
    }

    /// A bot's `tasks`; a host from before tasks sends none.
    pub(in crate::tui) fn list(v: &Value) -> Vec<Self> {
        v.as_array().into_iter().flatten().filter_map(Self::parse).collect()
    }

    /// What it is doing, in a few words: that it waits on the user or is starting, else its step, else its title.
    fn what(&self) -> String {
        match self.status {
            TaskStatus::NeedsInput | TaskStatus::Starting => tr(self.status.word()).into_owned(),
            _ if self.activity.is_empty() => self.title.clone(),
            _ => tr(&self.activity).into_owned(),
        }
    }
}

/// A bot's live tasks in one line: what the line over the composer and the roster's preview say after
/// `In the background`.
#[derive(PartialEq, Eq, Debug)]
pub struct Background {
    /// One task: what it is doing. Several: how many, and whether one needs the user.
    pub what: String,
    /// When the oldest began.
    pub since: Option<i64>,
    pub needs_you: bool,
}

impl Background {
    pub fn of(tasks: &[Task]) -> Option<Self> {
        let needs_you = tasks.iter().any(|t| t.status == TaskStatus::NeedsInput);
        let what = match tasks {
            [] => return None,
            [only] => only.what(),
            several => {
                let count = fill(&tr("{count} tasks running"), &[("count", &several.len().to_string())]);
                if needs_you { format!("{count} · {}", tr("Needs you").to_lowercase()) } else { count }
            }
        };
        Some(Self { what, since: tasks.iter().filter_map(|t| t.started_at).min(), needs_you })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(status: &str, activity: &str, started_at: i64) -> Task {
        Task::parse(&json!({"id": format!("task-{started_at}"), "title": "Fix login", "status": status,
            "activity": activity, "startedAt": started_at, "start": "fork"}))
        .expect("a task with an id parses")
    }

    #[test]
    fn a_bot_without_tasks_has_none_and_one_without_an_id_is_left_out() {
        assert!(Task::list(&Value::Null).is_empty(), "a host from before tasks");
        let tasks = Task::list(&json!([{"title": "no id"}, {"id": "task-1", "title": "Fix login", "status": "later"}]));
        assert_eq!(tasks.len(), 1);
        assert_eq!((tasks[0].status, tasks[0].started_at), (TaskStatus::Working, None), "an unknown status works");
        let over = task("stopped", "", 5);
        assert!(!over.status.live());
        assert_eq!(Task::parse(&json!({"id": "t", "status": "done", "endedAt": 9})).and_then(|t| t.ended_at), Some(9));
    }

    #[test]
    fn one_task_says_its_step_and_several_are_counted() {
        let what = |tasks: &[Task]| Background::of(tasks).map(|b| b.what);
        assert_eq!(Background::of(&[]), None);
        assert_eq!(what(&[task("starting", "Reading files", 5)]).as_deref(), Some("Starting…"));
        assert_eq!(what(&[task("working", "Reading files", 5)]).as_deref(), Some("Reading files"));
        assert_eq!(what(&[task("working", "", 5)]).as_deref(), Some("Fix login"), "no step yet: its title");
        let waiting = Background::of(&[task("needsInput", "Reading files", 5)]).expect("one task");
        assert_eq!((waiting.what.as_str(), waiting.needs_you, waiting.since), ("Needs you", true, Some(5)));

        let two = [task("working", "Reading files", 9), task("starting", "", 4)];
        let both = Background::of(&two).expect("two tasks");
        assert_eq!(
            (both.what.as_str(), both.needs_you, both.since),
            ("2 tasks running", false, Some(4)),
            "the oldest's time"
        );
        let three = [two[0].clone(), task("needsInput", "", 7), two[1].clone()];
        let asked = Background::of(&three).expect("three tasks");
        assert_eq!((asked.what.as_str(), asked.needs_you), ("3 tasks running · needs you", true));
    }
}
