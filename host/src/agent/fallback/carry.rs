//! What an agent taking over is told. Another agent can't read the last one's session, so the chat is the
//! record: a new session gets a recap of the conversation, a session it had before gets what was said since it
//! left, both with a ledger of what the agents before changed and ran (`ledger`), and a turn that failed partway
//! gets what the last agent already did. A handoff is an index sized to the next agent's window: it points at the
//! history and the files instead of repeating them.

pub mod index;
pub mod ledger;

use super::Reason;
use crate::agent::acp::truncate;
use crate::store::{Entry, EntryKind, message_text};

/// Longest message quoted, in bytes: a pasted document mustn't fill the new session.
const QUOTE: usize = 2_000;
/// Longest earlier message of the user's a recap repeats, in bytes: about 300 tokens, which is 300 CJK characters.
const EARLIER_QUOTE: usize = 900;
/// Messages a recap repeats in full, newest last.
const RECAP: usize = 12;
/// Messages a catch-up repeats at most.
const CATCH_UP: usize = 20;
/// What a handoff may take of the next agent's window: a tenth of it, and never more than this many tokens. An agent
/// with a wider window reads further back itself (`search_history`).
const WINDOW_SHARE: u64 = 10;
const MOST_TOKENS: u64 = 20_000;
/// A conservative target window when the next agent has not reported one yet. Its one-tenth handoff
/// allowance is still bounded and uses the same explicit omission labels as a reported window.
const UNKNOWN_WINDOW: u64 = 32_000;
/// Bytes counted as a token: a CJK character is three bytes and about a token, and Latin text has more bytes to a
/// token, so the count errs on the side of a smaller handoff.
const TOKEN_BYTES: usize = 3;
/// What a quoted message's line adds around its text, in bytes.
const LINE_OVERHEAD: usize = 32;

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
            let text = message_text(&e.data);
            e.is_message()
                .then(|| Said { user, date: crate::chat::memory::ymd(e.created_at), text })
                .filter(|s| !s.text.trim().is_empty())
        })
        .collect()
}

/// `said` as quoted lines, each message at most `quote` characters.
fn lines<'a>(said: impl IntoIterator<Item = &'a Said>, quote: usize) -> String {
    said.into_iter()
        .map(|s| {
            let who = if s.user { format!("User ({})", s.date) } else { "Bot".to_owned() };
            format!("- {who}: \"{}\"", truncate(s.text.trim(), quote))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The newest `most` of `said`.
fn latest(said: &[Said], most: usize) -> &[Said] {
    &said[said.len().saturating_sub(most)..]
}

/// What a handoff may take, in bytes, of an agent whose window holds `window` tokens.
pub fn room(window: u64) -> usize {
    usize::try_from((window / WINDOW_SHARE).min(MOST_TOKENS)).unwrap_or(usize::MAX).saturating_mul(TOKEN_BYTES)
}

/// The messages a recap repeats: the latest in full, and before them the user's own, shortened.
struct Repeated<'a> {
    earlier: Vec<&'a Said>,
    latest: &'a [Said],
}

/// What of `said` fits `room` bytes, newest first: up to [`RECAP`] messages in full, then the
/// user's earlier ones, shortened, as far back as they fit. The user's words are the brief; the bot's earlier replies
/// are in the history. With no `room` known, the latest [`RECAP`] alone.
fn repeated(said: &[Said], room: Option<usize>) -> Repeated<'_> {
    let Some(mut room) = room else {
        return Repeated { earlier: Vec::new(), latest: latest(said, RECAP) };
    };
    let mut take = |s: &Said, quote: usize| {
        let cost = truncate(s.text.trim(), quote).len() + LINE_OVERHEAD;
        let taken = cost <= room;
        if taken {
            room = room.saturating_sub(cost);
        }
        taken
    };
    let mut from = said.len();
    while from > 0 && said.len() - from < RECAP && take(&said[from - 1], QUOTE) {
        from -= 1;
    }
    let mut earlier: Vec<&Said> =
        said[..from].iter().rev().filter(|s| s.user).take_while(|s| take(s, EARLIER_QUOTE)).collect();
    earlier.reverse();
    Repeated { earlier, latest: &said[from..] }
}

/// Before the first message of a new session on another agent: the conversation so far, and what the agents before
/// left behind.
#[derive(Default)]
pub struct Recap<'a> {
    /// What the user calls the agent the chat has been going on with.
    pub from: &'a str,
    /// The agent resumed its existing session: render this as a catch-up rather than a new-session recap.
    pub catching_up: bool,
    /// The last summary of the session it takes over from.
    pub summary: Option<&'a str>,
    pub said: &'a [Said],
    /// The open plan.
    pub plan: Option<&'a str>,
    /// The message `search_history` reads back from.
    pub bookmark: Option<&'a str>,
    /// What the agents before changed and ran (`ledger`).
    pub ledger: Option<&'a str>,
    /// What it may take, in bytes ([`room`]); `None` when the agent's window isn't known, in which case
    /// [`UNKNOWN_WINDOW`] supplies a conservative bounded allowance.
    pub room: Option<usize>,
    /// The mechanical context manifest. Its rows and omission labels share the recap's byte allowance.
    pub manifest: Option<&'a ledger::Ledger>,
}

