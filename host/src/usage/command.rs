//! Run an installed CLI for a usage probe. A missing program is silence: the
//! probe is skipped until the agent is installed.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// `name` on the login-shell PATH.
pub(super) fn program(name: &str) -> Option<PathBuf> {
    crate::agent::backends::which(name)
}

/// Stdout of `program`, with terminal colour removed. Nothing when it isn't
/// installed, the process produces no output, or it doesn't finish in time.
/// A non-zero exit still returns stdout: the parser decides whether it is usage.
pub(super) async fn output(
    program: &Path,
    args: &[&str],
    stdin_text: Option<&str>,
    timeout: Duration,
) -> Option<String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .kill_on_drop(true)
        .stdin(if stdin_text.is_some() { std::process::Stdio::piped() } else { std::process::Stdio::null() })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(home) = dirs::home_dir() {
        cmd.current_dir(home);
    }
    let mut child = cmd.spawn().ok()?;
    if let Some(text) = stdin_text
        && let Some(mut input) = child.stdin.take()
    {
        use tokio::io::AsyncWriteExt;
        // A CLI that didn't get its command prints no usage: the empty answer below reads as no reading.
        let _ = input.write_all(text.as_bytes()).await;
    }
    let done = tokio::time::timeout(timeout, child.wait_with_output()).await.ok()?.ok()?;
    let text = strip_ansi(&String::from_utf8_lossy(&done.stdout));
    (!text.trim().is_empty()).then_some(text)
}

/// Drop CSI colour sequences (`ESC [ … m`). Other characters stay, including
/// the cursor-position query a Windows console sends.
pub(super) fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_codes_drop_out_of_a_usage_line() {
        let raw = "Credits \u{1b}[32m(12 of 100\u{1b}[0m covered)";
        assert_eq!(strip_ansi(raw), "Credits (12 of 100 covered)");
    }
}
