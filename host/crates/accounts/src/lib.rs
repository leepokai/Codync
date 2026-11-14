//! More than one login to the same coding agent on one computer, so several agents can run at once on different
//! accounts.
//!
//! An account is a folder (its slot) the agent keeps its login in, and the agent's own variables point a process
//! at it. The login the computer already has stays where it is. The agent's own CLI signs in and out: nothing here
//! reads a token to use it, stores one, or asks a provider who an account is.
//!
//! The crate holds what is true of the agents, not of an app: [`Agent`] says how each one keeps a login in a
//! folder, [`Agent::share`] gives a slot the user's own setup, and [`Account`] is the record an app keeps. Where
//! the records are saved, how a sign-in is shown and which processes run are the app's.

mod agents;
mod identity;
mod slot;

pub use agents::{Agent, SignIn, SignedIn, agent, agents};
pub use identity::jwt_claims;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The longest name a user may give an account.
pub const LABEL_MAX: usize = 60;

/// A name the user typed, as it is kept: trimmed, `None` when empty (the account goes by who it signs in again).
pub fn label(typed: &str) -> Result<Option<String>, Error> {
    let name = typed.trim();
    if name.chars().count() > LABEL_MAX {
        return Err(Error::LongName);
    }
    Ok((!name.is_empty()).then(|| name.to_owned()))
}

/// Set for an agent on an account, so its helpers know they don't report for the computer's own login.
pub const ACCOUNT_VAR: &str = "CODYNC_AGENT_ACCOUNT";

/// One more login to an agent, beside the one the computer already has.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    /// The agent it is an account of ([`Agent::id`]).
    #[serde(rename = "backend")]
    pub agent: String,
    /// The name the user gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Who its agent says is signed in, where the agent's files say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Its place among the agent's logins, the computer's own being the first: "Account 2" where nothing names it.
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub signed_in: bool,
}

impl Account {
    /// A new, signed-out account of agent `agent`, numbered after `others` (the accounts there are).
    pub fn new(id: String, agent: &str, others: &[Account]) -> Self {
        let last = others.iter().filter(|a| a.agent == agent).map(|a| a.number).max().unwrap_or(1);
        Self { id, agent: agent.to_owned(), label: None, email: None, number: last.max(1) + 1, signed_in: false }
    }

    /// What to call it: the name the user gave it, else, once it's signed in, who it signs in or its number. Not
    /// for analytics.
    pub fn name(&self) -> Option<String> {
        if self.label.is_some() || !self.signed_in {
            return self.label.clone();
        }
        Some(self.email.clone().unwrap_or_else(|| self.numbered()))
    }

    /// [`Self::name`], or its number while nothing names it: what to say of it in a sentence.
    pub fn called(&self) -> String {
        self.name().unwrap_or_else(|| self.numbered())
    }

    fn numbered(&self) -> String {
        format!("Account {}", self.number.max(2))
    }

    /// Its slot under `root`. The id may come from a file anyone can edit, so it must be one plain folder name.
    pub fn slot(&self, root: &Path) -> Result<PathBuf, Error> {
        let agent = agent(&self.agent).ok_or_else(|| Error::NoAccounts(self.agent.clone()))?;
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(Error::InvalidId);
        }
        Ok(root.join(agent.id).join(&self.id))
    }
}

#[derive(Debug)]
pub enum Error {
    /// The agent keeps one login only.
    NoAccounts(String),
    /// An account id that isn't one folder name.
    InvalidId,
    /// A name longer than [`LABEL_MAX`].
    LongName,
    /// A file or folder of the slot couldn't be read, written or linked.
    Slot { doing: &'static str, path: PathBuf, source: std::io::Error },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAccounts(agent) => write!(f, "{agent} can't have more than one account"),
            Self::InvalidId => f.write_str("that account's id isn't valid"),
            Self::LongName => write!(f, "an account's name is at most {LABEL_MAX} characters"),
            Self::Slot { doing, path, source } => write!(f, "{doing} {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Slot { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, agent: &str) -> Account {
        Account { id: id.into(), agent: agent.into(), label: None, email: None, number: 0, signed_in: false }
    }

    #[test]
    fn a_slot_is_one_folder_under_its_agent() {
        let root = Path::new("/data/agent-accounts");
        assert_eq!(account("a1", "claude").slot(root).unwrap(), root.join("claude").join("a1"));
        assert!(matches!(account("../../x", "claude").slot(root), Err(Error::InvalidId)));
        assert!(matches!(account("", "codex").slot(root), Err(Error::InvalidId)));
        assert!(matches!(account("a1", "cursor").slot(root), Err(Error::NoAccounts(_))));
    }

    #[test]
    fn a_typed_name_is_trimmed_and_an_empty_one_clears_it() {
        assert_eq!(label("  Work ").unwrap().as_deref(), Some("Work"));
        assert_eq!(label("   ").unwrap(), None);
        assert!(matches!(label(&"x".repeat(LABEL_MAX + 1)), Err(Error::LongName)));
    }

    #[test]
    fn an_account_goes_by_who_it_signs_in_or_its_number() {
        let first = Account::new("a1".into(), "claude", &[]);
        assert_eq!((first.number, first.name()), (2, None), "the computer's own login is the first");
        assert_eq!(first.called(), "Account 2", "signed out, it still has something to go by");
        let others = [first, account("b1", "codex")];
        let mut second = Account::new("a2".into(), "claude", &others);
        assert_eq!(second.number, 3);
        second.signed_in = true;
        assert_eq!(second.name().as_deref(), Some("Account 3"));
        second.email = Some("work@example.com".into());
        assert_eq!(second.name().as_deref(), Some("work@example.com"));
        second.signed_in = false;
        second.label = Some("Work".into());
        assert_eq!(second.name().as_deref(), Some("Work"), "its own name, signed in or not");
        // One saved before accounts had numbers.
        let old = Account { signed_in: true, ..account("o", "claude") };
        assert_eq!(old.name().as_deref(), Some("Account 2"));
    }
}
