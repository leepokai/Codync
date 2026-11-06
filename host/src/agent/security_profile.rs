//! Security profiles: one choice when a bot is made (or later) that sets how much the bot may do without asking. A profile
//! is not a new mechanism: it is the Approvals setting (`permission`), a list of approval rules (`approval_rules`) and a
//! daily token limit (`budget`), written for the person. The forbidden commands (`blocklist`) apply under every profile,
//! `Full` included.
//!
//! A profile governs approvals. It does not confine folders: the agent behind a bot runs over ACP with its own sandbox and
//! settings, and Codync cannot draw a path boundary around it.
//!
//! Idea from Mercury Agent's `src/bots/permission-tiers.ts` (MIT; `THIRD-PARTY-NOTICES.md`): read-only, builder, operator
//! and full tiers. The code and the mapping to approval rules are Codync's own.

use super::approval_rules::{ApprovalRule, Decision};
use crate::store::{BotConfig, Permission};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecurityProfile {
    /// Reads and searches; asks before changing a file; no shell commands.
    ReadOnly,
    /// Also edits files; asks before deleting or moving; no shell commands.
    Builder,
    /// Also moves files and runs plain shell commands; asks before deleting.
    Operator,
    /// Approves everything, except the forbidden commands.
    Full,
}

/// A rule for each of `kinds`.
fn rules(decision: Decision, kinds: &[&str]) -> Vec<ApprovalRule> {
    kinds.iter().map(|kind| ApprovalRule { pattern: "*".into(), kind: Some((*kind).into()), decision }).collect()
}

const READS: [&str; 4] = ["read", "search", "fetch", "think"];

impl SecurityProfile {
    pub const ALL: [Self; 4] = [Self::ReadOnly, Self::Builder, Self::Operator, Self::Full];

    pub const fn id(self) -> &'static str {
        match self {
            Self::ReadOnly => "readOnly",
            Self::Builder => "builder",
            Self::Operator => "operator",
            Self::Full => "full",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::ReadOnly => "Read-only",
            Self::Builder => "Builder",
            Self::Operator => "Operator",
            Self::Full => "Full access",
        }
    }

    /// One sentence for the person choosing.
    pub const fn summary(self) -> &'static str {
        match self {
            Self::ReadOnly => "Reads and searches on its own. Asks before changing a file. Never runs commands.",
            Self::Builder => "Also edits files on its own. Asks before deleting or moving them. Never runs commands.",
            Self::Operator => {
                "Also moves files and runs plain commands on its own. Asks before deleting. Commands that chain others still ask."
            }
            Self::Full => "Does everything without asking, except the commands no bot may run.",
        }
    }

    /// The Approvals setting the profile leaves: what the rules do not cover.
    pub const fn permission(self) -> Permission {
        match self {
            Self::Full => Permission::Auto,
            _ => Permission::Ask,
        }
    }

    /// The rules, first match first.
    pub fn rules(self) -> Vec<ApprovalRule> {
        let mut out = rules(Decision::Allow, &READS);
        match self {
            Self::ReadOnly => {
                out.extend(rules(Decision::Ask, &["edit", "delete", "move"]));
                out.extend(rules(Decision::Deny, &["execute"]));
            }
            Self::Builder => {
                out.extend(rules(Decision::Allow, &["edit"]));
                out.extend(rules(Decision::Ask, &["delete", "move"]));
                out.extend(rules(Decision::Deny, &["execute"]));
            }
            Self::Operator => {
                out.extend(rules(Decision::Allow, &["edit", "move", "execute"]));
                out.extend(rules(Decision::Ask, &["delete"]));
            }
            Self::Full => return Vec::new(),
        }
        out
    }

    /// The daily token limit the profile sets, when it sets one; `Full` leaves the bot's own.
    pub const fn budget(self) -> Option<u64> {
        match self {
            Self::ReadOnly => Some(300_000),
            Self::Builder => Some(1_000_000),
            Self::Operator => Some(3_000_000),
            Self::Full => None,
        }
    }

    /// Makes `cfg` this profile: the Approvals setting and the rules are replaced.
    pub fn apply(self, cfg: &mut BotConfig) {
        cfg.permission = self.permission();
        cfg.approval_rules = self.rules();
        cfg.security_profile = Some(self);
    }

    /// What a client shows to choose: the sentence, and what it will set.
    pub fn describe(self) -> Value {
        let rules: Vec<Value> = self.rules().iter().map(|r| json!({"kind": r.kind, "decision": r.decision})).collect();
        json!({
            "id": self.id(),
            "title": self.title(),
            "summary": self.summary(),
            "permission": self.permission(),
            "budget": self.budget(),
            "rules": rules,
        })
    }
}

