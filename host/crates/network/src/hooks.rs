//! The turn tier: a coding agent's hook tells its session, at its next tool call or when its turn
//! ends, that letters wait for it. Codex, Copilot CLI and Grok run hooks the same way (a command
//! with JSON on stdin naming `hook_event_name` and `session_id`) but read their answers differently.
//! ([agent sessions](https://github.com/leepokai/Codync/blob/dev/docs/plans/agent-sessions.md#hooks-turn-tier))

use crate::wire::AgentKind;
use serde_json::{Value, json};

/// A hook the turn tier answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookEvent {
    /// After a tool call: the text joins the agent's context.
    PostToolUse,
    /// The turn ends: the text keeps it going.
    Stop,
}

/// The session and the event a hook's stdin names; `None` for an event the turn tier doesn't answer.
pub fn hook_input(stdin: &Value) -> Option<(String, HookEvent)> {
    let event = match stdin["hook_event_name"].as_str()? {
        "PostToolUse" => HookEvent::PostToolUse,
        "Stop" => HookEvent::Stop,
        _ => return None,
    };
    let session = stdin["session_id"].as_str().filter(|s| !s.is_empty())?;
    Some((session.to_owned(), event))
}

/// What `kind`'s hook prints for `event` so the agent reads `text`. Codex refuses an answer with
/// fields it doesn't know, and Copilot CLI reads only the top-level one.
pub fn hook_output(kind: AgentKind, event: HookEvent, text: &str) -> Value {
    match event {
        HookEvent::Stop => json!({"decision": "block", "reason": text}),
        HookEvent::PostToolUse if kind == AgentKind::Copilot => json!({"additionalContext": text}),
        HookEvent::PostToolUse => {
            json!({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": text}})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_names_its_session_and_gets_the_answer_its_agent_reads() {
        let stdin = json!({"hook_event_name": "PostToolUse", "session_id": "s1", "tool_name": "Bash"});
        assert_eq!(hook_input(&stdin), Some(("s1".into(), HookEvent::PostToolUse)));
        assert_eq!(hook_input(&json!({"hook_event_name": "SessionStart", "session_id": "s1"})), None);
        assert_eq!(hook_input(&json!({"hook_event_name": "Stop", "session_id": ""})), None);
        assert_eq!(hook_output(AgentKind::Copilot, HookEvent::PostToolUse, "x"), json!({"additionalContext": "x"}));
        assert_eq!(
            hook_output(AgentKind::Codex, HookEvent::PostToolUse, "x")["hookSpecificOutput"]["additionalContext"],
            "x"
        );
        assert_eq!(hook_output(AgentKind::Grok, HookEvent::Stop, "x"), json!({"decision": "block", "reason": "x"}));
    }
}