impl Recap<'_> {
    /// The entire recap, including headings, summaries and omission notices, fits the known target allowance.
    /// A window too small even for the notice must fail the handoff, never silently start without its context.
    pub fn text(&self) -> anyhow::Result<String> {
        let has_manifest = self.manifest.is_some_and(|manifest| manifest.rows_len() + manifest.omitted.len() > 0);
        if self.said.is_empty()
            && self.summary.is_none_or(|value| value.trim().is_empty())
            && self.plan.is_none_or(|value| value.trim().is_empty())
            && self.ledger.is_none()
            && self.bookmark.is_none()
            && !has_manifest
        {
            return Ok(String::new());
        }
        // The manifest owns its collection-stage labels while it is rendered; this list names sections
        // dropped from the whole recap, and takes those labels over when the whole manifest is dropped.
        let omitted = Vec::new();
        let budget = self.room.unwrap_or_else(|| room(UNKNOWN_WINDOW));
        let mut candidate = Self { ..*self };
        let mut omitted = omitted;
        let mut messages = budget;
        loop {
            let text = candidate.render(Some(messages), &omitted)?;
            if text.len() <= budget {
                return Ok(text);
            }
            if messages > 0 {
                messages = messages.saturating_sub(text.len() - budget);
                continue;
            }
            // A section that cannot fit is named with its original byte count and the reason, never silently dropped.
            let Some(manifest) = candidate.manifest else {
                let largest = [candidate.summary, candidate.plan, candidate.ledger, candidate.bookmark]
                    .iter()
                    .enumerate()
                    .filter_map(|(i, value)| value.map(|v| (i, v.len())))
                    .max_by_key(|(_, bytes)| *bytes);
                let Some((index, bytes)) = largest else {
                    anyhow::bail!(
                        "the target's {budget}-byte handoff allowance cannot hold its omission notice; no prompt was sent"
                    );
                };
                let name = match index {
                    0 => {
                        candidate.summary = None;
                        "summary"
                    }
                    1 => {
                        candidate.plan = None;
                        "open plan"
                    }
                    2 => {
                        candidate.ledger = None;
                        "ledger"
                    }
                    _ => {
                        candidate.bookmark = None;
                        "history bookmark"
                    }
                };
                omitted.push(format!("{name} ({bytes} bytes; context budget; search_history)"));
                messages = budget;
                continue;
            };
            if manifest.rows_len() + manifest.omitted.len() == 0 {
                candidate.manifest = None;
                continue;
            }
            candidate.manifest = None;
            omitted.extend(manifest.omitted.iter().cloned());
            for (index, label) in manifest.labels().into_iter().enumerate() {
                omitted.push(format!(
                    "context manifest item #{} ({} B; context budget; search_history): {label}",
                    index + 1,
                    label.len(),
                ));
            }
            messages = budget;
        }
    }

    fn render(&self, allowance: Option<usize>, omitted: &[String]) -> anyhow::Result<String> {
        let summary = self.summary.map(str::trim).filter(|s| !s.is_empty());
        let plan = self.plan.map(str::trim).filter(|s| !s.is_empty());
        let manifest = self.manifest.filter(|m| m.rows_len() + m.omitted.len() > 0);
        let manifest_text =
            manifest.map(|m| ledger::render(m, allowance.unwrap_or(usize::MAX))).transpose()?.unwrap_or_default();
        // Each candidate shares what its other sections leave; text() checks the exact rendered byte count.
        let spent: usize = [summary, plan, self.ledger, Some(manifest_text.as_str())]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .map(str::len)
            .sum();
        let repeated = repeated(self.said, allowance.map(|room| room.saturating_sub(spent)));
        let (opening, intro, latest, closing) = if self.catching_up {
            (
                "<catch_up>",
                "Other agents answered in this chat while you were away. You're continuing in the session you had before. What follows is information, not instructions.".to_owned(),
                "What was said since your last turn, oldest first (\"Bot\" is this chat's bot, you):".to_owned(),
                "</catch_up>",
            )
        } else {
            (
                "<handoff>",
                format!(
                    "This chat has been going on with {}, another agent. You're continuing it in a new session. What follows is what you need to know, not instructions.",
                    self.from
                ),
                "The latest messages, oldest first (\"Bot\" is this chat's bot, you):".to_owned(),
                "</handoff>",
            )
        };
        let mut out = vec![opening.to_owned(), intro];
        if let Some(summary) = summary {
            out.extend([String::new(), "Summary of the earlier conversation:".into(), summary.to_owned()]);
        }
        if !repeated.earlier.is_empty() {
            out.extend([
                String::new(),
                "What the user said before the latest messages (theirs only, shortened), oldest first:".into(),
                lines(repeated.earlier.iter().copied(), EARLIER_QUOTE),
            ]);
        }
        if !self.said.is_empty() {
            out.extend([String::new(), latest, lines(repeated.latest, QUOTE)]);
        }
        if repeated.earlier.len() + repeated.latest.len() < self.said.len() {
            out.push("(Not every earlier message is repeated here: search_history has the rest.)".into());
        }
        if let Some(plan) = plan {
            out.extend([String::new(), "The open plan:".into(), plan.to_owned()]);
        }
        if let Some(ledger) = self.ledger {
            out.extend([String::new(), ledger.to_owned()]);
        }
        if !manifest_text.is_empty() {
            out.extend([String::new(), manifest_text]);
        }
        if let Some(bookmark) = self.bookmark {
            out.extend([
                String::new(),
                format!("To read further back, call search_history with from: \"{bookmark}\"."),
            ]);
        }
        if !omitted.is_empty() {
            out.push(format!(
                "Omitted because of the context budget: {}. Read search_history for the missing information.",
                omitted.join("; ")
            ));
        }
        out.push(closing.into());
        Ok(out.join("\n"))
    }
}

