//! Driving coding agents: harness discovery, the ACP registry, sign-in, setup
//! terminals and the per-bot actor that runs turns over ACP.

pub mod acp;
pub mod auth;
pub mod backends;
pub mod bot;
mod process;
#[cfg(windows)]
mod process_windows;
pub mod registry;
pub mod term;
pub mod workspace;
