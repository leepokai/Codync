//! Running git for the host: one way to find it and to start it, with nothing inherited that could point it at
//! another repository, and never through a shell.

use anyhow::{Context as _, Result, anyhow, bail};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// git, where running it can't open macOS's "install the command line tools" prompt.
pub async fn program() -> Option<PathBuf> {
    let git = crate::agent::backends::which("git")?;
    if cfg!(target_os = "macos") && git == Path::new("/usr/bin/git") {
        let tools = tokio::process::Command::new("/usr/bin/xcode-select").arg("-p").output().await.ok()?;
        if !tools.status.success() {
            return None;
        }
    }
    Some(git)
}

/// `git` in `dir`, its arguments still to give.
pub fn command(git: &Path, dir: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(git);
    // The folder only: a `GIT_DIR`, `GIT_INDEX_FILE`… this process inherited (from a git hook, say) would point
    // git at another repository.
    for (key, _) in std::env::vars_os().filter(|(k, _)| k.to_string_lossy().starts_with("GIT_")) {
        cmd.env_remove(key);
    }
    // Never a prompt: nobody is there to answer one.
    cmd.current_dir(dir).env("GIT_TERMINAL_PROMPT", "0").stdin(std::process::Stdio::null()).kill_on_drop(true);
    cmd
}

/// Runs `git <args>` in `dir` and gives what it printed, trimmed. An error when git isn't there, takes longer
/// than `limit` (it is stopped) or fails, with what it said.
pub async fn run(dir: &Path, args: &[&str], limit: Duration) -> Result<String> {
    let git = program().await.ok_or_else(|| anyhow!("git isn't installed"))?;
    let out = tokio::time::timeout(limit, command(&git, dir).args(args).output())
        .await
        .map_err(|_| anyhow!("git {} took too long", args.first().copied().unwrap_or_default()))?
        .context("starting git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
