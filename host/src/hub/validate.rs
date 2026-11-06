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
    crate::agent::fallback::check(cfg)
}
