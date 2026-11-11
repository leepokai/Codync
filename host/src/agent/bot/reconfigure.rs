//! Applying bot configuration after the running turn, through the existing restart path.

use super::Actor;
use crate::agent::own_memory;
use crate::chat::memory;
use crate::store::BotConfig;

impl Actor {
    pub(super) async fn reconfigure(&mut self, base: BotConfig) {
        let removed: Vec<_> = self.base.connectors.iter().filter(|id| !base.connectors.contains(id)).cloned().collect();
        for connection in removed {
            // Non-native connections have no nativeFence and are unaffected. Revoke
            // immediately; the next worker/actor admission also checks the new binding.
            self.revoke_native_inputs(&connection, None, None).await;
        }
        let old = std::mem::replace(&mut self.base, base);
        // On its own agent the bot takes the new one; on a fallback it keeps that agent (its step may have
        // moved) until `settle_chain` moves it off one it no longer uses.
        let cfg = if crate::agent::fallback::step_of(&old, &self.cfg.backend) == Some(0) {
            crate::agent::fallback::effective(&self.base, 0)
        } else {
            super::fallback::running(&self.base, &self.cfg)
        };
        // The harness reads its own memory's switch when it starts: an agent that can resume its session
        // restarts into it, one that can't starts afresh.
        let memory_changed = own_memory::kept(&cfg) != own_memory::kept(&self.cfg);
        let resumes = self.conn.as_ref().is_none_or(|c| c.sessions.load);
        let account_changed = cfg.account != self.cfg.account;
        let restart = account_changed
            || cfg.backend != self.cfg.backend
            || cfg.command != self.cfg.command
            || cfg.cwd != self.cfg.cwd
            || cfg.model != self.cfg.model
            || cfg.model_options != self.cfg.model_options
            || (memory_changed && !resumes);
        self.tools_changed |= cfg.connectors != self.cfg.connectors
            || cfg.computer != self.cfg.computer
            || cfg.manage_team != self.cfg.manage_team
            || memory_changed;
        // Live sessions switch before their next turn (see `fast_mode`).
        let fast_changed = cfg.fast_mode != self.cfg.fast_mode;
        // Name, description and skill changes reach the agent as a profile update
        // on the next message (see `context`).
        self.cfg = cfg;
        if fast_changed {
            self.fast_mode_changed();
        }
        if memory_changed {
            // Claude Code's notes come or go on the memory screen.
            memory::screen_changed(&self.hub, &self.cfg.id);
        }
        // The running turn finishes first: it may be the bot changing its own model (`update_bot`).
        if restart && self.turn.is_some() {
            self.restart_after_turn = true;
        } else if restart {
            self.restart_agent().await;
        }
        self.settle_chain(&old).await;
    }
}
