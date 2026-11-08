//! Readiness probes: one GET with a short timeout, retried until the tool answers, dies,
//! or the deadline passes.

use anyhow::{Result, bail};
use std::future::Future;
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_EVERY: Duration = Duration::from_millis(500);

/// One probe: any 2xx answers.
pub async fn probe(url: &str) -> bool {
    matches!(crate::http().get(url).timeout(PROBE_TIMEOUT).send().await, Ok(r) if r.status().is_success())
}

/// Probes `url` until it answers. `alive` says whether the tool's process still runs, and
/// `current` whether this start is still the wanted one (a stop or a new start ends the wait quietly).
/// Returns `Ok(true)` when the tool answered, `Ok(false)` when the wait was abandoned.
pub async fn wait_ready<A: Future<Output = Result<bool>> + Send>(
    url: &str,
    timeout: Duration,
    current: impl Fn() -> bool + Send,
    alive: impl Fn() -> A + Send,
) -> Result<bool> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if !current() {
            return Ok(false);
        }
        if !alive().await? {
            bail!("it stopped before it was ready");
        }
        if probe(url).await {
            return Ok(true);
        }
        if tokio::time::Instant::now() > deadline {
            bail!("it didn't answer within {} seconds", timeout.as_secs());
        }
        tokio::time::sleep(RETRY_EVERY).await;
    }
}