/// The profiles in the order a person picks among them.
pub fn describe_all() -> Value {
    Value::Array(SecurityProfile::ALL.iter().map(|p| p.describe()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::approval_rules::{Outcome, evaluate};

    fn say(profile: SecurityProfile, kind: &str, command: Option<&str>) -> Outcome {
        evaluate(&profile.rules(), &[], profile.permission(), kind, command, "A request").outcome
    }

    fn decision(outcome: &Outcome) -> Option<Decision> {
        match outcome {
            Outcome::Rule { decision, .. } => Some(*decision),
            _ => None,
        }
    }

    #[test]
    fn read_only_asks_before_editing_and_refuses_commands() {
        let p = SecurityProfile::ReadOnly;
        assert_eq!(decision(&say(p, "read", None)), Some(Decision::Allow));
        assert_eq!(decision(&say(p, "edit", None)), Some(Decision::Ask), "asks before editing a file");
        assert_eq!(decision(&say(p, "execute", Some("ls"))), Some(Decision::Deny), "no commands");
    }

    #[test]
    fn each_step_up_allows_more_and_nothing_steps_back() {
        let (b, o) = (SecurityProfile::Builder, SecurityProfile::Operator);
        assert_eq!(decision(&say(b, "edit", None)), Some(Decision::Allow));
        assert_eq!(decision(&say(b, "execute", Some("ls"))), Some(Decision::Deny));
        assert_eq!(decision(&say(o, "execute", Some("ls"))), Some(Decision::Allow));
        assert_eq!(decision(&say(o, "delete", None)), Some(Decision::Ask));
        // An operator's allowed shell still asks for a chained command.
        assert_eq!(say(o, "execute", Some("ls && make")), Outcome::Setting { permission: Permission::Ask });
    }

    #[test]
    fn full_approves_everything_except_what_no_bot_may_run() {
        let f = SecurityProfile::Full;
        assert_eq!(say(f, "execute", Some("make")), Outcome::Setting { permission: Permission::Auto });
        assert!(
            matches!(say(f, "execute", Some("rm -rf /")), Outcome::Forbidden { .. }),
            "forbidden under every profile"
        );
        for p in SecurityProfile::ALL {
            assert!(matches!(say(p, "execute", Some("sudo reboot")), Outcome::Forbidden { .. }), "{p:?}");
        }
    }

    #[test]
    fn applying_a_profile_replaces_the_setting_and_the_rules() {
        let mut cfg: BotConfig =
            serde_json::from_value(json!({"id": "b", "name": "B", "backend": "claude"})).expect("bot");
        cfg.approval_rules = vec![ApprovalRule { pattern: "x".into(), kind: None, decision: Decision::Allow }];
        SecurityProfile::Builder.apply(&mut cfg);
        assert_eq!((cfg.permission, cfg.security_profile), (Permission::Ask, Some(SecurityProfile::Builder)));
        assert_eq!(cfg.approval_rules, SecurityProfile::Builder.rules(), "the old rules are gone");
        SecurityProfile::Full.apply(&mut cfg);
        assert_eq!((cfg.permission, cfg.approval_rules.len()), (Permission::Auto, 0));
    }

    #[test]
    fn the_description_tells_a_client_everything_it_shows() {
        let all = describe_all();
        let ids: Vec<&str> = all.as_array().expect("a list").iter().filter_map(|p| p["id"].as_str()).collect();
        assert_eq!(ids, ["readOnly", "builder", "operator", "full"]);
        assert_eq!(all[0]["permission"], "ask");
        assert_eq!(all[0]["budget"], 300_000);
        assert!(all[3]["budget"].is_null() && all[3]["rules"].as_array().is_some_and(Vec::is_empty));
    }
}
