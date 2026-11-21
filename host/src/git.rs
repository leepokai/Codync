//! Running git for the host: one way to find it and to start it, with nothing inherited that could point it at
//! another repository, and never through a shell.

use crate::LockExt as _;
use anyhow::{Context as _, Result, anyhow, bail};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// git, where running it can't open macOS's "install the command line tools" prompt.
pub async fn program() -> Option<PathBuf> {
    let git = codync_agent::backends::which("git")?;
    if cfg!(target_os = "macos") && git == Path::new("/usr/bin/git") {
        let tools = tokio::process::Command::new("/usr/bin/xcode-select").arg("-p").output().await.ok()?;
        if !tools.status.success() {
            return None;
        }
    }
    Some(git)
}

/// How long an answer of [`installed`] stands before git is looked for again.
const INSTALLED_FOR: Duration = Duration::from_mins(5);

/// Whether the host can run git (`bot.gitInstalled`), without waiting for the look: the last answer, looked up
/// again in the background once it is older than [`INSTALLED_FOR`]. Before the first look ends it says yes, as a
/// host without the field does.
pub fn installed() -> bool {
    /// The last answer, and when the last look started.
    static KNOWN: Mutex<(bool, Option<Instant>)> = Mutex::new((true, None));
    let mut known = KNOWN.locked();
    if known.1.is_none_or(|at| at.elapsed() > INSTALLED_FOR)
        && let Ok(runtime) = tokio::runtime::Handle::try_current()
    {
        known.1 = Some(Instant::now());
        runtime.spawn(async {
            let found = program().await.is_some();
            KNOWN.locked().0 = found;
        });
    }
    known.0
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
    tokio::time::timeout(limit, output(dir, args, true))
        .await
        .map_err(|_| anyhow!("git {} took too long", args.first().copied().unwrap_or_default()))?
}

/// As [`run`], with no time limit, and never stopped, not even when the host stops: for git whose stop would leave
/// its lock behind (`agent::versions`). What git printed is given as it is, untrimmed: a `-z` list's names may start
/// or end with a space.
pub async fn run_to_end(dir: &Path, args: &[&str]) -> Result<String> {
    raw(dir, args, false).await
}

async fn output(dir: &Path, args: &[&str], stoppable: bool) -> Result<String> {
    Ok(raw(dir, args, stoppable).await?.trim().to_owned())
}

async fn raw(dir: &Path, args: &[&str], stoppable: bool) -> Result<String> {
    let git = program().await.ok_or_else(|| anyhow!("git isn't installed"))?;
    let out = command(&git, dir).kill_on_drop(stoppable).args(args).output().await.context("starting git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
