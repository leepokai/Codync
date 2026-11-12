//! Stopping bots: one bot's running turn, or every bot's at once.

use super::{BotStatus, Hub};
use crate::agent::bot::Cmd;
use anyhow::Result;

impl Hub {
    /// Interrupts one bot's running turn (a group's room turns, for a group).
    pub fn stop_bot(self: &std::sync::Arc<Self>, bot: &str) -> Result<()> {
        match self.store.bot(bot)? {
            Some(row) if row.config.is_group() => crate::chat::group::stop(self, bot),
            // The terminal answers to its own interrupt, not to us.
            Some(row) if crate::external::is_external(&row.config) => {}
            _ => self.send_cmd(bot, Cmd::Stop)?,
        }
        Ok(())
    }

    /// Interrupts every bot that is working or waiting on the user, and says how many it stopped. Groups follow
    /// their members, so only bots are counted; one that can't be stopped doesn't keep the others running.
    pub fn stop_all(self: &std::sync::Arc<Self>) -> Result<usize> {
        let mut stopped = 0;
        for row in self.store.bots()? {
            let config = &row.config;
            if config.is_group()
                || crate::external::is_external(config)
                || self.runtime(&config.id).status == BotStatus::Idle
            {
                continue;
            }
            match self.stop_bot(&config.id) {
                Ok(()) => stopped += 1,
                Err(error) => {
                    tracing::warn!(bot = %config.id, error = format!("{error:#}"), "stop all: bot didn't stop");
                }
            }
        }
        Ok(stopped)
    }
}
