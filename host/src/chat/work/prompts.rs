//! What threads are told, and what a bot is told about its box.

use super::Report;
use super::tasks::Task;
use std::fmt::Write as _;

/// What a bot is told about its box, in its instructions (and each turn, for a session opened before it).
pub const BRIEF: &str = "You do long work in the background, not in this chat: the user waits on this chat and can't \
reach you while a turn runs. Before work that needs more than a few tool calls (reading several files, changing code, \
running builds or tests, research, writing a document), call add_task(title, brief) first, then say in a sentence that \
it is under way and end your turn. One of your threads (you again, in a session of its own) does it away from this \
chat; its report comes back to you as a message, and you tell the user what it found. Add one task per piece of work: \
tasks wait their turn in your event box, and how many threads run at once is the user's setting. For more of the same \
work, add a task with after: the id of the one it continues. steer_task moves a task first, starts it now, replaces \
its brief or stops it. Only quick answers and work of a few tool calls you do here yourself.";

/// Said with each message in the bot's own chat: with the instructions alone, a real agent (Claude, 2026-11-15) did
/// 90 seconds of reading in the chat's turn instead of handing the work on.
pub const EACH_TURN: &str = "If this needs more than a few tool calls, don't do it in this turn: call add_task first \
(the chat must stay free for the user), then say briefly that it is under way.";

/// What a message in a bot's own chat says of its box, if anything. `briefed`: the session's instructions hold
/// [`BRIEF`]; `known`: the agent is a harness Codync knows. A known harness hears [`EACH_TURN`] with every message; a
/// custom command gets the message as written, with the brief only where its instructions predate the box.
pub fn reminder(briefed: bool, known: bool) -> Option<&'static str> {
    if briefed { known.then_some(EACH_TURN) } else { Some(BRIEF) }
}

/// What a thread is told to work on `task`: what it is, the work, what the user said, and how its words reach the
/// user. `first`: the thread is new; otherwise it goes on in the session it has. What it says a thread can't do is what
/// `tools::refuse_chat_only` and the other servers' `refuse_in_thread` refuse.
pub(super) fn task_prompt(task: &Task, first: bool) -> String {
    let lead = if first {
        "[Task] You are continuing as the same bot, away from the chat, on"
    } else {
        "[Task] Next, in this same thread, with what you did before in mind"
    };
    let mut said = task.words.as_deref().map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    for note in &task.notes {
        let _ = write!(said, "\n\n[The user said since: «{note}»]");
    }
    if task.cut_in {
        said.push_str(
            "\n\n[Codync: this work was begun before in another thread and set aside. See what is already done \
             before doing it again.]",
        );
    }
    format!(
        "{lead}: \"{}\".\n\n{}{said}\n\n\
         [Codync: work until it's done. Your last message is the report the chat gets, so end with what you did and \
         found. Messages you send with send_message show in this thread, not in the chat. Approvals reach the \
         user in the chat, and so does what the user must see or decide before you can go on: request_decision \
         asks them there, send_file and publish_artifact share there. You can't add a task, set a follow-up or \
         message another bot from here: say in your report what you need.]",
        task.title, task.brief
    )
}

/// How many of the user's messages to a thread its report carries, the newest, and how much of each.
const TOLD_IN_REPORT: usize = 5;
const TOLD_LONGEST: usize = 280;

/// Said after each message the user writes to a thread, so the thread knows who speaks and where its answer
/// goes.
const IN_THREAD: &str = "the user wrote this to you directly, in this task's thread, which they read. Your last \
message of this turn is again the report the chat gets.";

/// What the user wrote to a thread (`text`), as the thread hears it: after the work itself (`picked_up`) where
/// its session doesn't hold it, and saying who wrote it and where.
pub fn in_thread(text: &str, picked_up: Option<String>) -> String {
    let written = format!("{text}\n\n[Codync: {IN_THREAD}]");
    picked_up.map_or(written.clone(), |work| format!("{work}\n\n{written}"))
}

/// What a new session is told before the user's message to `task`'s thread, when the session that did the work
/// can't be picked up: the work, what the user said about it, and what it reported.
pub(super) fn picked_up(task: &Task) -> String {
    let mut said = task.words.as_deref().map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    for note in &task.notes {
        let _ = write!(said, "\n\n[The user said since: «{note}»]");
    }
    let reported = task.said.as_deref().map_or_else(
        || "[Codync: it ended without a report.]".to_owned(),
        |text| format!("[Codync: what you reported when it ended:]\n{}", crate::agent::acp::truncate(text, 2_000)),
    );
    format!(
        "[Task] You are the same bot, away from the chat. You worked on \"{}\" before, in a session that can't be \
         picked up, so this is a new one. What it was:\n\n{}{said}\n\n{reported}\n\n\
         [Codync: the user now writes to you about it. See what is already done before doing anything again.]",
        task.title, task.brief
    )
}

/// What a report says of the user's messages to the task's thread since its last report: their own words, the
/// newest few, each shortened. Nothing when they wrote none.
fn told_line(told: &[String]) -> Option<String> {
    let shown = &told[told.len().saturating_sub(TOLD_IN_REPORT)..];
    let words: Vec<String> =
        shown.iter().map(|w| format!("«{}»", crate::agent::acp::truncate(w, TOLD_LONGEST))).collect();
    let earlier = match told.len() - shown.len() {
        0 => String::new(),
        n => format!(" (and {n} earlier)"),
    };
    (!words.is_empty()).then(|| {
        format!(
            "[Codync: while it worked, the user wrote to it directly. These are the user's own words, not the \
             thread's: {}{earlier}]",
            words.join(" ")
        )
    })
}

