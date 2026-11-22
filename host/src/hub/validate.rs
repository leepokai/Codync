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
        && !codync_agent::backends::is_known(&cfg.backend)
    {
        bail!("unknown backend {}", cfg.backend);
    }
    // An id the agent listed (`codync_agent::modes`), sent back to it as is.
    if cfg.mode.as_deref().is_some_and(|m| m.trim().is_empty() || m.len() > 64) {
        bail!("mode must be one of the agent's mode ids, or null for its default");
    }
    if !(1..=crate::store::MAX_THREADS).contains(&cfg.max_threads) {
        bail!("maxThreads is 1 to {}", crate::store::MAX_THREADS);
    }
    cfg.compaction.check().map_err(anyhow::Error::msg)?;
    crate::agent::fallback::check(cfg)?;
    // Advisor mode: the threads' agent and its fallbacks hold to the rules a bot's own do.
    if let Some(agent) = &cfg.thread_agent
        && agent.command.as_deref().map(str::trim).unwrap_or_default().is_empty()
        && !codync_agent::backends::is_known(&agent.backend)
    {
        bail!("unknown threads' agent {}", agent.backend);
    }
    crate::agent::fallback::check(&crate::agent::fallback::threads(cfg))
}
