//! The login a chain step runs on, as a key: an agent's id on the computer's own login, `<agent>#account-<id>` on
//! one of its agent accounts (`agent::accounts`), `<agent>#<hash>` for a custom command (two custom commands are
//! two agents). It tells the chain's steps apart, and it is whose usage a rest is about.

use crate::store::BotConfig;

/// Between the agent and the id of its agent account. Never the start of a custom command's hash.
const ON_ACCOUNT: &str = "#account-";

/// The login `cfg` runs on. `cfg` is the bot as it runs: its `account` is its agent's.
pub fn of(cfg: &BotConfig) -> String {
    key(&cfg.backend, cfg.command.as_deref(), cfg.account.as_deref())
}

/// The login of agent `backend` run by `command`, if a custom one, on agent account `account`.
pub fn key(backend: &str, command: Option<&str>, account: Option<&str>) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    if let Some(id) = account {
        return format!("{backend}{ON_ACCOUNT}{id}");
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

/// The agent account `login` is on; none for the computer's own login.
pub(super) fn agent_account(login: &str) -> Option<&str> {
    login.split_once(ON_ACCOUNT).map(|(_, id)| id)
}

/// The agent `login` runs.
pub(super) fn backend_of(login: &str) -> &str {
    login.split_once('#').map_or(login, |(backend, _)| backend)
}
