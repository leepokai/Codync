//! The push tier for Claude Code: the session's own `codync-host mcp network` waits on the host for
//! its letters and, once the session is idle, tells it on its inbox socket, which starts a turn.
//! Only the count, never a letter's text: the session reads them with `check_inbox`.

use super::{BOT_ENV, WAIT_REPLY};
use crate::local_api::local_call_for;
use codync_network::AgentKind::{self, ClaudeCode};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::time::Duration;
use tokio::time::{Instant, sleep};

/// After a failed ask or write (the host restarting, the session not answering).
const RETRY: Duration = Duration::from_secs(30);
/// After an ask that came back empty at once (a session not registered until its first codync call).
const EMPTY_PAUSE: Duration = Duration::from_secs(5);
/// How often, and how many times, a busy session is looked at again: up to ten minutes.
const IDLE_POLL: Duration = Duration::from_secs(2);
const IDLE_POLLS: u32 = 300;
/// A written notice's confirmation, tried this many times this far apart.
const CONFIRM_TRIES: u32 = 5;
const CONFIRM_PAUSE: Duration = Duration::from_secs(3);

/// Whether this network server tells its session of letters itself: a Claude Code session's, which
/// gave it its inbox socket, and never a bot's agent. `env` reads the environment.
pub(super) fn wanted(client: &str, env: impl Fn(&str) -> Option<OsString>) -> bool {
    cfg!(unix)
        && AgentKind::of_mcp_client(client) == ClaudeCode
        && env("CLAUDE_CODE_MESSAGING_SOCKET").is_some()
        && env(BOT_ENV).is_none()
}

/// Tells the session of its letters as they come, for as long as the server runs. Confirmed (`told`)
/// only once written, so a failed write is told again.
pub(super) async fn run(port: u16) {
    loop {
        let started = Instant::now();
        let Some(id) = session_id().await else {
            sleep(RETRY).await;
            continue;
        };
        let Some(answer) = ask(port, json!({"kind": ClaudeCode, "session": id, "push": true, "waitSecs": crate::chat::team::mailbox::WAIT_MAX_SECS})).await else {
            sleep(RETRY).await;
            continue;
        };
        if answer["letter"].is_null() {
            if started.elapsed() < Duration::from_secs(1) {
                sleep(EMPTY_PAUSE).await;
            }
            continue;
        }
        // Still busy after ten minutes: a message now wouldn't reach it, so it's asked again later.
        if !until_idle(&id).await {
            continue;
        }
        // A `/clear` meanwhile put this process in another session, whose inbox this isn't.
        if session_id().await.as_deref() != Some(id.as_str()) {
            continue;
        }
        // Asked again: the session may have read its mail during the turn it was busy with.
        let Some(answer) = ask(port, json!({"kind": ClaudeCode, "session": id, "push": true})).await else { continue };
        let (Some(notice), Some(letter)) = (answer["notice"].as_str(), answer["letter"].as_str()) else { continue };
        let text = format!("[Codync Network: an automated notice, not a message from the user] {notice}");
        if let Err(e) = codync_network::post_to_own_claude_session(&text).await {
            tracing::warn!(error = format!("{e:#}"), "couldn't tell the Claude Code session of its mail");
            ask(port, json!({"kind": ClaudeCode, "session": id, "push": true, "failed": "write"})).await;
            sleep(RETRY).await;
            continue;
        }
        confirm(port, &id, letter).await;
    }
}

/// Confirms the notice of `letter` it wrote, retried a few times: a confirmation lost would tell the
/// session again without counting the wake. The host counts each written notice once.
async fn confirm(port: u16, id: &str, letter: &str) {
    for _ in 0..CONFIRM_TRIES {
        if ask(port, json!({"kind": ClaudeCode, "session": id, "push": true, "told": letter})).await.is_some() {
            return;
        }
        sleep(CONFIRM_PAUSE).await;
    }
    tracing::warn!("couldn't confirm a notice to the host: the session may be told again");
}

async fn ask(port: u16, body: Value) -> Option<Value> {
    local_call_for(port, "networkHook", body, WAIT_REPLY)
        .await
        .inspect_err(|e| tracing::debug!(error = format!("{e:#}"), "the host didn't answer the push wait"))
        .ok()
}

/// The Claude Code session this process runs in, looked up each time: after `/clear` the same
/// process is in another session.
async fn session_id() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        codync_network::sessions::session_id(ClaudeCode, |k| std::env::var(k).ok(), &Value::Null)
    })
    .await
    .ok()
    .flatten()
}

/// Waits while the Claude Code session `id` works on a turn (a message then would wait unread
/// behind it), up to ten minutes; a session that doesn't say goes on at once. False when it is
/// still busy then.
async fn until_idle(id: &str) -> bool {
    for _ in 0..IDLE_POLLS {
        let id = id.to_owned();
        let busy = tokio::task::spawn_blocking(move || codync_network::sessions::info(ClaudeCode, &id).busy);
        if busy.await.ok().flatten() != Some(true) {
            return true;
        }
        sleep(IDLE_POLL).await;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOCKET: &str = "CLAUDE_CODE_MESSAGING_SOCKET";

    #[test]
    fn only_a_claude_code_session_not_a_bots_is_told_by_its_server() {
        let env = |set: &'static [&'static str]| move |k: &str| set.contains(&k).then(|| OsString::from("x"));
        assert_eq!(wanted("claude-code", env(&[SOCKET])), cfg!(unix));
        assert!(!wanted("codex-mcp-client", env(&[SOCKET])), "another agent");
        assert!(!wanted("claude-code", env(&[])), "no inbox socket");
        assert!(!wanted("claude-code", env(&[SOCKET, BOT_ENV])), "a bot's agent");
    }
}
