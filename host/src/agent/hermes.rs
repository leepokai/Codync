//! Hermes execution is an owned, pinned sidecar; native profiles retain their credentials.

mod health;
mod installation;
mod settings;

pub(crate) use installation::Installation;
pub(crate) use settings::{Settings, api, load};

use crate::{agent::registry, hub::Hub, store::BotConfig};
use anyhow::Result;
use std::sync::Arc;

pub(crate) struct Launch {
    pub command: registry::Cmd,
    pub settings: String,
}

pub(crate) async fn launch(hub: &Arc<Hub>, cfg: &BotConfig) -> Result<Option<Launch>> {
    if cfg.backend != "hermes" {
        return Ok(None);
    }
    let Some(saved) = load(&hub.store, &cfg.id)?.filter(|saved| saved.config.enabled) else {
        return Ok(None);
    };
    let installation = Installation::resolve(&saved.config).await?;
    let script = installation.prepare_owner().await?;
    Ok(Some(Launch {
        settings: serde_json::to_string(&saved.config)?,
        command: registry::Cmd {
            program: crate::shell::quote(&installation.python.to_string_lossy()),
            args: format!(
                "{} --hermes-root {}",
                crate::shell::quote(&script.to_string_lossy()),
                crate::shell::quote(&installation.root.to_string_lossy())
            ),
        },
    }))
}
