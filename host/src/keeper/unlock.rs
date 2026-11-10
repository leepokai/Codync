//! The vault key, from the Codync cloud at each start: `GET /v1/keeper/vault-key`, signed by the keeper's identity
//! (the key its claim named). It stays in memory; until it arrives, every op that needs a secret answers `locked`.

use super::daemon::Keeper;
use crate::remote::crypto;
use anyhow::{Result, anyhow, ensure};
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

const PATH: &str = "/v1/keeper/vault-key";
#[cfg(not(test))]
const FIRST_WAIT: Duration = Duration::from_secs(2);
#[cfg(test)]
const FIRST_WAIT: Duration = Duration::from_millis(20);
/// The longest wait while a new computer's claim may land any moment (the host waits 15 s for its keeper's key).
const SOON: Duration = Duration::from_secs(5);
/// How long the claim may take before the keeper asks less often.
const CLAIM_TIME: Duration = Duration::from_mins(10);
const MAX_WAIT: Duration = Duration::from_mins(1);

/// Asks until the cloud answers (404 before the claim named this keeper, 5xx, offline), waiting 2 s, doubling to 5 s
/// for the first ten minutes, then to a minute.
pub async fn run(keeper: Arc<Keeper>, cloud: String) {
    let started = std::time::Instant::now();
    let mut wait = FIRST_WAIT;
    loop {
        match fetch(&keeper, &cloud).await {
            Ok(key) => {
                keeper.set_key(key);
                tracing::info!("the keeper has its vault key");
                return;
            }
            Err(e) => tracing::info!(error = format!("{e:#}"), retry_secs = wait.as_secs(), "no vault key yet"),
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(if started.elapsed() < CLAIM_TIME { SOON } else { MAX_WAIT });
    }
}

async fn fetch(keeper: &Keeper, cloud: &str) -> Result<Zeroizing<[u8; 32]>> {
    let sig = keeper.identity.sign_request("GET", &crate::remote::cloud::authority(cloud)?, PATH, b"");
    let answer = crate::http()
        .get(format!("{cloud}{PATH}"))
        .header("Codync-Sig", sig)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| anyhow!("the Codync cloud is unreachable: {}", e.without_url()))?;
    let status = answer.status();
    ensure!(status.is_success(), "the Codync cloud answered {status}");
    let body = Zeroizing::new(answer.bytes().await?.to_vec());
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|_| anyhow!("the Codync cloud sent no key"))?;
    let key = v["key"].as_str().ok_or_else(|| anyhow!("the Codync cloud sent no key"))?;
    Ok(Zeroizing::new(crypto::unb64_n::<32>(key).map_err(|_| anyhow!("the Codync cloud sent an invalid key"))?))
}