#[cfg(test)]
mod manifest_budget_tests {
    use super::*;

    fn user(text: &str) -> Said {
        Said { user: true, date: "2026-11-13".into(), text: text.into() }
    }

    #[test]
    fn the_manifest_shares_the_recap_budget_and_names_what_it_could_not_expand() {
        let manifest = ledger::Ledger {
            files: (0..4)
                .map(|i| ledger::Touched {
                    path: format!("src/changed-{i}.rs"),
                    bytes: Some(4096),
                    sha256: Some("a".repeat(64)),
                    why: "modified",
                    unavailable: None,
                })
                .collect(),
            ..ledger::Ledger::default()
        };
        let said = [user("Continue")];
        let recap =
            Recap { from: "Codex", said: &said, room: Some(900), manifest: Some(&manifest), ..Recap::default() };
        let text = recap.text().expect("a bounded handoff");
        assert!(text.len() <= 900, "{} of 900", text.len());
        assert!(text.contains("<ledger>"), "{text}");
        assert!(text.contains("context budget") || text.contains("labels not expanded"), "{text}");
        assert!(text.contains("src/changed-"), "{text}");
        assert!(text.contains("bytes; modified") || text.contains("4096 B"), "{text}");
    }

    #[test]
    fn collection_omissions_are_named_once_in_the_manifest() {
        let manifest = ledger::Ledger {
            omitted: vec!["src/overflow.rs (12 B; collection limit)".into()],
            ..ledger::Ledger::default()
        };
        let said = [user("Continue")];
        let recap =
            Recap { from: "Codex", said: &said, room: Some(1_500), manifest: Some(&manifest), ..Recap::default() };
        let text = recap.text().expect("a bounded handoff");
        assert!(text.contains("src/overflow.rs"), "{text}");
        assert_eq!(text.matches("src/overflow.rs").count(), 1, "no duplicate label: {text}");
        assert!(!text.contains("Omitted because of the context budget"), "{text}");
    }

