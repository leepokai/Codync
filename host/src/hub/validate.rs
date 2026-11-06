//! What a bot's setup must hold before it's saved, whoever changes it (a client, a managing bot, the host).

use crate::store::BotConfig;
use anyhow::{Result, bail};

pub(crate) fn validate(cfg: &BotConfig) -> Result<()> {
    if cfg.name.trim().is_empty() {
        bail!("name is required");
    }
    if cfg.is_group() {
        return Ok(());
    }
    // The bot a call or shortcut that names none talks to stays in sight.
    if cfg.lead && cfg.hidden {
        bail!("the lead bot can't be hidden");
    }
    if !cfg.cwd.is_empty() && !std::path::Path::new(&cfg.cwd).is_dir() {
        bail!("workspace folder does not exist: {}", cfg.cwd);
    }
    if cfg.command.as_deref().map(str::trim).unwrap_or_default().is_empty()
        && !crate::agent::backends::is_known(&cfg.backend)
    {
        bail!("unknown backend {}", cfg.backend);
    }
    // An id the agent listed (`agent::modes`), sent back to it as is.
    if cfg.mode.as_deref().is_some_and(|m| m.trim().is_empty() || m.len() > 64) {
        bail!("mode must be one of the agent's mode ids, or null for its default");
    }
    crate::agent::fallback::check(cfg)
}
