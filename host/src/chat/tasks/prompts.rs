//! What tasks are told, and what a bot is told about its box.

use super::Report;
use super::items::Item;
use std::fmt::Write as _;

/// What a bot is told about its box, in its instructions (and each turn, for a session opened before it).
pub const BRIEF: &str = "You do long work in the background, not in this chat: the user waits on this chat and can't \
reach you while a turn runs. Before work that needs more than a few tool calls (reading several files, changing code, \
running builds or tests, research, writing a document), call add_work(title, brief) first, then say in a sentence that \
it is under way and end your turn. Another session of you does it, away from this chat; its report comes back to you \
as a message, and you tell the user what it found. Add one item per piece of work: they wait their turn, and how many run at once is \
the user's setting. For more of the same work, add an item with after: the id of the one it continues. steer_work moves \
an item first, starts it now, replaces its brief or stops it. Only quick answers and work of a few tool calls you do \
here yourself.";

/// Said with each message in the bot's own chat: with the instructions alone, a real agent (Claude, 2026-11-15) did
/// 90 seconds of reading in the chat's turn instead of handing the work on.
pub const EACH_TURN: &str = "If this needs more than a few tool calls, don't do it in this turn: call add_work first \
(the chat must stay free for the user), then say briefly that it is under way.";

/// What a message in a bot's own chat says of its box, if anything. `briefed`: the session's instructions hold
/// [`BRIEF`]; `known`: the agent is a harness Codync knows. A known harness hears [`EACH_TURN`] with every message; a
/// custom command gets the message as written, with the brief only where its instructions predate the box.
pub fn reminder(briefed: bool, known: bool) -> Option<&'static str> {
    if briefed { known.then_some(EACH_TURN) } else { Some(BRIEF) }
}

/// What a task is told to work on `item`: what it is, the work, what the user said, and how its words reach the
/// user. `first`: the task is new; otherwise it goes on in the session it has. What it says a task can't do is what
/// `tools::refuse_chat_only` and the other servers' `refuse_in_task` refuse.
pub(super) fn item_prompt(item: &Item, first: bool) -> String {
    let lead = if first {
        "[Task] You are continuing as the same bot, away from the chat, on"
    } else {
        "[Task] Next, in this same session, with what you did before in mind"
    };
    let mut said = item.words.as_deref().map(|w| format!("\n\n[The user's own words: «{w}»]")).unwrap_or_default();
    for note in &item.notes {
        let _ = write!(said, "\n\n[The user said since: «{note}»]");
    }
    if item.cut_in {
        said.push_str(
            "\n\n[Codync: this work was begun before in another session and set aside. See what is already done \
             before doing it again.]",
        );
    }
    format!(
        "{lead}: \"{}\".\n\n{}{said}\n\n\
         [Codync: work until it's done. Your last message is the report the chat gets, so end with what you did and \
         found. Messages you send with send_message show in the task's details, not in the chat. Approvals reach the \
         user in the chat, and so does what the user must see or decide before you can go on: request_decision \
         asks them there, send_file and publish_artifact share there. You can't add more work, set a follow-up or \
         message another bot from here: say in your report what you need.]",
        item.title, item.brief
    )
}

/// The prompt of the turn that takes `reports` in the bot's chat.
pub fn report_prompt(reports: &[Report]) -> String {
    let parts: Vec<String> = reports
        .iter()
        .map(|r| {
            let said = match &r.outcome {
                Ok(text) => format!("[Codync: your work \"{}\" finished. Its report:]\n{text}", r.title),
                Err(why) => format!("[Codync: your work \"{}\" did not finish: {why}]", r.title),
            };
            // Where its changes are, when it worked in a worktree of its own.
            r.worktree.as_ref().map_or(said.clone(), |worktree| format!("{said}\n{}", worktree.line()))
        })
        .collect();
    format!(
        "{}\n\n[Codync: these are reports of your work in the background, not the user speaking. Tell the user what \
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
    fn a_report_turn_reads_each_item_and_says_who_speaks() {
        let report = |item: &str, title: &str, outcome: Result<&str, &str>| Report {
            item: item.into(),
            title: title.into(),
            outcome: outcome.map(str::to_owned).map_err(str::to_owned),
            hops: 1,
            worktree: None,
        };
        let done = report("i1", "Fix login", Ok("All green."));
        let failed = report("i2", "Docs", Err("it was cut off"));
        assert_eq!(report_notice(&failed), "\"Docs\" did not finish: it was cut off");
        let prompt = report_prompt(&[done, failed]);
        assert!(prompt.contains("your work \"Fix login\" finished. Its report:]\nAll green."));
        assert!(prompt.contains("your work \"Docs\" did not finish: it was cut off"));
        assert!(prompt.ends_with("reply with only (pass) if there is nothing to tell.]"));
    }

    #[test]
    fn a_task_is_told_its_item_and_what_the_user_said() {
        let mut item: Item = serde_json::from_value(json!({
            "id": "i1", "title": "Fix login", "brief": "The redirect drops the path.", "status": "working",
            "order": 1, "createdAt": 0, "words": "fix the login thing", "notes": ["and keep the query"],
        }))
        .expect("an item");
        let prompt = item_prompt(&item, true);
        assert!(
            prompt.starts_with("[Task] You are continuing as the same bot, away from the chat, on: \"Fix login\".")
        );
        assert!(prompt.contains(
            "The redirect drops the path.\n\n[The user's own words: «fix the login thing»]\n\n\
             [The user said since: «and keep the query»]\n\n[Codync: work until"
        ));
        item.cut_in = true;
        assert!(item_prompt(&item, true).contains("set aside"));
        assert!(item_prompt(&item, false).starts_with("[Task] Next, in this same session"));
    }

    #[test]
    fn a_chat_hears_of_its_box_each_turn_on_a_known_harness_and_once_elsewhere() {
        assert_eq!(reminder(false, true), Some(BRIEF), "a session from before the box gets the whole brief");
        assert_eq!(reminder(false, false), Some(BRIEF));
        assert_eq!(reminder(true, true), Some(EACH_TURN));
        assert_eq!(reminder(true, false), None, "a custom command gets the message as written");
    }
}
