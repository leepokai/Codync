//! What a bot's security profile (`agent::security_profile`) does when it is chosen or edited around.

use super::Hub;
use crate::store::BotConfig;

impl Hub {
    /// Applies the profile `cfg` names when it is new (chosen at creation, or different from the saved bot's), and drops
    /// the name when the person has since changed the rules or the Approvals setting away from it, so a bot never claims
    /// a profile it no longer follows.
    pub(super) fn settle_profile(before: Option<&BotConfig>, cfg: &mut BotConfig) {
        let Some(profile) = cfg.security_profile else { return };
        if before.and_then(|b| b.security_profile) != Some(profile) {
            profile.apply(cfg);
        } else if cfg.approval_rules != profile.rules() || cfg.permission != profile.permission() {
            cfg.security_profile = None;
        }
    }

    /// Sets the daily token limit the bot's profile asks for (`Full` leaves the bot's own).
    pub(super) fn set_profile_budget(&self, cfg: &BotConfig) {
        let Some(limit) = cfg.security_profile.and_then(crate::agent::security_profile::SecurityProfile::budget) else {
            return;
        };
        if let Err(error) = self.budget.set_limit(&self.store, &cfg.id, Some(limit)) {
            tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't set the profile's token limit");
        }
    }
}
