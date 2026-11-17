//! Who on the other computers may write to this computer's agents. The receiving computer checks
//! its own rules before it takes a letter, so none depends on the sender's goodwill: the first rule
//! that matches decides, and with no rules every member may write (the default). A rule allows,
//! denies, or lets the letter in under a ceiling (`auto`, `ask`): the app that takes it lets the
//! agent act for it no more freely than that.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Matches any computer or agent.
pub const ANY: &str = "*";
/// Enough for any one person's computers, and few enough to check on every letter.
pub const MAX_RULES: usize = 100;

/// What a sender may have an agent here do, the most trusted first (so the greater is the stricter).
/// `Auto` and `Ask` are ceilings on how the agent works on the letter, never a way to work more
/// freely than it is set to. Only an agent the app runs can be held to one, so a letter under a
/// ceiling to any other agent is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Taken as the agent is set to work.
    Allow,
    /// Taken; the agent does by itself only what it judges safe and asks its user about the rest.
    Auto,
    /// Taken; the agent asks its user before every change and command.
    Ask,
    Deny,
}

impl Access {
    /// The level as a rule spells it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Auto => "auto",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }

    /// Whether the agent has to be held to a ceiling for a letter at this level.
    pub fn is_ceiling(self) -> bool {
        matches!(self, Self::Auto | Self::Ask)
    }
}

/// `deny *@server → *`: letters from `from_agent` on `from_computer` to `to_agent` here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// A computer in the network, or `*`.
    pub from_computer: String,
    /// An agent on that computer, or `*`.
    pub from_agent: String,
    /// One of this computer's agents, or `*`.
    pub to_agent: String,
    pub access: Access,
}

impl Rule {
    /// `from` is `agent@computer`, either side `*`, or `*` alone for anyone.
    pub fn new(access: Access, from: &str, to: &str) -> Result<Self> {
        let (from_agent, from_computer) = match from.trim() {
            ANY => (ANY, ANY),
            from => from.rsplit_once('@').ok_or_else(|| anyhow::anyhow!("from is agent@computer, or *"))?,
        };
        let rule = Self {
            from_computer: from_computer.trim().into(),
            from_agent: from_agent.trim().into(),
            to_agent: to.trim().into(),
            access,
        };
        rule.check()?;
        Ok(rule)
    }

    fn check(&self) -> Result<()> {
        // An agent's address name is up to 40 characters of any script, as long as a letter carries.
        let parts = [
            (&self.from_computer, 64),
            (&self.from_agent, crate::wire::MAX_ADDRESS),
            (&self.to_agent, crate::wire::MAX_ADDRESS),
        ];
        for (part, max) in parts {
            if part.is_empty() || part.len() > max || part.contains(char::is_whitespace) {
                bail!("each part of a rule is a name or *, without spaces");
            }
        }
        Ok(())
    }

    fn matches(&self, computer: &str, agent: &str, to: &str) -> bool {
        let fits = |pattern: &str, name: &str| pattern == ANY || pattern.eq_ignore_ascii_case(name);
        fits(&self.from_computer, computer) && fits(&self.from_agent, agent) && fits(&self.to_agent, to)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}@{} → {}", self.access.word(), self.from_agent, self.from_computer, self.to_agent)
    }
}

impl FromStr for Access {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "allow" => Ok(Self::Allow),
            "auto" => Ok(Self::Auto),
            "ask" => Ok(Self::Ask),
            "deny" => Ok(Self::Deny),
            _ => bail!("a rule is allow, auto, ask or deny"),
        }
    }
}

/// Checks a whole list before it replaces the old one.
pub fn check_all(rules: &[Rule]) -> Result<()> {
    if rules.len() > MAX_RULES {
        bail!("at most {MAX_RULES} rules");
    }
    rules.iter().try_for_each(Rule::check)
}

/// How `agent` on `computer` may write to `to` here: the first rule that matches decides, and
/// without one `default` does.
pub fn access(rules: &[Rule], default: Access, computer: &str, agent: &str, to: &str) -> Access {
    rules.iter().find(|r| r.matches(computer, agent, to)).map_or(default, |r| r.access)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(access: Access, from: &str, to: &str) -> Rule {
        Rule::new(access, from, to).expect("a valid rule")
    }

    #[test]
    fn the_first_match_decides_and_no_rules_allow_everyone() {
        assert_eq!(access(&[], Access::Allow, "server", "lead", "reviewer"), Access::Allow);
        assert_eq!(access(&[], Access::Auto, "server", "lead", "reviewer"), Access::Auto, "the app's default");
        let rules = [rule(Access::Allow, "lead@server", "reviewer"), rule(Access::Deny, "*@server", "*")];
        assert_eq!(access(&rules, Access::Allow, "server", "lead", "reviewer"), Access::Allow);
        assert_eq!(
            access(&rules, Access::Allow, "server", "lead", "coder"),
            Access::Deny,
            "only the reviewer was allowed"
        );
        assert_eq!(
            access(&rules, Access::Allow, "Server", "other", "reviewer"),
            Access::Deny,
            "computer names ignore case"
        );
        assert_eq!(access(&rules, Access::Allow, "laptop", "lead", "coder"), Access::Allow, "no rule for the laptop");
        assert_eq!(access(&[rule(Access::Deny, "*", "*")], Access::Allow, "laptop", "a", "b"), Access::Deny);
    }

    #[test]
    fn a_rule_can_set_a_ceiling() {
        let rules =
            [rule(Access::Ask, "*@cloud", "*"), rule(Access::Auto, "*@mini", "*"), rule(Access::Allow, "*", "*")];
        assert_eq!(access(&rules, Access::Allow, "cloud", "lead", "reviewer"), Access::Ask);
        assert_eq!(access(&rules, Access::Allow, "mini", "lead", "reviewer"), Access::Auto);
        assert_eq!(access(&rules, Access::Allow, "desk", "lead", "reviewer"), Access::Allow);
        assert!(Access::Allow < Access::Auto && Access::Auto < Access::Ask && Access::Ask < Access::Deny);
        assert!(Access::Auto.is_ceiling() && Access::Ask.is_ceiling() && !Access::Allow.is_ceiling());
        assert_eq!("auto".parse::<Access>().expect("a level"), Access::Auto);
        assert_eq!("ask".parse::<Access>().expect("a level"), Access::Ask);
        assert_eq!(rules[0].to_string(), "ask *@cloud → *");
        assert_eq!(serde_json::to_value(&rules[0]).expect("serializes")["access"], "ask");
    }

    #[test]
    fn rules_read_and_show_like_addresses() {
        let r = rule(Access::Deny, "*@server", "*");
        assert_eq!((r.from_agent.as_str(), r.from_computer.as_str()), ("*", "server"));
        assert_eq!(r.to_string(), "deny *@server → *");
        assert!(Rule::new(Access::Allow, "lead", "x").is_err(), "a bare agent needs its computer");
        assert!(Rule::new(Access::Allow, "a b@c", "x").is_err());
        let long = "審".repeat(40);
        assert!(Rule::new(Access::Allow, &format!("{long}@desk"), &long).is_ok(), "any script's address names");
        assert!(check_all(&vec![r; MAX_RULES + 1]).is_err());
        let json = serde_json::to_value(rule(Access::Allow, "lead@desk", "reviewer")).expect("serializes");
        assert_eq!(json["access"], "allow");
        assert_eq!(json["fromComputer"], "desk");
    }
}
