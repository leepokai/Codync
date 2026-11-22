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

/// Gives the agent runtime the host's data directory, binary and HTTP client: `main` at start, and tests, which
/// never run it.
pub fn init_runtime() {
    codync_agent::init(codync_agent::Setup {
        data_dir: crate::service::data_dir(),
        exe: crate::service::current_exe().ok(),
        http: crate::http,
    });
}
