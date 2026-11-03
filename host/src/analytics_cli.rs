//! `codync-host analytics on|off`: the computer owner's usage analytics choice from the command
//! line. The Windows installer asks with it (docs/features/analytics.md).

use crate::local_api::{NotRunning, local_call};
use anyhow::{Context, Result};
use serde_json::json;

#[derive(clap::Args)]
pub struct AnalyticsArgs {
    /// Whether this computer shares usage analytics.
    #[arg(value_enum)]
    sharing: Sharing,
    #[arg(long, default_value_t = crate::service::DEFAULT_PORT)]
    port: u16,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Sharing {
    On,
    Off,
}

pub async fn run(args: AnalyticsArgs) -> Result<()> {
    let on = matches!(args.sharing, Sharing::On);
    let state = if on { "on" } else { "off" };
    // A running host keeps the setting in memory, so it has to hear the change.
    match local_call(args.port, "setAnalytics", json!({ "enabled": on })).await {
        Ok(_) => println!("Usage analytics {state}."),
        Err(e) if e.is::<NotRunning>() => {
            let port = args.port;
            tokio::task::spawn_blocking(move || {
                // Held while saving, so no host starts on the old setting meanwhile.
                let _lock = crate::service::lock_host()
                    .with_context(|| format!("codync-host is running here, but not on port {port}: pass its --port"))?;
                let (store, _, _) = crate::open_store()?;
                crate::analytics::save_choice(&store, on)
            })
            .await??;
            println!("Usage analytics {state} from codync-host's next start.");
        }
        Err(e) => return Err(e),
    }
    Ok(())
}
