//! "Always allow" as a rule of the bot's own. When the person picks the agent's `allow_always` option on a card, the host
//! proposes the rule it means: the command (or the title) as the pattern and the tool kind. The person sees the pattern
//! and may edit it before it is saved. A command that chains others, or one nobody may run, never becomes a rule.
//!
//! Computers only: the phone apps do not offer "always" (Coucou's rule: a thumb on a phone should not grant a standing
//! permission). The host checks the rule, not who asks.

use super::approval_rules::{ApprovalRule, Decision, MAX_PATTERN, is_chained};
use serde::Serialize;
use serde_json::Value;

/// Why a card gives no rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Refusal {
    /// The card has no command or title to match.
    NothingToMatch,
    /// The command runs several commands: an allow rule never approves those.
    Chained,
    /// The command is one no bot may run.
    Forbidden,
}

impl Refusal {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NothingToMatch => "This request has nothing a rule could match.",
            Self::Chained => "It runs several commands, and an allow rule never approves those: allow it once instead.",
            Self::Forbidden => "No bot may run this command, so no rule can allow it.",
        }
    }
}

/// The rule an `allow_always` answer to `card` proposes: its command exactly, else its title, for its tool kind.
pub fn propose(card: &Value) -> Result<ApprovalRule, Refusal> {
    let subject = card["command"].as_str().filter(|c| !c.trim().is_empty()).or_else(|| card["title"].as_str());
    let pattern = subject.map(str::trim).filter(|s| !s.is_empty()).ok_or(Refusal::NothingToMatch)?;
    let kind = card["toolKind"].as_str().filter(|k| !k.is_empty()).map(str::to_owned);
    let rule = ApprovalRule { pattern: pattern.to_owned(), kind, decision: Decision::Allow };
    check(&rule)?;
    Ok(rule)
}

/// Whether `rule` (as proposed or as the person edited it) may be saved as an "always" rule.
pub fn check(rule: &ApprovalRule) -> Result<(), Refusal> {
    let pattern = rule.pattern.trim();
    if pattern.is_empty() || pattern.chars().count() > MAX_PATTERN || rule.decision != Decision::Allow {
        return Err(Refusal::NothingToMatch);
    }
    if is_chained(pattern) {
        return Err(Refusal::Chained);
    }
    // A wildcard stands for any text, none included: try the pattern as it is (a literal star), with each `*` as nothing
    // and with each `*` as a word.
    let forbidden = [pattern.to_owned(), pattern.replace('*', ""), pattern.replace('*', "x")];
    if forbidden.iter().any(|command| super::blocklist::blocked(command).is_some()) {
        return Err(Refusal::Forbidden);
    }
    Ok(())
}

/// `rules` with `rule` added at the end, unless an identical rule is already there.
pub fn added(rules: &[ApprovalRule], rule: ApprovalRule) -> Vec<ApprovalRule> {
    let mut next = rules.to_vec();
    if !next.iter().any(|r| r == &rule) {
        next.push(rule);
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_command_becomes_its_own_pattern_and_kind() {
        let rule = propose(&json!({"command": "git status --short", "title": "Run git", "toolKind": "execute"}))
            .expect("a rule");
        assert_eq!(
            (rule.pattern.as_str(), rule.kind.as_deref(), rule.decision),
            ("git status --short", Some("execute"), Decision::Allow)
        );
    }

    #[test]
    fn a_request_without_a_command_uses_its_title() {
        let rule = propose(&json!({"command": null, "title": "Edit src/main.rs", "toolKind": "edit"})).expect("a rule");
        assert_eq!((rule.pattern.as_str(), rule.kind.as_deref()), ("Edit src/main.rs", Some("edit")));
        assert_eq!(propose(&json!({"command": " ", "title": ""})), Err(Refusal::NothingToMatch));
    }

    #[test]
    fn a_chained_or_forbidden_command_gives_no_rule() {
        assert_eq!(propose(&json!({"command": "git status && rm build"})), Err(Refusal::Chained));
        assert_eq!(propose(&json!({"command": "rm -rf /"})), Err(Refusal::Forbidden));
        assert_eq!(propose(&json!({"command": "sudo apt install x"})), Err(Refusal::Forbidden));
    }

    #[test]
    fn an_edited_pattern_is_checked_again() {
        let ok = |p: &str| check(&ApprovalRule { pattern: p.into(), kind: None, decision: Decision::Allow });
        assert_eq!(ok("git status*"), Ok(()));
        assert_eq!(ok("git status*; ls"), Err(Refusal::Chained));
        assert_eq!(ok("rm -rf /*"), Err(Refusal::Forbidden), "a wildcard doesn't hide a forbidden command");
        assert_eq!(ok("  "), Err(Refusal::NothingToMatch));
        let deny = ApprovalRule { pattern: "x".into(), kind: None, decision: Decision::Deny };
        assert_eq!(check(&deny), Err(Refusal::NothingToMatch), "\"always\" only ever allows");
    }

    #[test]
    fn the_same_rule_is_not_added_twice() {
        let rule = ApprovalRule { pattern: "ls".into(), kind: None, decision: Decision::Allow };
        let once = added(&[], rule.clone());
        assert_eq!(once.len(), 1);
        assert_eq!(added(&once, rule).len(), 1);
    }
}
