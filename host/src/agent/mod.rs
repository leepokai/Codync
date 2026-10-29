//! Driving coding agents: harness discovery, the ACP registry, sign-in, setup
//! terminals and the per-bot actor that runs turns over ACP.

pub mod acp;
pub mod auth;
pub mod backends;
pub mod bot;
pub mod config;
pub mod fallback;
pub mod gear;
pub mod new_bot;
pub mod node;
pub mod own_memory;
pub mod registry;
pub mod term;
pub mod workspace;

mod process;
#[cfg(windows)]
mod process_windows;
