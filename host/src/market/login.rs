//! A local connector's sign-in command, run with its environment before each start.
//!
//! Proton Pass is the reason: its CLI keeps a two-hour session, and `pass-cli login`
//! reads the personal access token from the connector's own environment. A failed or
//! slow sign-in never blocks the start; the server reports its own auth error.

use std::collections::BTreeMap;
use std::time::Duration;

/// How long a sign-in may take before the server starts without it.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Runs `login` with `env`, best effort.
pub async fn run(login: &[String], env: &BTreeMap<String, String>) {
    let Some((program, args)) = login.split_first() else { return };
    let mut command = tokio::process::Command::new(crate::shell::program(program));
    command
        .args(args)
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    match tokio::time::timeout(TIMEOUT, command.status()).await {
        Ok(Ok(status)) if !status.success() => tracing::warn!(%status, "a connector sign-in failed"),
        Ok(Err(error)) => tracing::warn!(%error, "a connector sign-in didn't start"),
        Err(_) => tracing::warn!("a connector sign-in timed out"),
        Ok(Ok(_)) => {}
    }
}
