//! `delegate_batch`: a bot hands several bots a task each and gets one answer when all of them are done, instead of
//! waking once per `ask_bot` reply. A task is typed: a goal, optional acceptance criteria, and a reply that says how
//! it went. The answer is a digest the coordinator can use as it is.
//!
//! Idea from mercury-agent's `tasks.ts` and batch fan-in (MIT); no code copied.

use super::{ask, visible_bot};
use crate::agent::acp::truncate;
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

/// Tasks in one batch: more than a coordinator can read back is a job for a routine, not a call.
pub(super) const MAX_TASKS: usize = 8;
pub(super) const MAX_GOAL_BYTES: usize = 8_000;
pub(super) const MAX_CRITERIA: usize = 10;
const MAX_CRITERION_BYTES: usize = 500;
/// How much of each reply the digest keeps.
const DIGEST_REPLY_BYTES: usize = 1_500;

pub(super) fn tool() -> Value {
    json!({
        "name": "delegate_batch",
        "description": format!("Give up to {MAX_TASKS} other Codync bots one task each, at the same time, and wait for all of them (up to 10 minutes). You get one digest with how each task went, instead of one wake-up per bot. Each task is a goal plus optional acceptance criteria the bot checks its work against. The bots work in their own folders with their own permissions and don't see your conversation: make each goal self-contained. A failed or timed-out task is reported in the digest; partial work may have happened, so don't blindly retry it. Use ask_bot for a single bot."),
        "inputSchema": {
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_TASKS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "botId": {"type": "string", "description": "Recipient ID from list_bots. Each bot at most once."},
                            "goal": {"type": "string", "description": "What to do, with the context, file paths and result you expect."},
                            "timeoutSeconds": {"type":"integer","minimum":1,"maximum":600,"description":"Task deadline including queue time (default 600). Expiry cancels this task only; partial work may remain."},
                            "acceptance": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": format!("Up to {MAX_CRITERIA} checks that say the task is done."),
                            },
                        },
                        "required": ["botId", "goal"],
                    },
                },
            },
            "required": ["tasks"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    })
}

/// One task of a batch, as the coordinator wrote it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct TaskSpec {
    pub bot_id: String,
    pub goal: String,
    pub acceptance: Vec<String>,
    pub timeout: Duration,
}

/// How one task ended: the bot's reply, or why there is none.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct TaskResult {
    pub bot_name: String,
    pub goal: String,
    pub outcome: Result<String, String>,
}

/// The tasks of a `delegate_batch` call by `own_id`. A batch with a bad task is refused whole, before any bot works.
pub(super) fn parse(args: &Value, own_id: &str) -> Result<Vec<TaskSpec>> {
    parse_list(&args["tasks"], own_id, true)
}

/// The tasks in `list`: each bot at most once when `unique` (a batch runs them together), any bot any number of
/// times otherwise (a pipeline runs them one after another).
pub(super) fn parse_list(list: &Value, own_id: &str, unique: bool) -> Result<Vec<TaskSpec>> {
    let list = list.as_array().ok_or_else(|| anyhow!("tasks must be a list"))?;
    if list.is_empty() || list.len() > MAX_TASKS {
        bail!("a batch holds 1 to {MAX_TASKS} tasks");
    }
    let mut seen = HashSet::new();
    list.iter()
        .enumerate()
        .map(|(i, task)| {
            let n = i + 1;
            let bot_id = task["botId"].as_str().map(str::trim).filter(|s| !s.is_empty());
            let bot_id = bot_id.ok_or_else(|| anyhow!("task {n}: botId is required"))?;
            if bot_id == own_id {
                bail!("task {n}: a bot cannot give itself a task");
            }
            if !seen.insert(bot_id.to_owned()) && unique {
                bail!("task {n}: each bot can have one task per batch");
            }
            let goal = task["goal"].as_str().map(str::trim).unwrap_or_default();
            if goal.is_empty() || goal.len() > MAX_GOAL_BYTES {
                bail!("task {n}: goal must be nonempty and at most {MAX_GOAL_BYTES} bytes");
            }
            let acceptance: Vec<String> = match task.get("acceptance") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|item| item.as_str().map(str::trim).filter(|s| !s.is_empty()))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| anyhow!("task {n}: acceptance holds nonempty strings"))?
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                Some(_) => bail!("task {n}: acceptance must be a list"),
            };
            if acceptance.len() > MAX_CRITERIA || acceptance.iter().any(|c| c.len() > MAX_CRITERION_BYTES) {
                bail!("task {n}: at most {MAX_CRITERIA} criteria of {MAX_CRITERION_BYTES} bytes each");
            }
            let timeout = super::blocking::task_timeout(task).map_err(|error| anyhow!("task {n}: {error}"))?;
            Ok(TaskSpec { bot_id: bot_id.to_owned(), goal: goal.to_owned(), acceptance, timeout })
        })
        .collect()
}

