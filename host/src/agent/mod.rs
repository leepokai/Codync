//! Driving coding agents: harness discovery, the ACP registry, sign-in, setup
//! terminals and the per-bot actor that runs turns over ACP.

pub mod account;
pub mod acp;
pub mod always_rule;
pub mod approval_rules;
pub mod auth;
pub mod backends;
pub mod blocklist;
pub mod bot;
pub mod config;
#[allow(dead_code, unused_imports, reason = "OpenMausBot scaffolding, not wired into the host yet")]
pub mod coordinator;
pub mod envelope;
pub mod fallback;
pub mod loop_guard;
pub mod new_bot;
pub mod node;
pub mod own_memory;
pub mod personality;
pub mod registry;
#[allow(dead_code, unused_imports, reason = "OpenMausBot scaffolding, not wired into the host yet")]
pub mod sandbox;
pub mod security_profile;
pub mod term;
pub mod workspace;

pub(crate) mod process;
#[cfg(windows)]
mod process_windows;
