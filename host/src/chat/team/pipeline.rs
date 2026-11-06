//! `run_pipeline`: bots work one after another, and each stage's reply is the next stage's input, with no
//! coordinating turn in between. A stage asks for what it needs with `{{previous}}` in its goal; a goal without it
//! gets the previous result added at the end. A stage that fails stops the pipeline, and the stages after it don't run.
//!
//! Idea from mercury-agent's `manifest.pipeline.stages` (MIT); no code copied.

use super::batch::{MAX_CRITERIA, MAX_GOAL_BYTES, MAX_TASKS, TaskSpec, parse_list, render_prompt};
use super::{ASK_TIMEOUT, ask, visible_bot};
use crate::agent::acp::truncate;
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::fmt::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PLACEHOLDER: &str = "{{previous}}";
/// How much of the previous stage's reply the next stage is given.
const PREVIOUS_BYTES: usize = 6_000;
/// What the pipeline's first stage may be given as `{{previous}}`.
const MAX_INPUT_BYTES: usize = 8_000;
/// The whole pipeline's time: each stage still gets at most `ASK_TIMEOUT`.
const TOTAL_TIME: Duration = Duration::from_mins(30);
/// Replies kept per stage in the digest, and for the last one.
const STAGE_REPLY_BYTES: usize = 600;
const FINAL_REPLY_BYTES: usize = 4_000;

pub(super) fn tool() -> Value {
    json!({
        "name": "run_pipeline",
        "description": format!("Run other Codync bots one after another (up to {MAX_TASKS} stages): each stage's reply becomes the next stage's input, with no turn of yours in between. Put {PLACEHOLDER} in a stage's goal where the previous stage's result belongs; without it the result is added to the end of the goal. A bot may appear in several stages. A failed stage stops the pipeline: the rest don't run, and partial work may have happened. You wait for the whole pipeline (up to 30 minutes) and get one digest plus the last stage's reply. Use delegate_batch for tasks that don't depend on each other."),
        "inputSchema": {
            "type": "object",
            "properties": {
                "stages": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_TASKS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "botId": {"type": "string", "description": "Recipient ID from list_bots."},
                            "goal": {"type": "string", "description": format!("What this stage does, in at most {MAX_GOAL_BYTES} bytes; self-contained, since the bot doesn't see your conversation.")},
                            "acceptance": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": format!("Up to {MAX_CRITERIA} checks that say the stage is done."),
                            },
                        },
                        "required": ["botId", "goal"],
                    },
                },
                "input": {"type": "string", "description": format!("What the first stage gets as {PLACEHOLDER}, if its goal asks for it.")},
            },
            "required": ["stages"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    })
}

/// How one stage ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum StageOutcome {
    Replied(String),
    Failed(String),
    /// An earlier stage failed, or the time ran out, so it never started.
    Skipped,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct StageResult {
    pub bot_name: String,
    pub goal: String,
    pub outcome: StageOutcome,
}

/// What a stage is asked: its goal with the previous result in it, then the usual checks and reply shape.
pub(super) fn stage_prompt(task: &TaskSpec, previous: Option<&str>) -> String {
    let previous = previous.map(|p| truncate(p.trim(), PREVIOUS_BYTES));
    let goal = match &previous {
        Some(result) if task.goal.contains(PLACEHOLDER) => task.goal.replace(PLACEHOLDER, result),
        Some(result) => format!("{}\n\nResult of the previous stage:\n{result}", task.goal),
        None => task.goal.replace(PLACEHOLDER, ""),
    };
    render_prompt(&TaskSpec { bot_id: task.bot_id.clone(), goal, acceptance: task.acceptance.clone() })
}

/// The time the next stage may take after `elapsed`, or none when the pipeline's time is used up.
pub(super) fn stage_budget(elapsed: Duration) -> Option<Duration> {
    TOTAL_TIME.checked_sub(elapsed).filter(|left| !left.is_zero()).map(|left| left.min(ASK_TIMEOUT))
}

/// The last stage's reply, when every stage replied: the pipeline's result.
fn final_result(results: &[StageResult]) -> Option<&str> {
    if results.iter().any(|r| !matches!(r.outcome, StageOutcome::Replied(_))) {
        return None;
    }
    match &results.last()?.outcome {
        StageOutcome::Replied(reply) => Some(reply.as_str()),
        _ => None,
    }
}

