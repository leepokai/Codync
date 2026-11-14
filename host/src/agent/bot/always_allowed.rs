//! What the user chose Always allow for. Some agents ask about the same kind of action again anyway (`MiniMax` Code
//! asks before every command), so Codync answers their next request of that kind in that session itself.

use serde_json::Value;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct AlwaysAllowed(HashSet<(String, String)>);

impl AlwaysAllowed {
    /// The user answered a card with `option_id` in `session`: an Always allow covers the card's kind of action.
    pub(super) fn note(&mut self, session: &str, card: &Value, option_id: &str) {
        let options = card["options"].as_array().map(Vec::as_slice).unwrap_or_default();
        let always = options.iter().any(|o| o["optionId"] == option_id && o["kind"] == "allow_always");
        if always {
            self.0.insert((session.to_owned(), scope(card)));
        }
    }

    /// A session the host has let go: its approvals go with it. An agent numbers its sessions from the start in each
    /// process, so a fresh conversation can come back on an id an earlier one was always allowed under.
    pub(super) fn forget(&mut self, session: &str) {
        self.0.retain(|(saved, _)| saved != session);
    }

    /// Every conversation starts over (a restarted agent, `newSession`): what the old ones were allowed answers
    /// nothing.
    pub(super) fn forget_all(&mut self) {
        self.0.clear();
    }

    /// The option that answers a request like `card` without asking, once the user always allowed its kind in
    /// `session`: allow always, else allow once. `None` asks the user.
    pub(super) fn pick<'a>(&self, session: &str, card: &Value, options: &'a [Value]) -> Option<&'a str> {
        if !self.0.contains(&(session.to_owned(), scope(card))) {
            return None;
        }
        ["allow_always", "allow_once"].iter().find_map(|kind| options.iter().find(|o| o["kind"] == *kind))?["optionId"]
            .as_str()
    }
}

/// What one Always allow covers: a kind of action (run a command, edit, read, search, fetch…). `other` holds every
/// tool the agent doesn't classify, an MCP server's included, so there it covers only the tool of that title.
fn scope(card: &Value) -> String {
    let kind = card["toolKind"].as_str().unwrap_or("other");
    if kind == "other" { format!("other:{}", card["title"].as_str().unwrap_or_default()) } else { kind.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn options() -> Vec<Value> {
        vec![
            json!({"optionId": "once", "kind": "allow_once"}),
            json!({"optionId": "always", "kind": "allow_always"}),
            json!({"optionId": "deny", "kind": "reject_once"}),
        ]
    }

    fn card(kind: &str, title: &str) -> Value {
        json!({"toolKind": kind, "title": title, "options": options()})
    }

    #[test]
    fn always_allow_answers_the_same_kind_in_the_same_session() {
        let mut allowed = AlwaysAllowed::default();
        allowed.note("s1", &card("execute", "ls"), "always");
        assert_eq!(allowed.pick("s1", &card("execute", "cargo test"), &options()), Some("always"));
        assert_eq!(allowed.pick("s1", &card("edit", "main.rs"), &options()), None, "another kind still asks");
        assert_eq!(allowed.pick("s2", &card("execute", "ls"), &options()), None, "another session still asks");
    }

    #[test]
    fn allow_once_and_deny_are_remembered_by_nobody() {
        let mut allowed = AlwaysAllowed::default();
        allowed.note("s1", &card("execute", "ls"), "once");
        allowed.note("s1", &card("read", "a.txt"), "deny");
        assert_eq!(allowed.pick("s1", &card("execute", "ls"), &options()), None);
        assert_eq!(allowed.pick("s1", &card("read", "a.txt"), &options()), None);
    }

    #[test]
    fn an_unclassified_tool_is_allowed_by_its_title_only() {
        let mut allowed = AlwaysAllowed::default();
        allowed.note("s1", &card("other", "mcp: send_email"), "always");
        assert_eq!(allowed.pick("s1", &card("other", "mcp: send_email"), &options()), Some("always"));
        assert_eq!(allowed.pick("s1", &card("other", "mcp: delete_repo"), &options()), None);
    }

    #[test]
    fn a_forgotten_session_asks_again_and_another_keeps_its_answer() {
        let mut allowed = AlwaysAllowed::default();
        allowed.note("s1", &card("execute", "ls"), "always");
        allowed.note("s2", &card("execute", "ls"), "always");
        allowed.forget("s1");
        assert_eq!(allowed.pick("s1", &card("execute", "cargo test"), &options()), None);
        assert_eq!(allowed.pick("s2", &card("execute", "cargo test"), &options()), Some("always"));
    }

    #[test]
    fn every_conversation_starting_over_forgets_every_approval() {
        let mut allowed = AlwaysAllowed::default();
        allowed.note("s1", &card("execute", "ls"), "always");
        allowed.note("s2", &card("edit", "a.txt"), "always");
        allowed.forget_all();
        assert_eq!(allowed.pick("s1", &card("execute", "ls"), &options()), None);
        assert_eq!(allowed.pick("s2", &card("edit", "a.txt"), &options()), None);
    }

    #[test]
    fn a_request_without_allow_always_takes_allow_once() {
        let mut allowed = AlwaysAllowed::default();
        allowed.note("s1", &card("execute", "ls"), "always");
        let once_only =
            [json!({"optionId": "yes", "kind": "allow_once"}), json!({"optionId": "no", "kind": "reject_once"})];
        assert_eq!(allowed.pick("s1", &card("execute", "pwd"), &once_only), Some("yes"));
        let deny_only = [json!({"optionId": "no", "kind": "reject_once"})];
        assert_eq!(allowed.pick("s1", &card("execute", "pwd"), &deny_only), None);
    }
}