/// The prompt of the turn that takes `reports` in the bot's chat.
pub fn report_prompt(reports: &[Report]) -> String {
    let parts: Vec<String> = reports
        .iter()
        .map(|r| {
            let said = match &r.outcome {
                Ok(text) => format!("[Codync: your task \"{}\" finished. Its report:]\n{text}", r.title),
                Err(why) => format!("[Codync: your task \"{}\" did not finish: {why}]", r.title),
            };
            // Where its changes are, when it worked in a worktree of its own; then what the user told it.
            let lines = [r.worktree.as_ref().map(super::Worktree::line), told_line(&r.told)];
            lines.into_iter().flatten().fold(said, |said, line| format!("{said}\n{line}"))
        })
        .collect();
    format!(
        "{}\n\n[Codync: these are reports of your tasks, not the user speaking. Tell the user what \
         matters in a message of your own; reply with only (pass) if there is nothing to tell.]",
        parts.join("\n\n")
    )
}

/// A report as the chat shows it when the bot's turn for it didn't answer.
pub fn report_notice(report: &Report) -> String {
    let said = match &report.outcome {
        Ok(text) => format!("\"{}\" finished in the background:\n\n{text}", report.title),
        Err(why) => format!("\"{}\" did not finish: {why}", report.title),
    };
    // Still there: its changes are on that branch, not in the bot's folder.
    match report.worktree.as_ref().filter(|worktree| worktree.wire().is_some()) {
        Some(w) => format!("{said}\n\nIt worked in a git worktree of its own: branch {}, in {}.", w.branch, w.path),
        None => said,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_report_turn_reads_each_task_and_says_who_speaks() {
        let report = |task: &str, title: &str, outcome: Result<&str, &str>| Report {
            task: task.into(),
            title: title.into(),
            outcome: outcome.map(str::to_owned).map_err(str::to_owned),
            hops: 1,
            worktree: None,
            told: Vec::new(),
        };
        let done = report("i1", "Fix login", Ok("All green."));
        let failed = report("i2", "Docs", Err("it was cut off"));
        assert_eq!(report_notice(&failed), "\"Docs\" did not finish: it was cut off");
        let prompt = report_prompt(&[done, failed]);
        assert!(prompt.contains("your task \"Fix login\" finished. Its report:]\nAll green."));
        assert!(prompt.contains("your task \"Docs\" did not finish: it was cut off"));
        assert!(prompt.ends_with("reply with only (pass) if there is nothing to tell.]"));
        assert!(!prompt.contains("the user wrote to it"), "nothing is added when the user wrote nothing");
    }

    #[test]
    fn a_report_ends_with_what_the_user_told_the_thread_as_their_own_words() {
        let told: Vec<String> = (1..=7).map(|n| format!("note {n}")).chain(["x".repeat(400)]).collect();
        let report = Report {
            task: "i".into(),
            title: "Fix".into(),
            outcome: Ok("Done.".into()),
            hops: 0,
            worktree: None,
            told,
        };
        let prompt = report_prompt(&[report]);
        assert!(prompt.contains(
            "Its report:]\nDone.\n[Codync: while it worked, the user wrote to it directly. These are the user's own \
             words, not the thread's: «note 4» «note 5» «note 6» «note 7» «"
        ));
        assert!(prompt.contains("» (and 3 earlier)]"), "a few, each shortened: {prompt}");
        assert!(!prompt.contains(&"x".repeat(300)));
    }

    #[test]
    fn a_new_session_for_a_thread_is_told_the_work_and_what_it_reported() {
        let task: Task = serde_json::from_value(json!({
            "id": "i1", "title": "Fix login", "brief": "The redirect drops the path.", "status": "working",
            "order": 1, "createdAt": 0, "words": "fix the login thing", "notes": ["keep the query"],
            "said": "All green.",
        }))
        .expect("a task");
        let prompt = picked_up(&task);
        assert!(prompt.starts_with("[Task] You are the same bot, away from the chat. You worked on \"Fix login\""));
        assert!(prompt.contains(
            "The redirect drops the path.\n\n[The user's own words: «fix the login thing»]\n\n\
             [The user said since: «keep the query»]\n\n[Codync: what you reported when it ended:]\nAll green."
        ));
    }

    #[test]
    fn a_thread_is_told_its_task_and_what_the_user_said() {
        let mut task: Task = serde_json::from_value(json!({
            "id": "i1", "title": "Fix login", "brief": "The redirect drops the path.", "status": "working",
            "order": 1, "createdAt": 0, "words": "fix the login thing", "notes": ["and keep the query"],
        }))
        .expect("a task");
        let prompt = task_prompt(&task, true);
        assert!(
            prompt.starts_with("[Task] You are continuing as the same bot, away from the chat, on: \"Fix login\".")
        );
        assert!(prompt.contains(
            "The redirect drops the path.\n\n[The user's own words: «fix the login thing»]\n\n\
             [The user said since: «and keep the query»]\n\n[Codync: work until"
        ));
        task.cut_in = true;
        assert!(task_prompt(&task, true).contains("set aside"));
        assert!(task_prompt(&task, false).starts_with("[Task] Next, in this same thread"));
    }

    #[test]
    fn a_chat_hears_of_its_box_each_turn_on_a_known_harness_and_once_elsewhere() {
        assert_eq!(reminder(false, true), Some(BRIEF), "a session from before the box gets the whole brief");
        assert_eq!(reminder(false, false), Some(BRIEF));
        assert_eq!(reminder(true, true), Some(EACH_TURN));
        assert_eq!(reminder(true, false), None, "a custom command gets the message as written");
    }
}