/// What the recipient is asked: the goal, the checks, and the shape of the reply the digest relies on.
pub(super) fn render_prompt(task: &TaskSpec) -> String {
    let mut text = format!("Goal: {}", task.goal);
    if !task.acceptance.is_empty() {
        text.push_str("\n\nAcceptance criteria (check your work against each before you reply):");
        for criterion in &task.acceptance {
            text.push_str("\n- ");
            text.push_str(criterion);
        }
    }
    text.push_str(
        "\n\nStart your reply with one line, `STATUS: done`, `STATUS: partial` or `STATUS: blocked`, then a short summary \
         and the paths of any files you produced.",
    );
    text
}

/// The coordinator's one answer: the counts, then each task with its reply or its error.
pub(super) fn render_digest(results: &[TaskResult]) -> String {
    let done = results.iter().filter(|r| r.outcome.is_ok()).count();
    let failed = results.len() - done;
    let mut text = format!("Batch finished: {done} of {} tasks returned a reply, {failed} failed.", results.len());
    for (i, r) in results.iter().enumerate() {
        let (label, body) = match &r.outcome {
            Ok(reply) => ("replied", truncate(reply.trim(), DIGEST_REPLY_BYTES)),
            Err(error) => ("failed", error.clone()),
        };
        let _ = write!(text, "\n\n{}. [{label}] {} — {}\n{body}", i + 1, r.bot_name, truncate(&r.goal, 120));
    }
    text
}

