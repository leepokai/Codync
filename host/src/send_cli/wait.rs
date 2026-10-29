//! `--wait`: follows the turn that answers the sent message until the bot settles, and never stops it.

use super::events::{BotFeed, Progress, Settled};
use super::{Failure, Report, SendArgs, Status, StopReason, redact};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

pub(super) async fn reply(
    args: &SendArgs,
    token: &str,
    feed: &mut BotFeed,
    report: &mut Report,
) -> Result<(), Failure> {
    let mut printed = HashSet::new();
    loop {
        let progress = feed.progress(&report.entry_id)?;
        report.elapsed_seconds = progress.elapsed.as_secs_f64();
        report.waited_for_approval_seconds = progress.approval.as_secs_f64();
        if let Some(turn) = progress.turn {
            report.turn = turn;
        }
        print_new(&progress, report, token, &mut printed);
        if finish(&progress, report) {
            return Ok(());
        }
        // An approval the bot waits for stops the clock.
        let left = (!progress.pending).then(|| {
            Duration::from_secs(args.timeout).saturating_sub(progress.elapsed.saturating_sub(progress.approval))
        });
        if left.is_some_and(|left| left.is_zero()) {
            report.status = Status::Timeout;
            eprintln!("timed out after {}s (the bot is still working; nothing was stopped)", args.timeout);
            return Ok(());
        }
        let deadline = async {
            match left {
                Some(left) => tokio::time::sleep(left).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            result = feed.changed() => result?,
            () = deadline => {}
        }
    }
}

/// Says, once each, the approvals the bot waits for and the notices of the answering turn.
fn print_new(progress: &Progress, report: &mut Report, token: &str, printed: &mut HashSet<String>) {
    for card in progress.approvals.iter().filter(|_| progress.pending) {
        if let Some(id) = card["id"].as_str()
            && printed.insert(id.to_owned())
        {
            let title =
                card["data"]["title"].as_str().or_else(|| card["data"]["command"].as_str()).unwrap_or("Use a tool");
            let summary: String = title.lines().next().unwrap_or("Use a tool").chars().take(180).collect();
            eprintln!("waiting for approval: {}", redact(&summary, token));
        }
    }
    for notice in turn_entries(progress, report.entry_seq).filter(|e| e["kind"] == "notice") {
        if let (Some(id), Some(text)) = (notice["id"].as_str(), notice["data"]["text"].as_str())
            && printed.insert(id.to_owned())
        {
            report.note(redact(text, token));
        }
    }
}

/// Whether the wait is over, with the report filled in: the bot is gone, the message left the queue unsent, or the
/// turn that took it ended.
fn finish(progress: &Progress, report: &mut Report) -> bool {
    if progress.bot.is_null() || progress.bot["deleted"] == true {
        return failed(report, StopReason::Error, "The bot was deleted while waiting.");
    }
    let own = progress.entries.iter().find(|e| e["id"] == report.entry_id.as_str());
    if own.is_some_and(|e| e["data"]["status"] == "cancelled") {
        return failed(report, StopReason::Cancelled, "The queued message was cancelled.");
    }
    let Some(settled) = progress.settled else { return false };
    let entries: Vec<&Value> = turn_entries(progress, report.entry_seq).collect();
    // Several messages (send_message bubbles, an artifact card) make one reply, as the host joins them.
    let replies: Vec<&Value> =
        entries.iter().copied().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true).collect();
    let texts: Vec<&str> = replies.iter().filter_map(|e| e["data"]["text"].as_str()).collect();
    report.reply = (!texts.is_empty()).then(|| texts.join("\n\n"));
    report.reply_id = replies.last().and_then(|e| e["id"].as_str()).map(str::to_owned);
    let reason = entries
        .iter()
        .rev()
        .find_map(|e| serde_json::from_value(e["data"]["stopReason"].clone()).ok())
        .unwrap_or(if settled == Settled::Error { StopReason::Error } else { StopReason::EndTurn });
    report.stop_reason = Some(reason);
    if reason != StopReason::EndTurn {
        report.status = Status::Failed;
        if report.notices.is_empty() {
            report.note("The bot entered an error state.".into());
        }
        return true;
    }
    report.status = Status::Completed;
    // A reaction can be the bot's whole answer.
    let reaction = own
        .and_then(|e| e["data"]["botReactions"].as_array())
        .and_then(|reactions| reactions.iter().find(|r| r["bot"] == report.bot_id.as_str()))
        .and_then(|r| r["emoji"].as_str());
    if report.reply.is_none()
        && let Some(emoji) = reaction
    {
        report.note(format!("Reacted {emoji} to the message."));
    }
    true
}

fn failed(report: &mut Report, reason: StopReason, notice: &str) -> bool {
    report.status = Status::Failed;
    report.stop_reason = Some(reason);
    report.note(notice.to_owned());
    true
}

/// The entries of the turn that took the sent message (`sent` its seq), after it, oldest first.
fn turn_entries(progress: &Progress, sent: i64) -> impl Iterator<Item = &Value> {
    let turn = progress.turn;
    progress.entries.iter().filter(move |e| {
        turn.is_some() && e["turn"].as_i64() == turn && e["seq"].as_i64().is_some_and(|seq| seq > sent)
    })
}
