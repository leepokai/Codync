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
    SETUP.get().expect("the agent runtime is set up at start, before any agent runs")
}
