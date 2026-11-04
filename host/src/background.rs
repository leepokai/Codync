//! Background services started alongside the host API.

use crate::{agent::registry, hub::Hub, market, remote::relay, screen, update, usage, voice};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

/// Starts the host's background work. Answers the tasks that run the computer's agent CLIs, for [`stop_agent_checks`].
pub fn start(hub: &Arc<Hub>) -> Vec<JoinHandle<()>> {
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
    let agent_checks = vec![
        tokio::spawn({
            let hub = hub.clone();
            async move { market::agent_sign_in::refresh(&hub).await }
        }),
        tokio::spawn(usage::poll(hub.clone())),
    ];
    tokio::spawn(update::automatic_loop(hub.clone()));
    tokio::spawn(update::release_watch());
    tokio::spawn(screen::serve_helpers(hub.screen.clone()));
    tokio::spawn(relay::run(hub.clone()));
    #[cfg(target_os = "linux")]
    tokio::spawn(screen::supervise_linux_helper(hub.screen.clone()));
    agent_checks
}

/// Ends the running agent CLI checks: the host leaves with `process::exit`, which never drops a task's child process,
/// and a `claude -p /usage` left behind holds on to its memory for as long as it runs.
// ponytail: a check an API call started (`usage`, `refreshBackends`) can still outlive the host; track those too if one
// shows up in practice.
pub async fn stop_agent_checks(tasks: Vec<JoinHandle<()>>) {
    for task in &tasks {
        task.abort();
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), futures::future::join_all(tasks)).await;
}
