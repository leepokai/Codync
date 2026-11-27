//! claude-agent-acp's own `_meta`: what a session passes to the Claude Agent SDK, and the extension that reports the
//! work Claude runs in the background.

use serde_json::{Value, json};

/// The `_meta` of a new or loaded session: `append` added to Claude Code's own system prompt, with the per-session
/// parts of that prompt (working directory, git status) moved to the first message so every session shares one
/// cached system prompt, and `options` for the SDK's query (`settings`, `extraArgs`).
pub fn session_meta(append: &str, options: &Value) -> Value {
    json!({"systemPrompt": {"append": append, "excludeDynamicSections": true}, "claudeCode": {"options": options}})
}

/// What `initialize`'s `clientCapabilities._meta` adds to hear about the work Claude runs apart from its turn: a
/// command or workflow it started in the background, a monitor, a turn it began by itself
/// ([`Event::Task`](crate::events::Event::Task)). The adapter gives its `asyncTasks` extension only to a client in
/// its AIR form, which also leaves out of later messages what an earlier one said about a tool call; the reader puts
/// that back, so every event reads as for any other client. A turn still waits for the subagents it started.
pub fn background_tasks_meta() -> Value {
    json!({"jetbrains": {"air": {"version": 1, "capabilities": ["asyncTasks"]}}})
}

/// Whether an `initialize` answer says the agent reports its background work ([`background_tasks_meta`]).
pub(super) fn reports_tasks(init: &Value) -> bool {
    init["_meta"]["jetbrains"]["air"]["capabilities"].as_array().is_some_and(|c| c.iter().any(|c| c == "asyncTasks"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_and_options_go_where_the_adapter_reads_them() {
        let meta = session_meta("Be brief.", &json!({"settings": {}}));
        assert_eq!(meta["systemPrompt"], json!({"append": "Be brief.", "excludeDynamicSections": true}));
        assert_eq!(meta["claudeCode"]["options"], json!({"settings": {}}));
    }
}
