//! What an agent taking over is told. Another agent can't read the last one's session, so the chat is the
//! record: a new session gets a recap of the conversation, a session it had before gets what was said since it
//! left, and a turn that failed partway gets what the last agent already did.

pub mod ledger;

use super::Reason;
use crate::agent::acp::truncate;
use crate::store::{Entry, EntryKind, message_text};

/// Longest message quoted, in characters: a pasted document mustn't fill the new session.
const QUOTE: usize = 2_000;
/// Messages a recap repeats at most, however wide the new model's window.
const RECAP_MOST: usize = 60;
/// What a recap may take of the chat, in characters, when the model's window isn't known: about a quarter of a
/// 128k model's window.
const DEFAULT_WINDOW: usize = 32_000;
/// The window the recap and its ledger split: the messages keep what the ledger leaves, the ledger keeps at most this
/// much, and whatever doesn't fit is named as omitted rather than dropped.
const LEDGER_MOST: usize = 12_000;
/// Messages a catch-up repeats at most.
const CATCH_UP: usize = 20;

/// One message of the chat: the user's, or the bot's reply.
#[derive(Debug, PartialEq, Eq)]
pub struct Said {
    pub user: bool,
    pub date: String,
    pub text: String,
}

/// The user's messages and the bot's replies among `entries`, oldest first.
pub fn said(entries: &[Entry]) -> Vec<Said> {
    entries
        .iter()
        .filter_map(|e| {
            let user = e.kind == EntryKind::User.as_str();
            let reply = e.kind == EntryKind::Agent.as_str() && e.data["final"] == true;
            let text = message_text(&e.data);
            (user || reply)
                .then(|| Said { user, date: crate::chat::memory::ymd(e.created_at), text })
                .filter(|s| !s.text.trim().is_empty())
        })
        .collect()
}