    #[test]
    fn an_unknown_window_uses_a_conservative_budget_and_names_omissions() {
        let summary = "s".repeat(12_000);
        let manifest = ledger::Ledger {
            files: vec![ledger::Touched {
                path: "src/continuing.rs".into(),
                bytes: Some(42),
                sha256: Some("a".repeat(64)),
                why: "modified",
                unavailable: None,
            }],
            ..ledger::Ledger::default()
        };
        let said = [user("Continue")];
        let recap = Recap {
            from: "Codex",
            summary: Some(&summary),
            said: &said,
            manifest: Some(&manifest),
            ..Recap::default()
        };
        let text = recap.text().expect("an unknown window still gets a bounded handoff");
        assert!(text.len() <= room(UNKNOWN_WINDOW), "{} bytes", text.len());
        assert!(!text.contains(&summary), "the oversized summary must not be inserted whole");
        assert!(text.contains("summary (12000 bytes; context budget; search_history)"), "{text}");
        assert!(text.contains("src/continuing.rs") && text.contains("<ledger>"), "{text}");
    }

    #[test]
    fn resuming_a_session_uses_its_window_and_names_bounded_manifest_items() {
        let manifest = ledger::Ledger {
            files: (0..4)
                .map(|i| ledger::Touched {
                    path: format!("src/resumed-{i}.rs"),
                    bytes: Some(4096),
                    sha256: Some("a".repeat(64)),
                    why: "modified",
                    unavailable: None,
                })
                .collect(),
            ..ledger::Ledger::default()
        };
        let said = [user("Continue")];
        let text = catch_up(&said, None, Some(&manifest), Some(900))
            .expect("a bounded catch-up")
            .expect("the resumed session has context");
        assert!(text.len() <= 900, "{} of 900", text.len());
        assert!(text.starts_with("<catch_up>") && text.ends_with("</catch_up>"), "{text}");
        assert!(text.contains("src/resumed-"), "{text}");
        assert!(text.contains("context budget") || text.contains("labels not expanded"), "{text}");
    }
}

