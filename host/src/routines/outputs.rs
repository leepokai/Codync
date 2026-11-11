//! Where a finished workflow's last reply goes: a channel, a chat, or the issue or pull request that
//! started the run.
//!
//! Outputs are durably queued after every step ends. See `delivery` for restart and cancellation
//! semantics; the run's chat remains the record if a destination cannot confirm its send.

use super::delivery::before_send;
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
    /// `slack`, `telegram`, `whatsapp`, `discord` or `githubComment`.
    pub kind: String,
    /// A channel id, a chat id, or (for GitHub) the repository, `owner/name`.
    pub target: String,
    /// The Telegram bot name, or the linked `WhatsApp` account id to send as.
    #[serde(default)]
    pub account: Option<String>,
    /// Labels, reviewers, or the issue title; other kinds ignore it.
    #[serde(default)]
    pub value: Option<String>,
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
        "discordReact" if target.contains(char::is_whitespace) => Some("Use one emoji or name:id.".into()),
        "telegram" if target.parse::<i64>().is_err() => Some("Use a Telegram chat id.".into()),
        "whatsapp" if !crate::whatsapp::delivery::jid(target) => Some("Use a WhatsApp person or group id.".into()),
        "whatsapp"
            if !output.account.as_deref().is_some_and(|account| {
                account.ends_with("@s.whatsapp.net") && crate::whatsapp::delivery::jid(account)
            }) =>
        {
            Some("Use the linked WhatsApp account id as account.".into())
        }
        "githubComment" | "githubIssue" if !github_repo(target) => Some("Use owner/name.".into()),
        "githubLabel" | "githubAssign" | "githubReview" => {
            if !github_repo(target) {
                return Some("Use owner/name.".into());
            }
            let extra = output.value.as_deref().unwrap_or_default().trim();
            if extra.is_empty() {
                return Some(format!("{} needs a value.", output.kind));
            }
            None
        }
        "slack" | "telegram" | "whatsapp" | "discord" | "discordReact" | "githubComment" | "githubIssue" => None,
        _ => Some("Choose what kind of output it is.".into()),
    }
}

/// Whether `value` is a `WhatsApp` chat id: a number or a group, never a bare number that could be either.
fn whatsapp_chat(value: &str) -> bool {
    let Some((id, host)) = value.split_once('@') else { return false };
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit() || b == b'-') && matches!(host, "s.whatsapp.net" | "g.us")
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
        "githubIssue" => format!("Open an issue in {}", output.target),
        "githubLabel" => format!("Label a {} issue", output.target),
        "githubAssign" => format!("Assign a {} issue", output.target),
        "githubReview" => format!("Request a review in {}", output.target),
        "slack" => format!("Post in Slack {}", output.target),
        "telegram" => format!("Send to Telegram {}", output.target),
        "whatsapp" => format!("Send to WhatsApp {}", output.target),
        "discord" => format!("Post in Discord {}", output.target),
        "discordReact" => format!("React with {} on Discord", output.target),
        other => format!("{other} {}", output.target),
    }
}

