use super::is_hermes_clarification;
use serde_json::json;

#[test]
fn hermes_clarification_requires_the_users_choice() {
    let question = json!({
        "toolCallId": "clarify-123",
        "rawInput": {"question": "Which one?", "choices": ["First", "Second"]}
    });
    assert!(is_hermes_clarification(&question));
    assert!(is_hermes_clarification(&json!({
        "toolCallId": "clarify-456",
        "rawInput": {"question": "What else?", "choices": []}
    })));
}

#[test]
fn ordinary_approvals_are_not_classified_as_clarifications() {
    assert!(!is_hermes_clarification(&json!({
        "toolCallId": "perm-check-1",
        "rawInput": {"command": "echo hello", "description": "Run a command"}
    })));
    assert!(!is_hermes_clarification(&json!({
        "toolCallId": "clarify-123",
        "rawInput": {"command": "echo hello"}
    })));
}
