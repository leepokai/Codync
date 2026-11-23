//! claude-agent-acp's own session `_meta`: what it passes to the Claude Agent SDK.

use serde_json::{Value, json};

/// The `_meta` of a new or loaded session: `append` added to Claude Code's own system prompt, with the per-session
/// parts of that prompt (working directory, git status) moved to the first message so every session shares one
/// cached system prompt, and `options` for the SDK's query (`settings`, `extraArgs`).
pub fn session_meta(append: &str, options: &Value) -> Value {
    json!({"systemPrompt": {"append": append, "excludeDynamicSections": true}, "claudeCode": {"options": options}})
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
