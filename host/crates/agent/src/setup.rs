//! What the agent runtime takes from the app that embeds it: where it keeps the registry cache, downloaded agents
//! and Codync's Node, the binary whose `npm` subcommand installs npm agents, and the process's HTTP client. Given
//! once at start, before any agent runs.

use std::path::PathBuf;
use std::sync::OnceLock;

pub struct Setup {
    /// Holds `registry.json`, `agents/` and `node/`.
    pub data_dir: PathBuf,
    /// The app's own binary; `None` when it can't be located (then npm agents can't be installed).
    pub exe: Option<PathBuf>,
    pub http: reqwest::Client,
}

static SETUP: OnceLock<Setup> = OnceLock::new();

/// The first call wins: a process has one data directory.
pub fn init(setup: Setup) {
    let _ = SETUP.set(setup);
}

pub(crate) fn get() -> &'static Setup {
    SETUP.get().expect("codync_agent::init runs at start, before any agent runs")
}

/// This crate's own tests: a data directory of their own.
#[cfg(test)]
pub(crate) fn set_up_for_tests() {
    init(Setup {
        data_dir: std::env::temp_dir().join(format!("codync-agent-unit-{}", std::process::id())),
        exe: std::env::current_exe().ok(),
        http: reqwest::Client::new(),
    });
}
