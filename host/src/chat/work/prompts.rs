//! What threads are told, and what a bot is told about its box.

use super::Report;
use super::tasks::{Source, Task};
use std::fmt::Write as _;

/// What a bot is told about its box, in its instructions (and each turn, for a session opened before it).
pub const BRIEF: &str = "You do long work in the background, not in this chat: the user waits on this chat and can't \
reach you while a turn runs. Before work that needs more than a few tool calls (reading several files, changing code, \
running builds or tests, research, writing a document), call add_task(title, brief) first, then say in a sentence that \
it is under way and end your turn. One of your threads does it away from this chat, in a session of its own that \
sees nothing of this chat: write the brief so it stands alone (the goal, what you know, the user's words that matter, \
what done looks like). Its report comes back to you as a message, and you tell the user what it found. Add one task per piece of work: \
tasks wait their turn in your event box, and how many threads run at once is the user's setting. For more of the same \
work, add a task with after: the id of the one it continues. steer_task moves a task first, starts it now, replaces \
its brief or stops it. tell_task passes words on to a task without starting it again: what the user adds about work \
under way, a fact it needs. ask_task asks a working task a short question and gives you its answer; a task that asks \
you something (ask_chat) waits for answer_task. recall_task looks back at your tasks, ended ones too, up to 30 days: \
what one was asked, what it found and its thread's steps. Only quick answers and work of a few tool calls you do here \
yourself.";

/// Said with each message in the bot's own chat: with the instructions alone, a real agent (Claude, 2026-11-15) did
/// 90 seconds of reading in the chat's turn instead of handing the work on.
pub const EACH_TURN: &str = "If this needs more than a few tool calls, don't do it in this turn: call add_task first \
(the chat must stay free for the user), then say briefly that it is under way. What is about a task under way, \
tell_task passes on.";

/// What a message in a bot's own chat says of its box, if anything. `briefed`: the session's instructions hold
/// [`BRIEF`]; `known`: the agent is a harness Codync knows. A known harness hears [`EACH_TURN`] with every message; a
/// custom command gets the message as written, with the brief only where its instructions predate the box.
pub fn reminder(briefed: bool, known: bool) -> Option<&'static str> {
    if briefed { known.then_some(EACH_TURN) } else { Some(BRIEF) }
}

/// What a thread is told to work on `task`: what it is, the work, what the user said, and that its last message is
/// its report. `first`: the thread is new; otherwise it goes on in the session it has.
pub(super) fn task_prompt(task: &Task, first: bool) -> String {
    let lead = if first { "[Task]" } else { "[Task] Next, in this same thread, with what you did before in mind:" };
    let mut said = said(task);
    if task.cut_in {
        said.push_str(
            "\n\n[Codync: this work was begun before in another thread and set aside. See what is already done \
             before doing it again.]",
        );
    }
    format!(
        "{lead} \"{}\".\n\n{}{said}\n\n\
         [Codync: work until it's done. Don't end your turn while something you started in the background still \
         runs: wait for it and report its result. Your last message is the report the chat gets, so end with what \
         you did and found. What you need that you can't do from here, say in your report.]",
        task.title, task.brief
    )
}

/// What a thread is told when it ended its turn while `running` (words that fit "while … still runs") still runs:
/// its task isn't done, so it waits for that and reports then (`work::hold`).
pub(super) fn wait_for(running: &str) -> String {
    format!(
        "[Codync: you ended your turn while {running} still runs, so your task isn't done and nothing was reported \
         yet. Wait for it here, in the foreground, without starting it again, then end with your report of what it \
         found; if it already finished, end with that report now. If you end your turn again while it still runs, \
         your task ends as stopped early.]"
    )
}

/// The prompt of the turn the watchdog starts in the bot's chat for `task`, which looks stuck (`evidence`: what
/// shows it).
pub(super) fn stuck(task: &Task, evidence: &str) -> String {
    format!(
        "[Codync watchdog] Task \"{}\" ({}) looks stuck: {evidence}. Look at it (work_status, recall_task) and do one \
         thing: tell_task it what to change, steer_task it (stop, or replace it with a better brief), or, if it is \
         fine, reply exactly (pass). This is Codync checking on your work, not the user speaking: if you act, tell \
         the user in a sentence what you did.",
        task.title, task.id
    )
}

/// What was said about `task`, each on its own paragraph: the user's own words that led to it, what they said since,
/// and what the bot's chat passed on (`tell_task`), which is never theirs.
fn said(task: &Task) -> String {
    let mut said = task.words.as_deref().map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    for note in &task.notes {
        let _ = write!(said, "\n\n[The user said since: «{note}»]");
    }
    for passed in &task.relayed {
        let _ = write!(said, "\n\n[Your chat passed on: «{passed}»]");
    }
    said
}

/// How many of the user's messages to a thread its report carries, the newest, and how much of each.
const TOLD_IN_REPORT: usize = 5;
const TOLD_LONGEST: usize = 280;

/// Said after each message the user writes to a thread, so the thread knows who speaks and where its answer
/// goes.
const IN_THREAD: &str = "the user wrote this to you directly, in this task's thread, which they read. Your last \
message of this turn is again the report the chat gets.";