/// The 1-based stage the pipeline stopped at, if it did.
fn stopped_at(results: &[StageResult]) -> Option<usize> {
    results.iter().position(|r| !matches!(r.outcome, StageOutcome::Replied(_))).map(|i| i + 1)
}

pub(super) fn render_digest(results: &[StageResult]) -> String {
    let replied = results.iter().filter(|r| matches!(r.outcome, StageOutcome::Replied(_))).count();
    let mut text = format!("Pipeline finished: {replied} of {} stages replied.", results.len());
    if let Some(stage) = stopped_at(results) {
        let _ = write!(text, " Stopped at stage {stage}; the stages after it didn't run.");
    }
    let last = results.len();
    for (i, r) in results.iter().enumerate() {
        let (label, body) = match &r.outcome {
            StageOutcome::Replied(reply) => {
                let limit = if i + 1 == last { FINAL_REPLY_BYTES } else { STAGE_REPLY_BYTES };
                ("replied", truncate(reply.trim(), limit))
            }
            StageOutcome::Failed(error) => ("failed", error.clone()),
            StageOutcome::Skipped => ("skipped", String::new()),
        };
        let _ = write!(text, "\n\n{}. [{label}] {} — {}", i + 1, r.bot_name, truncate(&r.goal, 120));
        if !body.is_empty() {
            text.push('\n');
            text.push_str(&body);
        }
    }
    text
}

