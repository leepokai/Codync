//! What a bot's setup must hold before it's saved, whoever changes it (a client, a managing bot, the host).

use crate::store::BotConfig;
use anyhow::{Result, bail};

pub(crate) fn validate(cfg: &BotConfig) -> Result<()> {
    if let Some(id) = &cfg.account {
        if cfg.is_group() {
            bail!("Groups cannot select an agent account");
        }
        crate::agent::account::validate(id, &cfg.backend)?;
    }
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
    if let Some(photo) = &cfg.avatar_photo
        && !photo.is_empty()
    {
        let ok = ["data:image/png;base64,", "data:image/jpeg;base64,", "data:image/webp;base64,"]
            .iter()
            .any(|prefix| photo.starts_with(prefix));
        if !ok || photo.len() > 400_000 {
            bail!("Use a PNG, JPEG or WebP image of at most 300 KB");
        }
    }
    crate::agent::approval_rules::validate(&cfg.approval_rules)?;
    if !cfg.cwd.is_empty() && !std::path::Path::new(&cfg.cwd).is_dir() {
        bail!("workspace folder does not exist: {}", cfg.cwd);
    }
    if cfg.command.as_deref().map(str::trim).unwrap_or_default().is_empty()
        && !crate::agent::backends::is_known(&cfg.backend)
        && !crate::external::is_external(cfg)
    {
        bail!("unknown backend {}", cfg.backend);
    }
    // An id the agent listed (`agent::modes`), sent back to it as is.
    if cfg.mode.as_deref().is_some_and(|m| m.trim().is_empty() || m.len() > 64) {
        bail!("mode must be one of the agent's mode ids, or null for its default");
    }
    crate::agent::fallback::check(cfg)
}