/// Runs the batch: every task at once, and the digest when the last one ends.
pub(super) async fn run(hub: &Arc<Hub>, source: &BotConfig, args: &Value) -> Result<Value> {
    let tasks = parse(args, &source.id)?;
    let targets = tasks.iter().map(|t| visible_bot(hub, &t.bot_id)).collect::<Result<Vec<_>>>()?;
    let prompts: Vec<String> = tasks.iter().map(render_prompt).collect();
    let replies = futures::future::join_all(
        tasks.iter().zip(&prompts).map(|(task, prompt)| ask(hub, source, &task.bot_id, prompt, task.timeout)),
    )
    .await;
    let results: Vec<TaskResult> = tasks
        .iter()
        .zip(&targets)
        .zip(replies)
        .map(|((task, target), reply)| TaskResult {
            bot_name: target.name.clone(),
            goal: task.goal.clone(),
            outcome: reply
                .map(|value| value["reply"].as_str().unwrap_or_default().to_owned())
                .map_err(|error| format!("{error:#}")),
        })
        .collect();
    let done = results.iter().filter(|r| r.outcome.is_ok()).count();
    let rows: Vec<Value> = tasks
        .iter()
        .zip(&results)
        .map(|(task, r)| match &r.outcome {
            Ok(reply) => json!({"botId": task.bot_id, "name": r.bot_name, "status": "replied", "reply": reply}),
            Err(error) => json!({"botId": task.bot_id, "name": r.bot_name, "status": "failed", "error": error}),
        })
        .collect();
    Ok(json!({"digest": render_digest(&results), "replied": done, "failed": results.len() - done, "tasks": rows}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::team::ASK_TIMEOUT;

    #[test]
    fn task_deadlines_are_preserved_and_invalid_later_tasks_reject_the_whole_batch() {
        let tasks = parse(
            &json!({"tasks":[
                {"botId":"b","goal":"review","timeoutSeconds":7},
                {"botId":"c","goal":"summarize"}
            ]}),
            "a",
        )
        .unwrap();
        assert_eq!(tasks[0].timeout, Duration::from_secs(7));
        assert_eq!(tasks[1].timeout, ASK_TIMEOUT);
        for value in [json!(0), json!(-1), json!(601), json!(1.5), json!("30"), Value::Null] {
            let error = parse(
                &json!({"tasks":[
                    {"botId":"b","goal":"valid"},
                    {"botId":"c","goal":"invalid","timeoutSeconds":value}
                ]}),
                "a",
            )
            .unwrap_err();
            assert!(error.to_string().contains("task 2: timeoutSeconds"));
        }
    }

    fn spec(bot: &str, goal: &str) -> TaskSpec {
        TaskSpec { bot_id: bot.into(), goal: goal.into(), acceptance: vec![], timeout: ASK_TIMEOUT }
    }

    #[test]
    fn a_valid_batch_is_parsed_with_trimmed_text() {
        let args = json!({"tasks": [
            {"botId": " b ", "goal": "  Review the diff ", "acceptance": [" no TODOs ", "tests pass"]},
            {"botId": "c", "goal": "Write notes"},
        ]});
        let tasks = parse(&args, "a").unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks[0],
            TaskSpec {
                bot_id: "b".into(),
                goal: "Review the diff".into(),
                acceptance: vec!["no TODOs".into(), "tests pass".into()],
                timeout: ASK_TIMEOUT,
            }
        );
        assert_eq!(tasks[1].acceptance, Vec::<String>::new());
    }

    #[test]
    fn a_bad_task_refuses_the_whole_batch() {
        let own = "a";
        let bad = |tasks: Value| parse(&json!({"tasks": tasks}), own).unwrap_err().to_string();
        assert!(bad(json!([])).contains("1 to 8"));
        assert!(bad(json!(vec![json!({"botId": "x", "goal": "g"}); 9])).contains("1 to 8"));
        assert!(bad(json!([{"botId": "a", "goal": "g"}])).contains("itself"));
        assert!(bad(json!([{"botId": "b", "goal": "g"}, {"botId": "b", "goal": "h"}])).contains("task 2"));
        assert!(bad(json!([{"goal": "g"}])).contains("botId"));
        assert!(bad(json!([{"botId": "b", "goal": "   "}])).contains("goal"));
        assert!(bad(json!([{"botId": "b", "goal": "g", "acceptance": "tests pass"}])).contains("list"));
        assert!(bad(json!([{"botId": "b", "goal": "g", "acceptance": [""]}])).contains("nonempty"));
        let many: Vec<String> = (0..=MAX_CRITERIA).map(|n| format!("check {n}")).collect();
        assert!(bad(json!([{"botId": "b", "goal": "g", "acceptance": many}])).contains("criteria"));
        assert!(parse(&json!({}), own).is_err());
    }

    #[test]
    fn the_prompt_carries_the_goal_the_checks_and_the_reply_shape() {
        let mut task = spec("b", "Review the diff");
        let plain = render_prompt(&task);
        assert!(plain.starts_with("Goal: Review the diff"));
        assert!(plain.contains("STATUS: done") && !plain.contains("Acceptance"));
        task.acceptance = vec!["tests pass".into(), "no TODOs".into()];
        let full = render_prompt(&task);
        assert!(full.contains("Acceptance criteria"));
        assert!(full.contains("\n- tests pass\n- no TODOs"));
    }

    #[test]
    fn the_digest_counts_and_orders_the_tasks() {
        let results = vec![
            TaskResult {
                bot_name: "Reviewer".into(),
                goal: "Review".into(),
                outcome: Ok("STATUS: done\nLooks fine.".into()),
            },
            TaskResult {
                bot_name: "Writer".into(),
                goal: "Draft notes".into(),
                outcome: Err("bot request timed out".into()),
            },
        ];
        let digest = render_digest(&results);
        assert!(digest.starts_with("Batch finished: 1 of 2 tasks returned a reply, 1 failed."));
        let first = digest.find("1. [replied] Reviewer").unwrap();
        let second = digest.find("2. [failed] Writer").unwrap();
        assert!(first < second);
        assert!(digest.contains("Looks fine.") && digest.contains("bot request timed out"));
    }

    #[test]
    fn a_long_reply_is_cut_in_the_digest_without_splitting_a_character() {
        let reply = "é".repeat(DIGEST_REPLY_BYTES);
        let results = vec![TaskResult { bot_name: "B".into(), goal: "g".into(), outcome: Ok(reply.clone()) }];
        let digest = render_digest(&results);
        assert!(digest.len() < reply.len());
        assert!(digest.contains('…'));
    }

    #[test]
    fn the_tool_names_its_limits() {
        let tool = tool();
        assert_eq!(tool["name"], "delegate_batch");
        assert_eq!(tool["inputSchema"]["properties"]["tasks"]["maxItems"], MAX_TASKS);
        assert_eq!(tool["inputSchema"]["required"], json!(["tasks"]));
    }
}
