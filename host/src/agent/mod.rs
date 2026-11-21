//! Driving coding agents: harness discovery, the ACP registry, sign-in, setup
//! terminals and the per-bot actor that runs turns over ACP.

pub mod auth;
pub mod bot;
pub mod codex_app;
pub mod elicitation;
pub mod fallback;
pub mod gear;
pub mod login_profiles;
pub mod new_bot;
pub mod own_memory;
pub mod term;
pub mod versions;
pub mod workspace;

/// Tests never run `main`: the agent runtime gets the tests' data directory here.
#[cfg(test)]
pub fn set_up_for_tests() {
    codync_agent::init(codync_agent::Setup {
        data_dir: crate::service::data_dir(),
        exe: crate::service::current_exe().ok(),
        http: crate::http().clone(),
    });
}