pub(super) async fn one(hub: &Arc<Hub>, output: &Output, event: &Value, text: &str) -> Result<()> {
    match output.kind.as_str() {
        "slack" => {
            let (token, _) = crate::slack::credentials(&hub.store)?.context("Slack is not configured.")?;
            let thread = event["thread"].as_str().unwrap_or_default();
            let client = before_send(
                crate::egress::client(
                    crate::egress::Policy::Only(&["slack.com"]),
                    "https://slack.com/api/chat.postMessage",
                )
                .await,
            )?;
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
        "whatsapp" => crate::whatsapp::delivery::send(hub, output, event, text).await?,
        "discordReact" => {
            let channel = event["channel"].as_str().context("The run did not come from a channel.")?;
            let message = event["messageId"].as_str().context("The run did not name a message.")?;
            crate::discord::react(&hub.store, channel, message, output.target.trim()).await?;
        }
        "githubComment" => github_comment(hub, output, event, text).await?,
        "githubIssue" => github_issue(hub, output, text).await?,
        "githubLabel" | "githubAssign" | "githubReview" => github_issue_action(hub, output, event).await?,
        other => bail!("Unknown output {other}."),
    }
    Ok(())
}

async fn github_comment(hub: &Arc<Hub>, output: &Output, event: &Value, text: &str) -> Result<()> {
    let repo = repo_of(output, event)?;
    let number = number(event).context("The run did not name an issue or pull request.")?;
    github_call(
        hub,
        reqwest::Method::POST,
        &format!("https://api.github.com/repos/{repo}/issues/{number}/comments"),
        json!({"body": text}),
    )
    .await
}

/// The repository an output names, else the one the run came from.
fn repo_of(output: &Output, event: &Value) -> Result<String> {
    let named = output.target.trim();
    if !named.is_empty() {
        return Ok(named.to_owned());
    }
    Ok(event["repo"].as_str().context("The run did not come from a repository.")?.to_owned())
}

/// Opens an issue whose title is the reply's first line and whose body is the rest.
async fn github_issue(hub: &Arc<Hub>, output: &Output, text: &str) -> Result<()> {
    let repo = output.target.trim();
    let title =
        output.value.as_deref().map(str::trim).filter(|t| !t.is_empty()).map_or_else(
            || text.lines().next().unwrap_or("Workflow result").chars().take(120).collect(),
            str::to_owned,
        );
    github_call(
        hub,
        reqwest::Method::POST,
        &format!("https://api.github.com/repos/{repo}/issues"),
        json!({"title": title, "body": text}),
    )
    .await
}

/// Labels, assigns or asks for review on the issue or pull request that started the run.
async fn github_issue_action(hub: &Arc<Hub>, output: &Output, event: &Value) -> Result<()> {
    let repo = repo_of(output, event)?;
    let number = number(event).context("The run did not name an issue or pull request.")?;
    let names: Vec<&str> =
        output.value.as_deref().unwrap_or_default().split(',').map(str::trim).filter(|v| !v.is_empty()).collect();
    let (method, url, body) = match output.kind.as_str() {
        "githubLabel" => (
            reqwest::Method::POST,
            format!("https://api.github.com/repos/{repo}/issues/{number}/labels"),
            json!({"labels": names}),
        ),
        "githubAssign" => (
            reqwest::Method::PATCH,
            format!("https://api.github.com/repos/{repo}/issues/{number}"),
            json!({"assignees": names}),
        ),
        _ => (
            reqwest::Method::POST,
            format!("https://api.github.com/repos/{repo}/pulls/{number}/requested_reviewers"),
            json!({"reviewers": names}),
        ),
    };
    github_call(hub, method, &url, body).await
}

async fn github_call(hub: &Arc<Hub>, method: reqwest::Method, url: &str, body: Value) -> Result<()> {
    let token = crate::github::connection(&hub.store)?.context("GitHub is not configured.")?.token;
    let client = before_send(crate::egress::client(crate::egress::Policy::Only(&["api.github.com"]), url).await)?;
    let response = client
        .request(method, url)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .header("user-agent", "codync")
        .json(&body)
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
        Output { kind: kind.into(), target: target.into(), account: None, value: None }
    }

    #[test]
    fn each_kind_checks_its_destination() {
        assert!(problem(&output("slack", "C0123")).is_none());
        assert!(problem(&output("discord", "123")).is_none());
        assert!(problem(&output("discordReact", "👍")).is_none());
        assert!(problem(&output("discordReact", "two words")).is_some());
        assert!(problem(&output("telegram", "-100123")).is_none());
        let linked = |target: &str| Output {
            account: Some("5215555555556@s.whatsapp.net".into()),
            ..output("whatsapp", target)
        };
        assert!(problem(&linked("5215555555555@s.whatsapp.net")).is_none());
        assert!(problem(&linked("1203630@g.us")).is_none());
        assert!(problem(&output("whatsapp", "5215555555555")).is_some());
        assert!(problem(&output("whatsapp", "abc@s.whatsapp.net")).is_some());
        assert!(problem(&output("githubComment", "owner/repo")).is_none());
        assert!(problem(&output("slack", "not a channel")).is_some());
        assert!(problem(&output("telegram", "abc")).is_some());
        assert!(problem(&output("githubComment", "owner")).is_some());
        assert!(problem(&output("githubComment", "owner/repo/extra")).is_some());
        assert!(problem(&output("nope", "x")).is_some());
        assert!(problem(&output("slack", " ")).is_some());
    }

    #[test]
    fn whatsapp_outputs_require_distinct_recipient_and_linked_account_ids() {
        let mut report = output("whatsapp", "5215555555555@s.whatsapp.net");
        assert!(problem(&report).is_some(), "sender account is required");
        report.account = Some("5215555555556@s.whatsapp.net".into());
        assert!(problem(&report).is_none());
        report.target = "123-456@g.us".into();
        assert!(problem(&report).is_none());
        report.account = Some("123-456@g.us".into());
        assert!(problem(&report).is_some(), "a group cannot be the sending account");
        for target in ["bot-id", "123@other.net", "@g.us", "123@s.whatsapp.net?extra"] {
            report.target = target.into();
            assert!(problem(&report).is_some());
        }
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