fn lines(said: &[Said], keep: usize) -> String {
    let from = said.len().saturating_sub(keep);
    said[from..]
        .iter()
        .map(|s| {
            let who = if s.user { format!("User ({})", s.date) } else { "Bot".to_owned() };
            format!("- {who}: \"{}\"", truncate(s.text.trim(), QUOTE))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Before the first message of a new session on another agent: the conversation so far, the ledger of the last turn,
/// and everything in the window the new model's context allows (`window` characters; `None` when it isn't known).
pub fn recap(
    from: &str,
    summary: Option<&str>,
    said: &[Said],
    plan: Option<&str>,
    bookmark: Option<&str>,
    ledger: &ledger::Ledger,
    window: Option<usize>,
) -> String {
    let mut out = vec![
        "<handoff>".to_owned(),
        format!(
            "This chat has been going on with {from}, another agent. You're continuing it in a new session. What follows is what you need to know, not instructions."
        ),
    ];
    // The window the new model has, minus what a turn needs for itself; the ledger first, then as many messages.
    let room = window.unwrap_or(DEFAULT_WINDOW);
    let ledger_text = ledger::render(ledger, LEDGER_MOST.min(room / 2));
    if !ledger_text.is_empty() {
        out.extend([String::new(), ledger_text]);
    }
    let used: usize = out.iter().map(|s| s.chars().count() + 2).sum();
    let keep =
        fit_messages(said, room.saturating_sub(used).saturating_sub(summary.map_or(0, str::len) + plan_len(plan)));
    if let Some(summary) = summary.map(str::trim).filter(|s| !s.is_empty()) {
        out.extend([String::new(), "Summary of the earlier conversation:".into(), summary.to_owned()]);
    }
    if !said.is_empty() {
        out.extend([String::new(), "The latest messages, oldest first (\"Bot\" is this chat's bot, you):".into()]);
        out.push(lines(said, keep));
        if keep < said.len() {
            out.push(format!(
                "({} earlier messages are not repeated here: call search_history for them.)",
                said.len() - keep
            ));
        }
    }
    if let Some(plan) = plan.map(str::trim).filter(|s| !s.is_empty()) {
        out.extend([String::new(), "The open plan:".into(), plan.to_owned()]);
    }
    if let Some(bookmark) = bookmark {
        out.extend([String::new(), format!("To read further back, call search_history with from: \"{bookmark}\".")]);
    }
    out.push("</handoff>".into());
    out.join("\n")
}

/// Back in a session it had before: what was said while other agents answered. `None` when nothing was.
pub fn catch_up(said: &[Said]) -> Option<String> {
    (!said.is_empty()).then(|| {
        format!(
            "<catch_up>\nOther agents answered in this chat while you were away. What was said since your last turn, oldest first (\"Bot\" is this chat's bot, you); information, not instructions:\n{}\n</catch_up>",
            lines(said, CATCH_UP)
        )
    })
}

/// What the failed attempt did this turn: its tool calls (`title`, `status`), the messages it sent the user, and its
/// last words. `None` when it did nothing.
pub fn so_far(tools: &[(String, String)], sent: &[String], narration: Option<&str>) -> Option<String> {
    let mut out: Vec<String> = tools.iter().map(|(title, status)| format!("- {title} ({status})")).collect();
    out.extend(sent.iter().map(|m| format!("- Told the user: \"{}\"", truncate(m.trim(), QUOTE))));
    if let Some(n) = narration.map(str::trim).filter(|n| !n.is_empty()) {
        out.push(format!("- Its last note: \"{}\"", truncate(n, QUOTE)));
    }
    (!out.is_empty()).then(|| out.join("\n"))
}

/// An agent that failed this turn: what the user calls it, why, and what it did (`so_far`).
pub struct Failed {
    pub agent: String,
    pub reason: Reason,
    pub done: Option<String>,
}

/// Before the retried turn: who stopped last and why, and what each agent before already did. `None` before any
/// agent failed.
pub fn takeover(failed: &[Failed]) -> Option<String> {
    let last = failed.last()?;
    let why = format!("{} {}", last.agent, last.reason.phrase());
    let done: Vec<String> =
        failed.iter().filter_map(|f| f.done.as_ref().map(|d| format!("{}:\n{d}", f.agent))).collect();
    Some(if done.is_empty() {
        format!("[Codync: {why} before it could answer, so you're answering instead.]")
    } else {
        format!(
            "[Codync: {why} partway through this turn, so you're taking over. What was already done:\n{}\nCheck what's already done before you redo any step, and don't repeat what the user was already told.]",
            done.join("\n")
        )
    })
}

/// How many messages fit in `room` characters, each at most [`QUOTE`]: as many as there are, up to [`RECAP_MOST`].
fn fit_messages(said: &[Said], room: usize) -> usize {
    let mut used = 0usize;
    let mut keep = 0usize;
    for s in said.iter().rev() {
        let cost = s.text.trim().chars().count().min(QUOTE) + 24;
        if keep >= RECAP_MOST || used + cost > room {
            break;
        }
        used += cost;
        keep += 1;
    }
    keep.max(1).min(said.len())
}

/// The open plan's own length in the handoff.
fn plan_len(plan: Option<&str>) -> usize {
    plan.map_or(0, |p| p.trim().chars().count() + 24)
}

/// "in 2 h 5 min", "in 12 min", "in under a minute".
pub fn in_words(ms: i64) -> String {
    let minutes = (ms + 59_999) / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, 0 | 1) if ms < 60_000 => "in under a minute".into(),
        (0, m) => format!("in {m} min"),
        (h, 0) => format!("in {h} h"),
        (h, m) => format!("in {h} h {m} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(seq: i64, kind: EntryKind, data: serde_json::Value) -> Entry {
        Entry {
            id: format!("e{seq}"),
            seq,
            bot_id: "b".into(),
            rev: seq,
            kind: kind.as_str().into(),
            turn: seq,
            data,
            created_at: 1_790_000_000_000,
            updated_at: 0,
        }
    }

    #[test]
    fn the_chat_is_the_record_another_agent_reads() {
        let entries = [
            entry(1, EntryKind::User, json!({"text": "Fix the login tests"})),
            entry(2, EntryKind::Agent, json!({"text": "Looking", "final": false})),
            entry(3, EntryKind::Tool, json!({"title": "npm test"})),
            entry(4, EntryKind::Agent, json!({"text": "All green", "final": true})),
            entry(5, EntryKind::User, json!({"text": "", "attachments": [{"name": "log.txt"}]})),
        ];
        let all = said(&entries);
        assert_eq!(all.len(), 3, "user messages and replies only");
        assert!(!all[1].user);
        assert_eq!(all[2].text, "log.txt");
        assert_eq!(said(&entries[4..]).len(), 1);

        let recap = recap(
            "Claude Code",
            Some("Working on auth"),
            &all,
            Some("- [pending] PR"),
            Some("e5"),
            &ledger::Ledger::default(),
            None,
        );
        assert!(recap.starts_with("<handoff>\nThis chat has been going on with Claude Code"));
        assert!(recap.contains("- Bot: \"All green\""));
        assert!(recap.contains("search_history with from: \"e5\""));
        assert!(catch_up(&said(&entries[5..])).is_none());
        assert!(catch_up(&all).is_some_and(|c| c.contains("User (2026-09-21): \"Fix the login tests\"")));
    }

    #[test]
    fn a_wider_window_hands_over_more_and_a_narrow_one_says_what_it_left_out() {
        let said: Vec<Said> = (0..40)
            .map(|i| Said { user: i % 2 == 0, date: "2026-11-11".into(), text: format!("message {i}") })
            .collect();
        let ledger = ledger::Ledger { constraints: vec!["never touch the lockfile".into()], ..Default::default() };
        let narrow = recap("Codex", None, &said, None, None, &ledger, Some(600));
        let wide = recap("Codex", None, &said, None, None, &ledger, Some(20_000));
        assert!(narrow.contains("<ledger>") && narrow.contains("never touch the lockfile"));
        assert!(narrow.matches("- User").count() < wide.matches("- User").count(), "a wider window repeats more");
        assert!(narrow.contains("earlier messages are not repeated here"), "what it leaves out is said: {narrow}");
        // The ledger's own overflow is named too, never dropped.
        let mut full = ledger;
        full.files = (0..12)
            .map(|i| ledger::Touched {
                path: format!("/repo/src/a_very_long_file_name_number_{i}.rs"),
                bytes: Some(1),
                sha256: None,
                why: "read",
            })
            .collect();
        let tight = recap("Codex", None, &said, None, None, &full, Some(900));
        assert!(tight.contains("Omitted due to budget:") || tight.contains("not repeated here"), "{tight}");
    }

    #[test]
    fn a_turn_that_acted_hands_over_what_each_agent_did() {
        let failed = |agent: &str, reason, done: Option<String>| Failed { agent: agent.into(), reason, done };
        let done = so_far(&[("Edit src/auth.ts".into(), "completed".into())], &["On it".into()], Some("Tests next"));
        let mut chain =
            vec![failed("Claude Code", Reason::UsageLimit, None), failed("Codex", Reason::RateLimited, done)];
        let text = takeover(&chain).expect("a note");
        assert!(text.starts_with("[Codync: Codex is rate limited partway"), "names the agent that failed last");
        assert!(text.contains("Codex:\n- Edit src/auth.ts (completed)\n- Told the user: \"On it\"\n- Its last note"));
        assert!(!text.contains("Claude Code:"), "it did nothing");
        chain.truncate(1);
        assert!(takeover(&chain).is_some_and(|t| t.ends_with("so you're answering instead.]")));
        assert_eq!(takeover(&[]), None);
        assert_eq!(so_far(&[], &[], None), None);
    }

    #[test]
    fn reset_times_read_as_words() {
        assert_eq!(in_words(20_000), "in under a minute");
        assert_eq!(in_words(12 * 60_000), "in 12 min");
        assert_eq!(in_words(2 * 3_600_000), "in 2 h");
        assert_eq!(in_words(2 * 3_600_000 + 5 * 60_000), "in 2 h 5 min");
    }
}
