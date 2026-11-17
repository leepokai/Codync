//! Why an agent process went away: the stderr it left, read before its exit is reported.

use super::{Failure, FailureKind, Incoming};
use std::time::Duration;
use tokio::sync::mpsc;

/// How long an exited agent's last stderr lines get to be read before its exit is reported.
pub(super) const STDERR_FLUSH: Duration = Duration::from_secs(1);

/// What an agent that failed with `e` printed to stderr, when `e` is its process going away: the reader reports
/// the tail right after it fails the waiters. `None` when it's still running or says nothing in time.
pub async fn exit_tail(e: &anyhow::Error, inbox: &mut mpsc::UnboundedReceiver<Incoming>) -> Option<String> {
    let exited = e.chain().filter_map(|c| c.downcast_ref::<Failure>()).any(|f| f.kind == FailureKind::Exited);
    if !exited {
        return None;
    }
    let closed = async {
        while let Some(incoming) = inbox.recv().await {
            if let Incoming::Closed { stderr_tail } = incoming {
                return Some(stderr_tail);
            }
        }
        None
    };
    tokio::time::timeout(STDERR_FLUSH * 2, closed).await.ok().flatten()
}

/// The stderr line that says why the agent quit: an error line, else the last one that isn't a pretty-printed Node
/// error's closing brace.
pub fn stderr_reason(stderr_tail: &str) -> Option<String> {
    let lines: Vec<&str> = stderr_tail.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    lines
        .iter()
        .rev()
        .find(|line| line.contains("Error"))
        .or_else(|| lines.iter().rev().find(|line| !matches!(**line, "}" | "{" | "}," | "]" | "[")))
        .map(|line| (*line).to_owned())
}

#[cfg(test)]
mod tests {
    use super::super::{Acp, INTERNAL_ERROR, RpcError};
    use super::*;
    use serde_json::json;

    fn quitting_agent() -> String {
        let python = if cfg!(windows) { "python" } else { "python3" };
        format!(r#"{python} -c "import sys; sys.stderr.write('profile acp does not exist\n'); sys.exit(1)""#)
    }

    #[tokio::test]
    async fn an_agent_that_quits_at_once_says_why() {
        let (acp, mut rx) = Acp::spawn(&quitting_agent(), env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
        let e = acp.request("initialize", json!({})).await.unwrap_err();
        let tail = exit_tail(&e, &mut rx).await;
        assert_eq!(tail.as_deref(), Some("profile acp does not exist"));
    }

    #[tokio::test]
    async fn an_agent_gone_before_the_request_is_written_says_why() {
        let (acp, mut rx) = Acp::spawn(&quitting_agent(), env!("CARGO_MANIFEST_DIR"), &[]).unwrap();
        // Its stdin is closed by the time `initialize` is written: the write itself fails.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let e = acp.request("initialize", json!({})).await.unwrap_err();
        let tail = exit_tail(&e, &mut rx).await;
        assert_eq!(tail.as_deref(), Some("profile acp does not exist"));
    }

    #[tokio::test]
    async fn an_agent_still_running_has_no_exit_tail() {
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let e = anyhow::anyhow!(RpcError { code: INTERNAL_ERROR, message: "no".into(), kind: None });
        assert_eq!(exit_tail(&e, &mut rx).await, None);
    }

    #[test]
    fn stderr_reason_skips_a_closing_brace() {
        let tail = "Error: Cannot find module '../dist/codebuddy'\nRequire stack:\n- /cli/bin/codebuddy\n  code: 'MODULE_NOT_FOUND'\n}\n";
        assert_eq!(stderr_reason(tail).as_deref(), Some("Error: Cannot find module '../dist/codebuddy'"));
        assert_eq!(stderr_reason("agent said no\n").as_deref(), Some("agent said no"));
    }
}
