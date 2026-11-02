//! The login shell's variables, for the agents we start. A host started by launchd or systemd
//! has only PATH and `CODYNC_HOME`, so a key the user exports in `~/.zshrc` (a custom model's
//! API key) would reach the agent in their terminal but never the same agent in Codync.
//! They go to agents only, never into this process: a proxy or TLS variable meant for the
//! user's tools must not change how the host itself connects (relay, push, updates).
//! Values are secrets: they are never logged or sent to a client.

use crate::LockExt;
use std::sync::Mutex;

static VARS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Keeps `login`'s variables that this process lacks, replacing what an earlier login gave.
pub(super) fn set(login: &[(String, String)]) {
    let vars = wanted(login, |key| std::env::var_os(key).is_some());
    tracing::info!(count = vars.len(), "read the login shell's environment for agents");
    *VARS.locked() = vars;
}

/// The variables an agent starts with on top of this process's own.
pub fn vars() -> Vec<(String, String)> {
    VARS.locked().clone()
}

/// This process's own variables win (HOME, PATH, which is merged on its own); the shell's
/// bookkeeping isn't the user's, and `CODYNC_*` configure Codync, never a login shell.
fn wanted(login: &[(String, String)], own: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    login
        .iter()
        .filter(|(k, _)| {
            !own(k) && !k.starts_with("CODYNC_") && !matches!(k.as_str(), "PATH" | "PWD" | "OLDPWD" | "SHLVL" | "_")
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn keeps_only_what_the_host_lacks() {
        let login = pairs(&[("HOME", "/x"), ("MY_KEY", "k"), ("PATH", "/bin"), ("CODYNC_HOME", "/y"), ("SHLVL", "2")]);
        assert_eq!(wanted(&login, |k| k == "HOME"), pairs(&[("MY_KEY", "k")]));
    }
}
