//! Where a finished workflow's last reply goes: a channel, a chat, or the issue or pull request that
//! started the run.
//!
//! Outputs are delivered once, after every step ended, from the deepest replies that have no step
//! after them. A failed delivery is logged and does not fail the run; the run's chat stays the record.

use super::Output as RoutineOutput;
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// The most outputs one routine may have.
pub const MAX: usize = 10;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Output {
    /// `slack`, `telegram`, `discord` or `githubComment`.
    pub kind: String,
    /// A channel id, a chat id, or (for GitHub) the repository, `owner/name`.
    pub target: String,
    /// The Telegram bot to send as; other kinds ignore it.
    #[serde(default)]
    pub account: Option<String>,
}

/// Why one output can't be saved, or `None` when it can.
pub fn problem(output: &Output) -> Option<String> {
    let target = output.target.trim();
    if target.is_empty() || target.len() > 200 {
        return Some("An output needs a destination.".into());
    }
    match output.kind.as_str() {
        "slack" | "discord" if target.len() > 64 || target.contains(char::is_whitespace) => {
            Some("Use a channel id.".into())
        }
        "telegram" if target.parse::<i64>().is_err() => Some("Use a Telegram chat id.".into()),
        "githubComment" if !github_repo(target) => Some("Use owner/name.".into()),
        "slack" | "telegram" | "discord" | "githubComment" => None,
        _ => Some("Choose what kind of output it is.".into()),
    }
}

fn github_repo(value: &str) -> bool {
    let Some((owner, name)) = value.split_once('/') else { return false };
    !owner.is_empty()
        && !name.is_empty()
        && !name.contains('/')
        && [owner, name]
            .iter()
            .all(|part| part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')))
}

pub fn parse(value: Option<&Value>) -> Result<Vec<Output>> {
    let Some(value) = value else { return Ok(Vec::new()) };
    let outputs: Vec<Output> = serde_json::from_value(value.clone()).context("invalid outputs")?;
    if outputs.len() > MAX {
        bail!("At most {MAX} outputs.");
    }
    if let Some(why) = outputs.iter().find_map(problem) {
        bail!("{why}");
    }
    Ok(outputs)
}

/// What an output says in the routine's own description.
pub fn describe(output: &Output) -> String {
    match output.kind.as_str() {
        "githubComment" => format!("Comment on a {} issue or pull request", output.target),
        "slack" => format!("Post in Slack {}", output.target),
        "telegram" => format!("Send to Telegram {}", output.target),
        "discord" => format!("Post in Discord {}", output.target),
        other => format!("{other} {}", output.target),
    }
}

/// Delivers `text` to every output in turn; errors are logged, never returned.
pub async fn deliver(hub: &Arc<Hub>, routine: &str, outputs: &[RoutineOutput], event: &Value, text: &str) {
    if outputs.is_empty() || text.trim().is_empty() {
        return;
    }
    for output in outputs {
        if let Err(error) = one(hub, output, event, text).await {
            tracing::warn!(routine, kind = output.kind, error = format!("{error:#}"), "workflow output failed");
        }
    }
}

async fn one(hub: &Arc<Hub>, output: &Output, event: &Value, text: &str) -> Result<()> {
    match output.kind.as_str() {
        "slack" => {
            let (token, _) = crate::slack::credentials(&hub.store)?.context("Slack is not configured.")?;
            let thread = event["thread"].as_str().unwrap_or_default();
            let client = crate::egress::client(
                crate::egress::Policy::Only(&["slack.com"]),
                "https://slack.com/api/chat.postMessage",
            )
            .await?;
            let body = json!({"channel": output.target, "text": text, "thread_ts": thread,
                "unfurl_links": false, "unfurl_media": false});
            let response = client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(token)
                .json(&body)
                .timeout(Duration::from_secs(20))
                .send()
                .await?;
            let answer: Value = response.json().await?;
            if answer["ok"] != true {
                bail!("Slack refused the post: {}", answer["error"].as_str().unwrap_or("unknown"));
            }
        }
        "telegram" => {
            let account = match output.account.as_deref() {
                Some(name) if !name.is_empty() => name.to_owned(),
                _ => crate::telegram::first_name(&hub.store)?.context("No Telegram bot is set up.")?,
            };
            let chat = output.target.parse::<i64>()?;
            crate::telegram::send_text(&hub.store, &account, chat, text).await?;
        }
        "discord" => crate::discord::send_text(&hub.store, &output.target, text).await?,
        "githubComment" => github_comment(hub, output, event, text).await?,
        other => bail!("Unknown output {other}."),
    }
    Ok(())
}

async fn github_comment(hub: &Arc<Hub>, output: &Output, event: &Value, text: &str) -> Result<()> {
    let token = crate::github::connection(&hub.store)?.context("GitHub is not configured.")?.token;
    let repo = if output.target.trim().is_empty() {
        event["repo"].as_str().context("The run did not come from a repository.")?.to_owned()
    } else {
        output.target.trim().to_owned()
    };
    let number = number(event).context("The run did not name an issue or pull request.")?;
    let url = format!("https://api.github.com/repos/{repo}/issues/{number}/comments");
    let client = crate::egress::client(crate::egress::Policy::Only(&["api.github.com"]), &url).await?;
    let response = client
        .post(&url)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .header("user-agent", "codync")
        .json(&json!({"body": text}))
        .timeout(Duration::from_secs(20))
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("GitHub answered {}", response.status());
    }
    Ok(())
}

/// The issue or pull request an event names.
fn number(event: &Value) -> Option<i64> {
    [&event["payload"]["number"], &event["payload"]["issue"]["number"], &event["payload"]["pull_request"]["number"]]
        .iter()
        .find_map(|value| value.as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(kind: &str, target: &str) -> Output {
        Output { kind: kind.into(), target: target.into(), account: None }
    }

    #[test]
    fn each_kind_checks_its_destination() {
        assert!(problem(&output("slack", "C0123")).is_none());
        assert!(problem(&output("discord", "123")).is_none());
        assert!(problem(&output("telegram", "-100123")).is_none());
        assert!(problem(&output("githubComment", "owner/repo")).is_none());
        assert!(problem(&output("slack", "not a channel")).is_some());
        assert!(problem(&output("telegram", "abc")).is_some());
        assert!(problem(&output("githubComment", "owner")).is_some());
        assert!(problem(&output("githubComment", "owner/repo/extra")).is_some());
        assert!(problem(&output("nope", "x")).is_some());
        assert!(problem(&output("slack", " ")).is_some());
    }

    #[test]
    fn parsing_accepts_an_absent_list_and_refuses_too_many() {
        assert_eq!(parse(None).unwrap().len(), 0);
        let many = vec![json!({"kind": "slack", "target": "C1"}); MAX + 1];
        assert!(parse(Some(&json!(many))).is_err());
    }

    #[test]
    fn a_number_is_read_from_the_issue_or_pull_request() {
        assert_eq!(number(&json!({"payload": {"number": 7}})), Some(7));
        assert_eq!(number(&json!({"payload": {"issue": {"number": 8}}})), Some(8));
        assert_eq!(number(&json!({"payload": {"pull_request": {"number": 9}}})), Some(9));
        assert_eq!(number(&json!({"payload": {}})), None);
    }

    #[test]
    fn github_repositories_are_owner_and_name() {
        assert!(github_repo("o/r") && github_repo("my-org/repo.name"));
        assert!(!github_repo("o") && !github_repo("o/r/x") && !github_repo("/r"));
    }
}