/// Runs the stages in order, each with the previous reply, and stops at the first that fails.
pub(super) async fn run(hub: &Arc<Hub>, source: &BotConfig, args: &Value) -> Result<Value> {
    let stages = parse_list(&args["stages"], &source.id, false)?;
    let input = args["input"].as_str().map(str::trim).filter(|s| !s.is_empty());
    if input.is_some_and(|text| text.len() > MAX_INPUT_BYTES) {
        return Err(anyhow!("input must be at most {MAX_INPUT_BYTES} bytes"));
    }
    let targets = stages.iter().map(|t| visible_bot(hub, &t.bot_id)).collect::<Result<Vec<_>>>()?;
    let started = Instant::now();
    let mut previous = input.map(str::to_owned);
    let mut stopped = false;
    let mut results = Vec::with_capacity(stages.len());
    for (stage, target) in stages.iter().zip(&targets) {
        let outcome = match stage_budget(started.elapsed()).filter(|_| !stopped) {
            None if stopped => StageOutcome::Skipped,
            None => {
                stopped = true;
                StageOutcome::Failed("the pipeline ran out of time before this stage".into())
            }
            Some(budget) => {
                let prompt = stage_prompt(stage, previous.as_deref());
                match ask(hub, source, &stage.bot_id, &prompt, budget).await {
                    Ok(value) => {
                        let reply = value["reply"].as_str().unwrap_or_default().to_owned();
                        previous = Some(reply.clone());
                        StageOutcome::Replied(reply)
                    }
                    Err(error) => {
                        stopped = true;
                        StageOutcome::Failed(format!("{error:#}"))
                    }
                }
            }
        };
        results.push(StageResult { bot_name: target.name.clone(), goal: stage.goal.clone(), outcome });
    }
    let rows: Vec<Value> = stages
        .iter()
        .zip(&results)
        .map(|(stage, r)| match &r.outcome {
            StageOutcome::Replied(reply) => {
                json!({"botId": stage.bot_id, "name": r.bot_name, "status": "replied", "reply": reply})
            }
            StageOutcome::Failed(error) => {
                json!({"botId": stage.bot_id, "name": r.bot_name, "status": "failed", "error": error})
            }
            StageOutcome::Skipped => json!({"botId": stage.bot_id, "name": r.bot_name, "status": "skipped"}),
        })
        .collect();
    Ok(json!({
        "digest": render_digest(&results),
        "result": final_result(&results),
        "stoppedAt": stopped_at(&results),
        "stages": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(goal: &str) -> TaskSpec {
        TaskSpec { bot_id: "b".into(), goal: goal.into(), acceptance: vec![] }
    }

    fn result(name: &str, outcome: StageOutcome) -> StageResult {
        StageResult { bot_name: name.into(), goal: format!("{name}'s goal"), outcome }
    }

    #[test]
    fn the_previous_result_goes_where_the_goal_asks_or_at_the_end() {
        let placed = stage_prompt(&stage("Summarize: {{previous}} in 3 lines"), Some("long text"));
        assert!(placed.contains("Summarize: long text in 3 lines"));
        assert!(!placed.contains("Result of the previous stage"));
        let appended = stage_prompt(&stage("Review it"), Some("draft v1"));
        assert!(appended.contains("Review it\n\nResult of the previous stage:\ndraft v1"));
    }

    #[test]
    fn the_first_stage_without_input_has_nothing_to_insert() {
        let first = stage_prompt(&stage("Research {{previous}}topic"), None);
        assert!(first.contains("Goal: Research topic"));
        assert!(!first.contains("{{previous}}") && !first.contains("previous stage"));
        let given = stage_prompt(&stage("Research {{previous}}"), Some("  rust  "));
        assert!(given.contains("Goal: Research rust"));
    }

    #[test]
    fn a_long_previous_result_is_cut_at_a_character() {
        let long = "ñ".repeat(PREVIOUS_BYTES);
        let prompt = stage_prompt(&stage("Use: {{previous}}"), Some(&long));
        assert!(prompt.contains('…'));
        assert!(prompt.len() < long.len() + 600);
    }

    #[test]
    fn a_stage_gets_what_is_left_of_the_pipelines_time() {
        assert_eq!(stage_budget(Duration::ZERO), Some(ASK_TIMEOUT));
        assert_eq!(
            stage_budget(TOTAL_TIME.checked_sub(Duration::from_secs(60)).unwrap()),
            Some(Duration::from_secs(60))
        );
        assert_eq!(stage_budget(TOTAL_TIME), None);
        assert_eq!(stage_budget(TOTAL_TIME + Duration::from_secs(1)), None);
    }

    #[test]
    fn a_finished_pipeline_has_a_result_and_no_stop() {
        let results =
            vec![result("A", StageOutcome::Replied("first".into())), result("B", StageOutcome::Replied("last".into()))];
        assert_eq!(final_result(&results), Some("last"));
        assert_eq!(stopped_at(&results), None);
        let digest = render_digest(&results);
        assert!(digest.starts_with("Pipeline finished: 2 of 2 stages replied."));
        assert!(!digest.contains("Stopped at"));
    }

    #[test]
    fn a_failed_stage_stops_the_pipeline_and_skips_the_rest() {
        let results = vec![
            result("A", StageOutcome::Replied("first".into())),
            result("B", StageOutcome::Failed("bot request timed out".into())),
            result("C", StageOutcome::Skipped),
        ];
        assert_eq!(final_result(&results), None);
        assert_eq!(stopped_at(&results), Some(2));
        let digest = render_digest(&results);
        assert!(digest.starts_with("Pipeline finished: 1 of 3 stages replied. Stopped at stage 2"));
        assert!(digest.contains("2. [failed] B") && digest.contains("bot request timed out"));
        assert!(digest.contains("3. [skipped] C"));
    }

    #[test]
    fn only_the_last_stage_keeps_a_long_reply() {
        let long = "x".repeat(FINAL_REPLY_BYTES);
        let results =
            vec![result("A", StageOutcome::Replied(long.clone())), result("B", StageOutcome::Replied(long.clone()))];
        let digest = render_digest(&results);
        let first = digest.find("1. [replied]").unwrap();
        let second = digest.find("2. [replied]").unwrap();
        assert!(second - first < STAGE_REPLY_BYTES + 200, "stage 1 is cut short");
        assert!(digest.len() - second > FINAL_REPLY_BYTES - 100, "the last reply is kept in full");
    }

    #[test]
    fn the_tool_takes_stages_and_an_optional_input() {
        let tool = tool();
        assert_eq!(tool["name"], "run_pipeline");
        assert_eq!(tool["inputSchema"]["required"], json!(["stages"]));
        assert!(tool["inputSchema"]["properties"]["input"].is_object());
    }
}
