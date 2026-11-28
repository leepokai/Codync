//! Lightweight, explicitly opted-in HTTP decision sidecar with native ordered failover
//! and in-memory circuit breaking. Disabled by default (returns `None` with zero network
//! traffic when unconfigured) and always fail-open.

mod breaker;
mod client;
pub mod config;

pub use config::SidecarTarget;

use crate::store::Store;

/// Non-sensitive telemetry summary of a decision sidecar call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionCallReport {
    pub content: Option<String>,
    pub targets: usize,
    pub attempted: usize,
    pub outcome: &'static str,
    pub ms: u64,
}

/// Runs a fast read-only decision call against explicitly configured OpenAI-compatible
/// sidecar targets from environment variables. Returns `None` without network traffic
/// when unconfigured or when all targets fail/cool down.
pub async fn decision_call(system: &str, prompt: &str) -> Option<String> {
    let targets = config::load_from_env();
    client::decision_call(&targets, system, prompt).await.content
}

/// Runs a fast read-only decision call using sidecar targets configured in environment
/// variables or `store` (`settings.decision_sidecar.targets`). Returns `None` without
/// network traffic when unconfigured or when all targets fail/cool down.
pub async fn decision_call_for_store(store: &Store, system: &str, prompt: &str) -> Option<String> {
    decision_call_for_store_report(store, system, prompt).await.content
}

/// Same as [`decision_call_for_store`], returning non-sensitive outcome/latency metadata for telemetry.
pub async fn decision_call_for_store_report(store: &Store, system: &str, prompt: &str) -> DecisionCallReport {
    let targets = config::load_targets(Some(store));
    client::decision_call(&targets, system, prompt).await
}
