//! What tasks are told, and what a bot is told about them.

use super::{Report, Task};

/// What a bot is told about tasks, in its instructions (and each turn, for a session opened before tasks).
pub const BRIEF: &str = "You do long work in a task, not in this chat: the user waits on this chat and can't reach you \
while a turn runs. Before work that needs more than a few tool calls (reading several files, changing code, running \
builds or tests, research, writing a document), call start_task(title, brief) first, then say in a sentence that it is \
under way and end your turn. The task is another session of you, away from this chat; its report comes back to you as \
a message, and you tell the user what it found. One task at a time: give several things to the same task, and add to \
a running one with tell_task instead of starting another. Only quick answers and work of a few tool calls you do here \
yourself.";

/// Said with each message in the bot's own chat: with the instructions alone, a real agent (Claude, 2026-11-15) did
/// 90 seconds of reading in the chat's turn instead of starting a task.
pub const EACH_TURN: &str = "If this needs more than a few tool calls, don't do it in this turn: call start_task first \
(the chat must stay free for the user), then say briefly that it is under way.";

/// What a message in a bot's own chat says of tasks, if anything. `briefed`: the session's instructions hold
/// [`BRIEF`]; `known`: the agent is a harness Codync knows. A known harness hears [`EACH_TURN`] with every message; a
/// custom command gets the message as written, with the brief only where its instructions predate tasks.
pub fn reminder(briefed: bool, known: bool) -> Option<&'static str> {
    if briefed { known.then_some(EACH_TURN) } else { Some(BRIEF) }
}

/// The first thing a task is told: what it is, the work, and how its words reach the user. What it says a task
/// can't do is what `tools::refuse_chat_only` and the other servers' `refuse_in_task` refuse.
pub(super) fn first_prompt(task: &Task) -> String {
    let words = task.words.as_deref().map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    format!(
        "[Task] You are continuing as the same bot, away from the chat, on: \"{}\".\n\n{}{words}\n\n\
         [Codync: work until it's done. Your last message is the report the chat gets, so end with what you did and \
         found. Messages you send with send_message show in the task's details, not in the chat. Approvals reach the \
         user in the chat. You can't ask the user a question, share a file, start another task, set a follow-up or \
         message another bot from here: say in your report what you need.]",
        task.title, task.brief
    )
}

/// What a task hears when its bot passes something on.
pub(super) fn told(message: &str, words: Option<&str>) -> String {
    let words = words.map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    format!("[From the chat, passed on by you there] {message}{words}")
}

/// The prompt of the turn that takes `reports` in the bot's chat.
pub fn report_prompt(reports: &[Report]) -> String {
    let parts: Vec<String> = reports
        .iter()
        .map(|r| match &r.outcome {
            Ok(text) => format!("[Codync: your task \"{}\" finished. Its report:]\n{text}", r.title),
            Err(why) => format!("[Codync: your task \"{}\" did not finish: {why}]", r.title),
        })
        .collect();
    format!(
        "{}\n\n[Codync: these are your tasks' reports, not the user speaking. Tell the user what matters in a message \
         of your own; reply with only (pass) if there is nothing to tell.]",
        parts.join("\n\n")
    )
}

/// A report as the chat shows it when the bot's turn for it didn't answer.
pub fn report_notice(report: &Report) -> String {
    match &report.outcome {
        Ok(text) => format!("The task \"{}\" finished:\n\n{text}", report.title),
        Err(why) => format!("The task \"{}\" did not finish: {why}", report.title),
    }
}

#[cfg(test)]
mod tests {
    use super::super::Status;
    use super::*;

    #[test]
    fn a_report_turn_reads_each_task_and_says_who_speaks() {
        let done =
            Report { task: "task-1".into(), title: "Fix login".into(), outcome: Ok("All green.".into()), hops: 1 };
        let failed =
            Report { task: "task-2".into(), title: "Docs".into(), outcome: Err("it was cut off".into()), hops: 1 };
        assert_eq!(report_notice(&failed), "The task \"Docs\" did not finish: it was cut off");
        let prompt = report_prompt(&[done, failed]);
        assert!(prompt.contains("your task \"Fix login\" finished. Its report:]\nAll green."));
        assert!(prompt.contains("your task \"Docs\" did not finish: it was cut off"));
        assert!(prompt.ends_with("reply with only (pass) if there is nothing to tell.]"));
    }

    #[test]
    fn a_task_is_told_its_work_and_the_users_words() {
        let task = Task {
            title: "Fix login".into(),
            brief: "The redirect drops the path.".into(),
            words: Some("fix the login thing".into()),
            status: Status::Starting,
            start: None,
            fork_from: None,
            fork_failed: false,
            hops: 1,
            created_at: 0,
            ended_at: None,
            report: None,
        };
        let prompt = first_prompt(&task);
        assert!(
            prompt.starts_with("[Task] You are continuing as the same bot, away from the chat, on: \"Fix login\".")
        );
        assert!(prompt.contains("The redirect drops the path.\n\n[The user's own words: «fix the login thing»]"));
    }

    #[test]
    fn a_chat_hears_of_tasks_each_turn_on_a_known_harness_and_once_elsewhere() {
        assert_eq!(reminder(false, true), Some(BRIEF), "a session from before tasks gets the whole brief");
        assert_eq!(reminder(false, false), Some(BRIEF));
        assert_eq!(reminder(true, true), Some(EACH_TURN));
        assert_eq!(reminder(true, false), None, "a custom command gets the message as written");
    }
}
