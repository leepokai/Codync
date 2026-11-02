//! Background services started alongside the host API.

use crate::{agent::registry, hub::Hub, market, remote::relay, screen, update, usage, voice};
use std::sync::Arc;

pub fn start(hub: &Arc<Hub>) {
    tokio::spawn({
        let hub = hub.clone();
        async move {
            if let Err(error) = crate::chat::lead::ensure(&hub).await {
                tracing::error!(error = format!("{error:#}"), "couldn't make the lead bot");
            }
        }
    });
    tokio::spawn(registry::refresh_loop());
    tokio::spawn(crate::icons::cleanup_loop(hub.clone()));
    tokio::spawn(market::refresh_first_page());
    tokio::spawn(market::automatic_loop(hub.clone()));
    tokio::spawn(voice::refresh_loop(hub.clone()));
    tokio::spawn(crate::chat::calls::sweep_loop(hub.clone()));
    tokio::spawn({
        let hub = hub.clone();
        async move { market::agent_sign_in::refresh(&hub).await }
    });
    tokio::spawn(usage::poll(hub.clone()));
    tokio::spawn(update::automatic_loop(hub.clone()));
    tokio::spawn(update::release_watch());
    tokio::spawn(screen::serve_helpers(hub.screen.clone()));
    tokio::spawn(relay::run(hub.clone()));
    #[cfg(target_os = "linux")]
    tokio::spawn(screen::supervise_linux_helper(hub.screen.clone()));
}
