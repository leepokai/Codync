//! What the hook prints back to the agent: the only place that knows each agent's stdout contract.
//!
//! The one rule that matters: **nothing that allows anything is ever printed without a decision a person
//! chose.** With no decision the reply is silence, an empty `{}`, or Copilot's explicit "ask", and the agent then
//! asks in its own terminal exactly as if Codync were not installed.
//!
//! Adapted from the MIT-licensed `coucou-hook` (github.com/Louis-CFM/coucou, `windows/hook/src/reply.rs`).

use serde_json::{Value, json};

/// The agents whose permission requests Codync can answer: Claude Code (no `--agent`), Codex, Copilot CLI.
/// A decision for any other agent is ignored, whatever the host sent.
pub fn takes_decisions(agent: &str) -> bool {
    matches!(agent, "" | "codex" | "copilot")
}

/// Agents that read a JSON object on stdout after every hook and take `{}` as "no opinion".
fn wants_json(agent: &str) -> bool {
    matches!(agent, "gemini" | "antigravity" | "copilot")
}

/// The line to print for `event` from `agent`, given the person's `decision` (`None`: nobody chose).
pub fn stdout(agent: &str, event: &str, decision: Option<&str>) -> Option<String> {
    if event != "PermissionRequest" {
        // Antigravity reads "{}" on PreToolUse as a denial; "ask" keeps its own prompt.
        if agent.eq_ignore_ascii_case("antigravity") && event == "PreToolUse" {
            return Some(r#"{"decision":"ask"}"#.to_owned());
        }
        return wants_json(agent).then(|| "{}".to_owned());
    }
    let decision = decision.map(str::trim).filter(|_| takes_decisions(agent));
    if agent == "copilot" {
        // Copilot is fail-closed: it must always get valid JSON, so silence is an "ask".
        let word = match decision {
            Some("allow") => "allow",
            Some("deny") => "deny",
            _ => "ask",
        };
        return Some(json!({ "permissionDecision": word }).to_string());
    }
    let behavior = match decision? {
        "allow" => json!({"behavior": "allow"}),
        "deny" => json!({"behavior": "deny", "message": "Denied from Codync"}),
        _ => return None,
    };
    Some(json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": behavior}}).to_string())
}

/// What a `PreToolUse` hook of a question or a plan prints, from the host's reply (`{"preToolUse": {...}}`). Only
/// a verdict the host made from a person's choice is ever printed: a missing or malformed one is silence, and the
/// terminal asks as usual.
pub fn pre_tool(reply: &Value) -> Option<String> {
    let verdict = reply.get("preToolUse")?;
    let mut out = match verdict.get("permissionDecision")?.as_str()? {
        "allow" => json!({"permissionDecision": "allow"}),
        "deny" => json!({
            "permissionDecision": "deny",
            "permissionDecisionReason": verdict.get("reason").and_then(Value::as_str).unwrap_or("Declined from Codync"),
        }),
        _ => return None,
    };
    if let Some(input) = verdict.get("updatedInput").filter(|i| i.is_object()) {
        out["updatedInput"] = input.clone();
    }
    out["hookEventName"] = "PreToolUse".into();
    Some(json!({"hookSpecificOutput": out}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_answer_prints_an_allow_with_the_answers_and_nothing_else_does() {
        let reply = json!({"decision": null, "preToolUse": {"permissionDecision": "allow",
            "updatedInput": {"questions": [], "answers": {"Which?": "A"}}}});
        let out: Value = serde_json::from_str(&pre_tool(&reply).unwrap()).unwrap();
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(out["hookSpecificOutput"]["updatedInput"]["answers"]["Which?"], "A");
        for silent in [
            json!({}),
            json!({"decision": "allow"}),
            json!({"preToolUse": {}}),
            json!({"preToolUse": {"permissionDecision": "ask"}}),
            json!({"preToolUse": "allow"}),
        ] {
            assert_eq!(pre_tool(&silent), None, "{silent}");
        }
    }

    #[test]
    fn a_rejected_plan_prints_a_denial_with_its_reason() {
        let reply = json!({"preToolUse": {"permissionDecision": "deny", "reason": "Not this plan"}});
        let out: Value = serde_json::from_str(&pre_tool(&reply).unwrap()).unwrap();
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(out["hookSpecificOutput"]["permissionDecisionReason"], "Not this plan");
    }

    const AGENTS: &[&str] = &["", "gemini", "antigravity", "cursor", "codex", "copilot", "my-tool"];
    const EVENTS: &[&str] =
        &["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Stop", ""];

    fn allows(out: &str) -> bool {
        let v: Value = serde_json::from_str(out).expect("every reply is JSON");
        let decided = v.get("decision").is_some_and(|d| d.as_str() != Some("ask"));
        v.to_string().contains("allow") || decided
    }

    #[test]
    fn nothing_is_allowed_without_a_decision() {
        for agent in AGENTS {
            for event in EVENTS {
                for decision in [None, Some(""), Some("ask"), Some("maybe"), Some("{\"permissionDecision\":\"allow\"}")]
                {
                    if let Some(out) = stdout(agent, event, decision) {
                        assert!(!allows(&out), "{agent:?} {event} {decision:?} printed {out}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_decision_only_counts_on_a_permission_request_from_an_agent_that_takes_one() {
        for agent in AGENTS {
            for event in EVENTS.iter().filter(|e| **e != "PermissionRequest") {
                if let Some(out) = stdout(agent, event, Some("allow")) {
                    assert!(!allows(&out), "{agent:?} {event}: {out}");
                }
            }
            let out = stdout(agent, "PermissionRequest", Some("allow"));
            assert_eq!(out.as_deref().is_some_and(allows), takes_decisions(agent), "{agent:?}: {out:?}");
        }
    }

    #[test]
    fn each_agent_gets_its_own_reply_shape() {
        let allow = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;
        let deny = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Denied from Codync"}}}"#;
        for agent in ["", "codex"] {
            assert_eq!(stdout(agent, "PermissionRequest", Some("allow")).as_deref(), Some(allow));
            assert_eq!(stdout(agent, "PermissionRequest", Some("deny")).as_deref(), Some(deny));
            assert_eq!(stdout(agent, "PermissionRequest", None), None);
            assert_eq!(stdout(agent, "PreToolUse", None), None);
        }
        assert_eq!(
            stdout("copilot", "PermissionRequest", Some("allow")).as_deref(),
            Some(r#"{"permissionDecision":"allow"}"#)
        );
        assert_eq!(stdout("copilot", "PermissionRequest", None).as_deref(), Some(r#"{"permissionDecision":"ask"}"#));
        assert_eq!(stdout("antigravity", "PreToolUse", None).as_deref(), Some(r#"{"decision":"ask"}"#));
        assert_eq!(stdout("gemini", "PostToolUse", None).as_deref(), Some("{}"));
        assert_eq!(stdout("cursor", "PreToolUse", None), None);
    }
}
