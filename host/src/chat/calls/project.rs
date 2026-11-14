//! The project a call's bot works in, from git: what the voice knows of it before it speaks.

use std::path::{Path, PathBuf};
use std::time::Duration;

const GIT_TIMEOUT: Duration = Duration::from_secs(2);

/// The project in `cwd` now: branch, changed files and the last commits; nothing outside a repository.
pub async fn now(cwd: &str) -> String {
    let dir = Path::new(cwd);
    if cwd.is_empty() || !dir.ancestors().any(|d| d.join(".git").exists()) {
        return String::new();
    }
    let Some(git) = git_program().await else { return String::new() };
    let run = |args: &'static [&'static str]| {
        let (git, dir) = (git.clone(), dir.to_owned());
        async move {
            let mut cmd = tokio::process::Command::new(git);
            // The bot's folder only: a `GIT_DIR`, `GIT_INDEX_FILE`… this process inherited (from a git hook, say)
            // would point git at another repository.
            for (key, _) in std::env::vars_os().filter(|(k, _)| k.to_string_lossy().starts_with("GIT_")) {
                cmd.env_remove(key);
            }
            // No optional locks: a status killed at the timeout can't leave `.git/index.lock` behind.
            cmd.args(args)
                .current_dir(dir)
                .env("GIT_OPTIONAL_LOCKS", "0")
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true);
            let out = tokio::time::timeout(GIT_TIMEOUT, cmd.output()).await.ok()?.ok()?;
            // Only the end: a porcelain line starts with its status column, a space when the change is unstaged.
            out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
        }
    };
    let (branch, changes, commits) = tokio::join!(
        run(&["rev-parse", "--abbrev-ref", "HEAD"]),
        run(&["status", "--porcelain"]),
        run(&["log", "-3", "--format=%s"]),
    );
    let mut lines = Vec::new();
    if let Some(branch) = branch.filter(|b| !b.is_empty()) {
        lines.push(format!("Branch: {branch}"));
    }
    if let Some(changes) = changes {
        let files: Vec<&str> = changes.lines().map(|l| l.get(3..).unwrap_or(l)).collect();
        if files.is_empty() {
            lines.push("No uncommitted changes".into());
        } else {
            let shown = files.iter().take(10).copied().collect::<Vec<_>>().join(", ");
            let more = if files.len() > 10 { format!(" and {} more", files.len() - 10) } else { String::new() };
            lines.push(format!("Changed files ({}): {shown}{more}", files.len()));
        }
    }
    if let Some(commits) = commits.filter(|c| !c.is_empty()) {
        lines.push(format!("Last commits: {}", commits.lines().collect::<Vec<_>>().join(" · ")));
    }
    lines.join("\n")
}

/// git, where running it can't open macOS's "install the command line tools" prompt.
async fn git_program() -> Option<PathBuf> {
    let git = crate::agent::backends::which("git")?;
    if cfg!(target_os = "macos") && git == Path::new("/usr/bin/git") {
        let tools = tokio::process::Command::new("/usr/bin/xcode-select").arg("-p").output().await.ok()?;
        if !tools.status.success() {
            return None;
        }
    }
    Some(git)
}
