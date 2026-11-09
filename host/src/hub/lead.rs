//! Which bot is the computer's lead (`chat::lead`): one at most, picked by the user.

use super::{Hub, validate};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};

impl Hub {
    /// Makes `id` the computer's one lead in place of the one before (`chat::lead::set`): checked and saved in one
    /// transaction under one `emit_lock`, so two picks never leave two leads (or none), and a bot deleted or hidden
    /// meanwhile never leads. Each bot whose role changed is reconfigured under the same lock, so its next message
    /// tells it (`context::profile_update`) and a later pick's settings always reach it after this one's.
    pub(crate) fn make_lead(&self, id: &str) -> Result<()> {
        for bot in self.save_lead(id)? {
            self.emit_bot(&bot);
        }
        Ok(())
    }

    /// Sets `id`'s lead flag and clears every other live bot's, then reconfigures each changed bot; returns their ids.
    fn save_lead(&self, id: &str) -> Result<Vec<String>> {
        let _g = self.emit_lock.locked();
        let bots: Vec<BotConfig> = self.store.bots()?.into_iter().filter(|r| !r.deleted).map(|r| r.config).collect();
        let mut target = bots.iter().find(|b| b.id == id).cloned().ok_or_else(|| anyhow!("unknown bot"))?;
        if target.is_group() {
            bail!("a group can't be the lead");
        }
        target.lead = true;
        // Refuses a hidden bot.
        validate(&target)?;
        let changed: Vec<BotConfig> = bots
            .into_iter()
            .filter(|cfg| cfg.lead != (cfg.id == id))
            .map(|mut cfg| {
                cfg.lead = cfg.id == id;
                cfg
            })
            .collect();
        self.store.save_bots(&changed)?;
        Ok(changed
            .into_iter()
            .map(|cfg| {
                let bot = cfg.id.clone();
                if let Err(error) = self.send_cmd(&bot, Cmd::Reconfigure(Box::new(cfg))) {
                    tracing::warn!(bot, error = format!("{error:#}"), "couldn't tell a bot its lead role changed");
                }
                bot
            })
            .collect())
    }
}
