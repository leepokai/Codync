use super::{Args, Failure, Report, Status};
use crate::cli::api::Api;
use crate::cli::events::{BotFeed, Progress};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::future::Future;
use std::time::Duration;
use tokio::time::Instant;

pub(super) async fn reply(
    args: &Args,
    api: &Api,
    feed: &mut BotFeed,
    entry: &Value,
    report: &mut Report,
) -> Result<(), Failure> {
    let mut printed = HashSet::new();
    let mut approvals = HashSet::new();
    let mut next_poll = Instant::now();
    loop {
        let progress = feed.progress(entry)?;
        update_report(&progress, report);
        print_approvals(&progress, api, &mut approvals);
        if finish(&progress, entry, report, api, &mut printed) {
            return Ok(());
        }
        if expired(args, &progress) {
            timed_out(args, report);
            return Ok(());
        }
        if Instant::now() >= next_poll {
            let entries = history(args, api, feed, entry, &report.bot_id).await?;
            if let Some(entries) = entries {
                feed.merge(&entries);
            } else {
                update_report(&feed.progress(entry)?, report);
                timed_out(args, report);
                return Ok(());
            }
            next_poll = Instant::now() + Duration::from_secs(1);
        } else {
            let now = Instant::now();
            let delay = remaining(args, &progress)
                .map_or(next_poll, |left| now + left.min(next_poll.saturating_duration_since(now)));
            tokio::select! {
                result = feed.changed() => result?,
                () = tokio::time::sleep_until(delay) => {}
            }
        }
    }
}

fn remaining(args: &Args, progress: &Progress) -> Option<Duration> {
    (!progress.pending)
        .then(|| Duration::from_secs(args.timeout).saturating_sub(progress.elapsed.saturating_sub(progress.approval)))
}

fn expired(args: &Args, progress: &Progress) -> bool {
    remaining(args, progress).is_some_and(|left| left.is_zero())
}

fn update_report(progress: &Progress, report: &mut Report) {
    report.elapsed_seconds = progress.elapsed.as_secs_f64();
    report.waited_for_approval_seconds = progress.approval.as_secs_f64();
    if let Some(turn) = progress.turn {
        report.turn = turn;
    }
}

fn timed_out(args: &Args, report: &mut Report) {
    report.status = Status::Timeout;
    eprintln!("timed out after {}s (the bot is still working; nothing was stopped)", args.timeout);
}

fn print_approvals(progress: &Progress, api: &Api, printed: &mut HashSet<String>) {
    if !progress.pending {
        return;
    }
    for card in &progress.approvals {
        if let Some(id) = card["id"].as_str()
            && printed.insert(id.to_owned())
        {
            let title =
                card["data"]["title"].as_str().or_else(|| card["data"]["command"].as_str()).unwrap_or("Use a tool");
            let line = title.lines().next().unwrap_or("Use a tool");
            let summary: String = line.chars().take(180).collect();
            eprintln!("waiting for approval: {}", api.redact(&summary));
        }
    }
}

fn finish(progress: &Progress, sent: &Value, report: &mut Report, api: &Api, printed: &mut HashSet<String>) -> bool {
    if progress.bot.is_null() || progress.bot["deleted"] == true {
        report.notices.push("The bot was deleted while waiting.".into());
        report.status = Status::Failed;
        report.stop_reason = Some("error".into());
        eprintln!("The bot was deleted while waiting.");
        return true;
    }
    let own = progress.entries.iter().find(|e| e["id"] == sent["id"]);
    if own.is_some_and(|e| matches!(e["data"]["status"].as_str(), Some("cancelled" | "failed"))) {
        report.status = Status::Failed;
        report.stop_reason =
            Some(if own.is_some_and(|e| e["data"]["status"] == "cancelled") { "cancelled" } else { "error" }.into());
        report.notices.push("The queued message was cancelled or failed.".into());
        eprintln!("The queued message was cancelled or failed.");
        return true;
    }
    let Some(turn) = progress.turn else { return false };
    for e in progress
        .entries
        .iter()
        .filter(|e| e["seq"].as_i64().is_some_and(|seq| seq > report.entry_seq) && e["turn"] == turn)
    {
        if e["kind"] == "agent" && e["data"]["final"] == true {
            report.reply = e["data"]["text"].as_str().map(str::to_owned);
        } else if e["kind"] == "notice"
            && let Some(text) = e["data"]["text"].as_str()
        {
            if let Some(id) = e["id"].as_str()
                && printed.insert(id.to_owned())
            {
                report.notices.push(api.redact(text));
                eprintln!("{}", api.redact(text));
            }
            if let Some(reason) = failure_reason(text) {
                report.stop_reason = Some(reason.into());
            }
        }
    }
    if report.stop_reason.is_some() || progress.settled.as_deref() == Some("error") {
        report.status = Status::Failed;
        report.stop_reason.get_or_insert_with(|| "error".into());
        if report.notices.is_empty() {
            report.notices.push("The bot entered an error state.".into());
            eprintln!("The bot entered an error state.");
        }
        return true;
    }
    if report.reply.is_some() && progress.settled.as_deref() == Some("idle") {
        report.status = Status::Completed;
        report.stop_reason = Some("end_turn".into());
        return true;
    }
    false
}

fn failure_reason(text: &str) -> Option<&'static str> {
    match text {
        "Stopped." => Some("cancelled"),
        "The agent hit its output limit." => Some("max_tokens"),
        "The agent hit its step limit for this turn." => Some("max_turn_requests"),
        "The agent declined to continue." => Some("refusal"),
        _ if text.starts_with("Couldn't start the agent:") => Some("start_failed"),
        _ if text.starts_with("The agent failed:") => Some("error"),
        _ => None,
    }
}

async fn bounded<T>(
    args: &Args,
    feed: &mut BotFeed,
    entry: &Value,
    request: impl Future<Output = Result<T, Failure>>,
) -> Result<Option<T>, Failure> {
    tokio::pin!(request);
    loop {
        let progress = feed.progress(entry)?;
        if expired(args, &progress) {
            return Ok(None);
        }
        let deadline = async {
            match remaining(args, &progress) {
                Some(left) => tokio::time::sleep(left.min(Duration::from_secs(20))).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            result = &mut request => return result.map(Some),
            result = feed.changed() => result?,
            () = deadline => {}
        }
    }
}

async fn history(
    args: &Args,
    api: &Api,
    feed: &mut BotFeed,
    sent: &Value,
    bot: &str,
) -> Result<Option<Vec<Value>>, Failure> {
    let mut entries = Vec::new();
    let mut before = i64::MAX;
    loop {
        let value =
            bounded(args, feed, sent, api.call("history", json!({"botId":bot,"limit":100,"beforeSeq":before}))).await?;
        let Some(value) = value else { return Ok(None) };
        let page = value["entries"].as_array().ok_or_else(|| Failure::api("the host returned invalid history"))?;
        let oldest = page.iter().filter_map(|e| e["seq"].as_i64()).min();
        entries.extend(page.iter().cloned());
        match oldest {
            Some(seq) if seq > sent["seq"].as_i64().unwrap_or(0) && seq < before && page.len() == 100 => before = seq,
            _ => return Ok(Some(entries)),
        }
    }
}
