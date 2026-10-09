//! Pinned Engram runtime. Each bot owns a separate database and stdio process.
//! Engram owns schema migrations and every write; Codync never writes its tables.

mod backup;
mod clear;
mod install;
mod migrate;
mod process;

use anyhow::{Result, bail};
use std::path::PathBuf;

pub(super) use backup::{export, import};
pub(super) use process::{call, clear, prepare, tools};

pub const VERSION: &str = "3.2.1";

pub fn directory(bot_id: &str) -> Result<PathBuf> {
    if bot_id.is_empty() || !bot_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        bail!("invalid bot id");
    }
    #[cfg(not(test))]
    let root = crate::service::data_dir();
    // Unit fixtures run several hosts with the same synthetic bot IDs on separate runtimes.
    // Keep their real native databases isolated from each other and from the user's bots.
    #[cfg(test)]
    let root = {
        static ROOT: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| {
            std::env::temp_dir().join(format!("codync-memory-unit-{}", uuid::Uuid::new_v4()))
        });
        ROOT.join(tokio::runtime::Handle::try_current()?.id().to_string())
    };
    Ok(root.join("bots").join(bot_id).join("engram"))
}

pub fn project(bot_id: &str) -> String {
    format!("codync-{bot_id}")
}
