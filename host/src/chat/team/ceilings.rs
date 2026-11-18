//! The ceiling a network rule holds a bot's work to while it works on another computer's text
//! ([ceilings](../../../../docs/features/codync-network.md#ceilings-auto-and-ask)): its agent's mode is no freer than
//! it, nothing is approved for it, and it calls only the host tools that read or answer the user.

use crate::LockExt;
use crate::store::{BotConfig, Permission};
use anyhow::{Result, bail};
use codync_network::Access;
use std::collections::HashMap;
use std::sync::Mutex;

/// The only host tools a bot under a ceiling may call: each reads, or speaks to the user in this turn. Anything else
/// (a routine, background work, a follow-up, a message to another bot, the bot's instructions, skills, fallbacks or
/// memory, a dev server, the team) could act later or elsewhere without the ceiling, so it is refused, and so is any
/// tool added after this list: a new tool is held until it is listed here.
const UNDER_CEILING: [&str; 17] = [
    "send_message",
    "send_file",
    "request_decision",
    "react",
    "publish_artifact",
    "list_artifacts",
    "list_agents",
    "list_dev_servers",
    "dev_server_logs",
    "work_status",
    "list_routines",
    "read_progress",
    "list_bots",
    "list_team",
    "search_memory",
    "search_history",
    "read_memory",
];

/// Each bot's ceiling, keyed by the bot whose turn runs (a task's own id for a task); a bot under none has no entry.
#[derive(Default)]
pub struct Ceilings(Mutex<HashMap<String, Access>>);

impl Ceilings {
    /// What this computer's network rules allow the sender of the text `bot` works on next (a request, or replies).
    /// It stands until the next thing in the bot's queue, past the turn's end, so work the agent goes on with after
    /// a turn under a ceiling is held to it too.
    pub fn set(&self, bot: &str, access: Access) {
        let mut ceilings = self.0.locked();
        if access.is_ceiling() {
            ceilings.insert(bot.to_owned(), access);
        } else {
            ceilings.remove(bot);
        }
    }

    /// The ceiling `bot` works under, `Allow` under none.
    pub fn access(&self, bot: &str) -> Access {
        self.0.locked().get(bot).copied().unwrap_or(Access::Allow)
    }

    /// Whether `bot` works under a network rule's ceiling.
    pub fn holds(&self, bot: &str) -> bool {
        self.access(bot).is_ceiling()
    }

    /// Refuses `tool` to a bot under a ceiling unless it is one of [`UNDER_CEILING`].
    pub fn check_tool(&self, bot: &str, tool: &str) -> Result<()> {
        if !UNDER_CEILING.contains(&tool) && self.holds(bot) {
            bail!(
                "{tool} isn't available in this turn: it works on text from a computer the user trusts less, and only \
                 reading and answering the user are. Tell the user what you would do instead."
            );
        }
        Ok(())
    }

    /// Whether what `bot` asks for is approved without a card: under Approve automatically, unless a ceiling holds
    /// its work, which asks whatever the bot's Approvals.
    pub fn approves_by_itself(&self, bot: &BotConfig) -> bool {
        bot.permission == Permission::Auto && !self.holds(&bot.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tool a bot under a ceiling may call is one a host server offers, so a rename can't leave the list
    /// holding a name nothing answers to while the renamed tool is refused.
    #[test]
    fn the_tools_allowed_under_a_ceiling_exist() {
        let offered: Vec<String> = [
            crate::mcp::chat_tools(),
            crate::routines::tools(),
            crate::chat::team::tools(),
            crate::chat::manage::tools(),
            crate::chat::memory::tools(),
        ]
        .iter()
        .flat_map(|t| t.as_array().cloned().unwrap_or_default())
        .filter_map(|t| t["name"].as_str().map(str::to_owned))
        .collect();
        for tool in UNDER_CEILING {
            assert!(offered.iter().any(|t| t == tool), "{tool} isn't a host tool");
        }
    }

    #[test]
    fn a_ceiling_holds_every_tool_but_reading_and_answering_until_it_is_lifted() {
        let ceilings = Ceilings::default();
        ceilings.set("a", Access::Ask);
        let refused = ceilings.check_tool("a", "message_bot").expect_err("its text would travel on");
        assert!(refused.to_string().contains("a computer the user trusts less"), "{refused}");
        assert!(ceilings.check_tool("a", "a_tool_added_later").is_err(), "held until it is listed");
        let refused = ceilings.check_tool("a", "save_routine").expect_err("it would run without the ceiling");
        assert!(refused.to_string().contains("save_routine isn't available"), "{refused}");
        assert!(ceilings.check_tool("a", "list_routines").is_ok(), "reading changes nothing");
        assert!(ceilings.check_tool("b", "save_routine").is_ok(), "another bot's turn isn't held");
        ceilings.set("a", Access::Allow);
        assert!(!ceilings.holds("a"));
        assert!(ceilings.check_tool("a", "save_routine").is_ok());
    }
}