/// Said after what the bot's chat passes on to a thread (`tell_task`), so the thread knows it isn't the user there.
const RELAYED: &str = "your chat passed this on to you while you work on this task (it may carry what the user said \
there). Take it into account and go on; your last message of this turn is again the report the chat gets.";

/// Said after the bot's question to a thread (`ask_task`): its next message goes back to the bot as the answer.
const ASKED: &str = "the bot that gave you this task asks you this while you work on it. Answer it in one short \
message, then go on with your task: your last message of this turn is still the report the chat gets.";

/// The bot's question to its working task (`text`), as the thread hears it.
pub fn asked(text: &str) -> String {
    format!("{text}\n\n[Codync: {ASKED}]")
}

/// The turn a thread's answer to the bot's question starts in its chat, once the call stopped waiting for it.
pub(super) fn late_answer(title: &str, task: &str, question: &str, answer: &str) -> String {
    format!(
        "[Codync: your task \"{title}\" ({task}) answered the question you asked it with {} («{question}»), after \
         your call stopped waiting.]\n{answer}\n\n[Codync: this is your thread's answer, not the user speaking: act on \
         it if it matters, and tell the user only what they need to know.]",
        super::ask::TOOL
    )
}

/// The turn a thread's question to its bot (`ask_chat`) starts in the bot's chat.
pub(super) fn question(title: &str, task: &str, question: &str) -> String {
    format!(
        "[Codync: your task \"{title}\" ({task}) asks you this while it works, and waits for your answer:]\n\
         {question}\n\n[Codync: this is your thread asking, not the user. Answer it with {}(task, answer) from what \
         you know: this chat, your memory, the files. What only the user can decide, such as a permission or their \
         own preference, say so in your answer: the task asks them itself. Once answered, reply with only (pass) \
         unless the user needs to know.]",
        super::ask_chat::ANSWER
    )
}

/// What the bot's chat passed on to its working task (`text`), as the thread hears it.
pub fn relayed(text: &str) -> String {
    format!("{text}\n\n[Codync: {RELAYED}]")
}

/// What the user wrote to a thread (`text`), as the thread hears it: after the work itself (`picked_up`) where
/// its session doesn't hold it, and saying who wrote it and where.
pub fn in_thread(text: &str, picked_up: Option<String>) -> String {
    let written = format!("{text}\n\n[Codync: {IN_THREAD}]");
    picked_up.map_or(written.clone(), |work| format!("{work}\n\n{written}"))
}

/// What a thread the user started is told before their first message.
const OPENED: &str = "[Task] You are the same bot, away from your chat: the user opened this thread to work with you \
directly, and reads it here. It starts without your chat's conversation.";

/// What a new session is told before the user's message to `task`'s thread, when the session that did the work
/// can't be picked up: the work, what the user said about it, and what it reported.
pub(super) fn picked_up(task: &Task) -> String {
    // A thread the user started hears its first message: there is no earlier work to tell it of.
    if task.source == Source::User && task.said.is_none() && task.notes.is_empty() {
        return OPENED.to_owned();
    }
    let said = said(task);
    let reported = task.said.as_deref().map_or_else(
        || "[Codync: it ended without a report.]".to_owned(),
        |text| format!("[Codync: what you reported when it ended:]\n{}", codync_agent::acp::truncate(text, 2_000)),
    );
    format!(
        "[Task] You worked on \"{}\" before, in a session that can't be picked up, so this is a new one. What it \
         was:\n\n{}{said}\n\n{reported}\n\n\
         [Codync: the user now writes to you about it. See what is already done before doing anything again.]",
        task.title, task.brief
    )
}

/// What a report says of the user's messages to the task's thread since its last report: their own words, the
/// newest few, each shortened. Nothing when they wrote none.
fn told_line(told: &[String]) -> Option<String> {
    let shown = &told[told.len().saturating_sub(TOLD_IN_REPORT)..];
    let words: Vec<String> =
        shown.iter().map(|w| format!("«{}»", codync_agent::acp::truncate(w, TOLD_LONGEST))).collect();
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
    fn what_the_chat_passes_on_says_it_is_not_the_user_there() {
        let text = relayed("Also run the API tests.");
        assert!(text.starts_with("Also run the API tests.\n\n[Codync: your chat passed this on"), "{text}");
        assert!(text.ends_with("the report the chat gets.]"));
    }

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
        assert!(prompt.starts_with("[Task] You worked on \"Fix login\" before"), "{prompt}");
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
            "relayed": ["the API page too"],
        }))
        .expect("a task");
        let prompt = task_prompt(&task, true);
        assert!(prompt.starts_with("[Task] \"Fix login\".\n\nThe redirect drops the path."), "{prompt}");
        assert!(prompt.contains(
            "The redirect drops the path.\n\n[The user's own words: «fix the login thing»]\n\n\
             [The user said since: «and keep the query»]\n\n[Your chat passed on: «the API page too»]\n\n\
             [Codync: work until"
        ));
        assert!(prompt.contains("Don't end your turn while something you started in the background still runs"));
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
