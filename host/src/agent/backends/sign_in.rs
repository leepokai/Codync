//! Whether each harness is signed in, as its own CLI tells it.

use super::{HARNESSES, Harness, on_path, which};
use crate::LockExt;
use serde_json::Value;
use std::sync::Mutex;
use std::time::Duration;

/// `<bin> <args>`, judged by exit code and stdout.
#[derive(Clone, Copy)]
pub struct SignInCheck {
    pub args: &'static str,
    pub ok: fn(bool, &str) -> bool,
}

pub(super) fn exit_ok(success: bool, _: &str) -> bool {
    success
}

pub(super) fn claude_signed_in(_: bool, out: &str) -> bool {
    serde_json::from_str::<Value>(out).is_ok_and(|v| v["loggedIn"] == true)
}

pub(super) fn qoder_signed_in(_: bool, out: &str) -> bool {
    serde_json::from_str::<Value>(out).is_ok_and(|v| v["logged_in"] == true)
}

pub(super) fn says_logged_in(_: bool, out: &str) -> bool {
    let out = out.to_lowercase();
    out.contains("logged in") && !out.contains("not logged in")
}

/// Last sign-in check per harness id; missing = can't tell.
static SIGNED_IN: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());

pub fn signed_in(id: &str) -> Option<bool> {
    SIGNED_IN.locked().iter().find(|(h, _)| h == id).map(|(_, ok)| *ok)
}

/// Records what a check found (`None`: can't tell).
pub fn set_signed_in(id: &str, state: Option<bool>) {
    let mut map = SIGNED_IN.locked();
    map.retain(|(h, _)| h != id);
    if let Some(ok) = state {
        map.push((id.to_owned(), ok));
    }
}

/// Re-checks every installed harness that can report its sign-in state.
pub async fn refresh_sign_in() {
    let installed = HARNESSES.iter().filter(|h| h.signed_in.is_some() && h.bins.iter().any(|b| on_path(b)));
    let results =
        futures::future::join_all(installed.map(|h| async move { (h.id, cli_signed_in(h, &[]).await) })).await;
    for (id, ok) in results {
        set_signed_in(id, ok);
    }
}

/// What `h`'s own CLI says about its sign-in, run with `env` (the keys its agent gets, so both
/// judge the same login). `None`: it has no such check, isn't installed or didn't answer.
pub async fn cli_signed_in(h: &Harness, env: &[(String, String)]) -> Option<bool> {
    let check = h.signed_in?;
    let bin = h.bins.iter().find_map(|b| which(b))?;
    let run = tokio::process::Command::new(bin)
        .args(check.args.split_whitespace())
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(15), run).await {
        Ok(Ok(out)) => Some((check.ok)(out.status.success(), &String::from_utf8_lossy(&out.stdout))),
        Ok(Err(e)) => {
            tracing::info!(backend = h.id, error = %e, "sign-in check failed");
            None
        }
        Err(_) => None,
    }
}
