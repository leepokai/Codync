//! The login a chain step runs on, as a key: an agent's id on the computer's own login, `<agent>#profile-<id>` on
//! one of its login profiles (`agent::login_profiles`), `<agent>#<hash>` for a custom command (two custom commands are
//! two agents). It tells the chain's steps apart, and it is whose usage a rest is about.

use crate::store::BotConfig;

/// Between the agent and the id of its login profile. Never the start of a custom command's hash.
const ON_PROFILE: &str = "#profile-";

/// The login `cfg` runs on. `cfg` is the bot as it runs: its login profile is its agent's.
pub fn of(cfg: &BotConfig) -> String {
    key(&cfg.backend, cfg.command.as_deref(), cfg.login_profile.as_deref())
}

/// The login of agent `backend` run by `command`, if a custom one, on login profile `login_profile`.
pub fn key(backend: &str, command: Option<&str>, login_profile: Option<&str>) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    if let Some(id) = login_profile {
        return format!("{backend}{ON_PROFILE}{id}");
    }
    let Some(command) = command.map(str::trim).filter(|c| !c.is_empty()) else {
        return backend.to_owned();
    };
    let hash = Sha256::digest(command);
    hash[..6].iter().fold(format!("{backend}#"), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The login profile `login` is on; none for the computer's own login.
pub(super) fn login_profile(login: &str) -> Option<&str> {
    login.split_once(ON_PROFILE).map(|(_, id)| id)
}

/// The agent `login` runs.
pub(super) fn backend_of(login: &str) -> &str {
    login.split_once('#').map_or(login, |(backend, _)| backend)
}
