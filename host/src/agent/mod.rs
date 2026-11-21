//! Driving coding agents: harness discovery, the ACP registry, sign-in, setup
//! terminals and the per-bot actor that runs turns over ACP.

pub mod acp;
pub mod auth;
pub mod backends;
pub mod bot;
pub mod codex_app;
pub mod config;
pub mod elicitation;
pub mod fallback;
pub mod gear;
pub mod login_profiles;
pub mod modes;
pub mod new_bot;
pub mod node;
pub mod own_memory;
pub mod registry;
pub mod setup;
pub mod term;
pub mod versions;
pub mod workspace;

/// Tests never run `main`: the agent runtime gets the tests' data directory here.
#[cfg(test)]
pub fn set_up_for_tests() {
    setup::init(setup::Setup {
        data_dir: crate::service::data_dir(),
        exe: crate::service::current_exe().ok(),
        http: crate::http().clone(),
    });
}

pub(crate) mod process;
#[cfg(unix)]
mod process_unix;
#[cfg(windows)]
mod process_windows;