/// Back in a session it had before: what was said while other agents answered, and the `ledger` of what they left
/// behind. `None` when nothing happened.
pub fn catch_up(
    said: &[Said],
    ledger: Option<&str>,
    manifest: Option<&ledger::Ledger>,
    room: Option<usize>,
) -> anyhow::Result<Option<String>> {
    let recap = Recap {
        catching_up: true,
        said: &said[said.len().saturating_sub(CATCH_UP)..],
        ledger,
        manifest,
        room,
        ..Recap::default()
    };
    let text = recap.text()?;
    Ok((!text.is_empty()).then_some(text))
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

/// What an agent was told, for a chat line to unfold: all of it in `most` bytes, otherwise its start and its end. A
/// long handoff loses its middle, the messages the chat shows anyway; what only it says comes last (the plan, the
/// ledger, what was done this turn).
pub fn excerpt(text: &str, most: usize) -> String {
    if text.len() <= most {
        return text.to_owned();
    }
    let start = most / 4;
    let (head, tail) = (text.floor_char_boundary(start), text.ceil_char_boundary(text.len() - (most - start)));
    format!("{}\n…\n{}", &text[..head], &text[tail..])
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

        let recap = Recap {
            from: "Claude Code",
            summary: Some("Working on auth"),
            said: &all,
            plan: Some("- [pending] PR"),
            bookmark: Some("e5"),
            ..Recap::default()
        }
        .text()
        .unwrap();
        assert!(recap.starts_with("<handoff>\nThis chat has been going on with Claude Code"));
        assert!(recap.contains("- Bot: \"All green\""));
        assert!(recap.contains("search_history with from: \"e5\""));
        assert!(!recap.contains("<ledger>") && !recap.contains("Not every earlier message"), "{recap}");
        assert!(catch_up(&said(&entries[5..]), None, None, None).unwrap().is_none());
        assert!(
            catch_up(&all, None, None, None)
                .unwrap()
                .is_some_and(|c| c.contains("User (2026-09-21): \"Fix the login tests\""))
        );
    }

    /// A recap of `said` alone, for an agent with `room`.
    fn recap(said: &[Said], room: Option<usize>) -> String {
        Recap { from: "Codex", said, room, ..Recap::default() }.text().unwrap()
    }

    fn chat(messages: usize, text: &str) -> Vec<Said> {
        (0..messages)
            .map(|i| Said { user: i % 2 == 0, date: "2026-11-11".into(), text: format!("{text} {i}") })
            .collect()
    }

    #[test]
    fn a_handoff_takes_a_tenth_of_the_window_and_never_more_than_its_cap() {
        assert_eq!(room(32_000), 3_200 * TOKEN_BYTES);
        assert_eq!(room(200_000), 20_000 * TOKEN_BYTES);
        assert_eq!(room(1_000_000), room(200_000), "a wider window reads the history itself");
    }

    #[test]
    fn with_no_window_known_the_latest_messages_go_as_ever() {
        let said = chat(40, "message");
        let recap = recap(&said, None);
        assert_eq!(recap.matches("\n- ").count(), RECAP, "{recap}");
        assert!(recap.contains("\"message 39\"") && !recap.contains("\"message 27\""), "{recap}");
        assert!(!recap.contains("What the user said before"), "{recap}");
        assert!(recap.contains("Not every earlier message is repeated here"), "what it leaves out is said: {recap}");
    }

    #[test]
    fn a_known_window_adds_the_users_earlier_words_and_stays_inside_it() {
        let said = chat(40, "message");
        let wide = recap(&said, Some(room(200_000)));
        assert!(wide.contains("What the user said before the latest messages"), "{wide}");
        assert!(wide.contains("- User (2026-11-11): \"message 0\""), "back to the user's first: {wide}");
        assert!(!wide.contains("- Bot: \"message 1\""), "the bot's earlier replies stay in the history: {wide}");
        assert!(wide.find("\"message 0\"") < wide.find("\"message 39\""), "oldest first: {wide}");
        assert_eq!(wide.matches("- Bot:").count(), RECAP / 2, "{wide}");

        // A narrow one repeats less, including all framing inside the allowance.
        let narrow = recap(&said, Some(500));
        assert!(narrow.matches("\n- ").count() < RECAP && narrow.contains("\"message 39\""), "{narrow}");
        assert!(narrow.len() <= 500);
        assert!(Recap { from: "Codex", said: &said, room: Some(0), ..Recap::default() }.text().is_err());
    }

    #[test]
    fn the_room_is_counted_in_bytes_so_cjk_text_fills_it_sooner() {
        let latin = chat(60, &"a".repeat(600));
        let cjk = chat(60, &"字".repeat(600));
        let room = room(60_000);
        let (latin, cjk) = (recap(&latin, Some(room)), recap(&cjk, Some(room)));
        assert!(cjk.matches("\n- ").count() < latin.matches("\n- ").count(), "{}", cjk.len());
        // The quoted messages stay inside the room, whatever the script.
        assert!(cjk.len() <= room, "{} of {room}", cjk.len());
        assert!(latin.len() <= room, "{} of {room}", latin.len());
    }

    #[test]
    fn sections_that_fit_leave_the_rest_of_the_complete_budget_for_messages() {
        let said = chat(40, &"word ".repeat(40));
        let ledger = "<ledger>\nFiles changed, newest first:\n- /repo/a.rs (modified)\n</ledger>";
        let summary = "s".repeat(4_000);
        let with = Recap {
            from: "Codex",
            summary: Some(&summary),
            said: &said,
            plan: Some("- [pending] PR"),
            ledger: Some(ledger),
            room: Some(6_000),
            ..Recap::default()
        }
        .text()
        .unwrap();
        let without = recap(&said, Some(6_000));
        assert!(with.contains(ledger) && with.contains(&summary) && with.contains("- [pending] PR"), "{with}");
        assert!(with.matches("\n- User").count() < without.matches("\n- User").count(), "{with}");
        assert!(with.find("The open plan:") < with.find("<ledger>"), "what was done follows what was said: {with}");
        assert!(with.len() <= 6_000);
    }

    #[test]
    fn a_catch_up_names_what_the_other_agents_left_behind() {
        let ledger = "<ledger>\nFiles changed, newest first:\n- /repo/a.rs (modified)\n</ledger>";
        let said = chat(2, "message");
        let text = catch_up(&said, Some(ledger), None, None).unwrap().expect("a catch-up");
        assert!(text.starts_with("<catch_up>\nOther agents answered") && text.ends_with("</ledger>\n</catch_up>"));
        assert!(text.contains("- User (2026-11-11): \"message 0\""), "{text}");
        // Tool calls with no reply (a turn that failed) are still news.
        assert!(catch_up(&[], Some(ledger), None, None).unwrap().is_some_and(|c| !c.contains("What was said")));
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
    fn a_chat_line_keeps_the_start_and_the_end_of_a_long_handoff() {
        let said = chat(120, &"字".repeat(200));
        let ledger = "<ledger>\nFiles changed, newest first:\n- /repo/a.rs (modified)\n</ledger>";
        let recap =
            Recap { from: "Codex", said: &said, ledger: Some(ledger), room: Some(room(200_000)), ..Recap::default() };
        let failed =
            Failed { agent: "Codex".into(), reason: Reason::UsageLimit, done: so_far(&[], &[], Some("Tests next")) };
        let told = format!("{}\n\n{}", recap.text().unwrap(), takeover(&[failed]).expect("a takeover"));
        assert!(told.len() > 20_000, "{}", told.len());
        let line = excerpt(&told, 8_000);
        assert!(line.len() <= 8_000 + "\n…\n".len(), "{}", line.len());
        assert!(line.starts_with("<handoff>\nThis chat has been going on with Codex"), "{line}");
        assert!(line.contains(ledger) && line.ends_with("don't repeat what the user was already told.]"), "{line}");
        assert_eq!(excerpt("short", 8_000), "short");
    }

    #[test]
    fn reset_times_read_as_words() {
        assert_eq!(in_words(20_000), "in under a minute");
        assert_eq!(in_words(12 * 60_000), "in 12 min");
        assert_eq!(in_words(2 * 3_600_000), "in 2 h");
        assert_eq!(in_words(2 * 3_600_000 + 5 * 60_000), "in 2 h 5 min");
    }
}
