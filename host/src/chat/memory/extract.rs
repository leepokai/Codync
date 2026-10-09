//! What the keeper is asked and what it answers: extraction, consolidation and episode
//! prompts (as in Grok Bot), parsing its reply and applying it to the memory folder.

use super::facts::{Fact, Kind, Memory, dedupe_key, normalize, ymd};
use super::{NONE, NOTE_PREFIX, PROFILE_PROMPT_LIMIT, PROFILE_TARGET, RECENT_PROMPT_LIMIT, RELEVANT_LIMIT};
use crate::store::Store;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const TRIVIAL: &[&str] = &[
    "hi",
    "hey",
    "hello",
    "yo",
    "sup",
    "thanks",
    "thank you",
    "ty",
    "thx",
    "ok",
    "okay",
    "k",
    "kk",
    "cool",
    "nice",
    "great",
    "awesome",
    "perfect",
    "yes",
    "yep",
    "yeah",
    "no",
    "nope",
    "sure",
    "got it",
    "gotcha",
    "lol",
    "haha",
    "np",
    "done",
    "good",
    "bye",
];

/// Small talk ("thanks", "ok") isn't worth a keeper run.
pub fn is_memorable(user: &str) -> bool {
    let user = user.trim();
    if user.is_empty() {
        return false;
    }
    if user.chars().count() > 40 || user.contains('?') {
        return true;
    }
    let bare = user.to_lowercase();
    let bare = bare.trim_end_matches(|c: char| c.is_whitespace() || "!.…,~)]".contains(c));
    !TRIVIAL.contains(&bare.split_whitespace().collect::<Vec<_>>().join(" ").as_str())
}

pub fn extraction_system_prompt() -> String {
    [
        "You maintain the long-term memory of a personal assistant. Read the latest exchanges and decide what — if anything — is worth remembering for future, unrelated conversations.",
        "",
        "Tag each fact you keep with a category:",
        "- \"profile\": enduring facts about who the user is and how to work with them — their name and how to address them, role, location, languages, lasting preferences and constraints, and important people or relationships. These are remembered indefinitely.",
        "- \"log\": substantive history worth keeping — ongoing projects and tasks, decisions, commitments, and time-bound details.",
        "- \"note\": minor, low-stakes details that might help someday but are not worth keeping in mind every turn (small one-off preferences, incidental context). Notes fade from the always-visible list fastest but stay on disk.",
        "",
        "Do NOT record one-off request mechanics, what the assistant did this turn, general knowledge, or anything already present in the existing memory list.",
        "",
        "If a new exchange updates or contradicts a fact in the existing memory list (e.g. the user moved, changed jobs, or renamed something), drop anything clearly superseded: output a line \"remove: <the exact existing fact text>\" and then add the corrected fact. Only remove facts that appear verbatim in the existing list — never invent removals.",
        "",
        "Write each fact as a self-contained statement, one per line: \"profile: <fact>\", \"log: <fact>\", or \"note: <fact>\" to add (e.g. \"profile: The user's name is Ian\", \"log: Planning a trip to Tokyo in October 2025\"), or \"remove: <existing fact>\" to drop a superseded one.",
        "Output exactly NONE (and nothing else) when there is nothing to add or remove.",
    ]
    .join("\n")
}

pub fn extraction_user_prompt(turns: &[EpisodeTurn], existing: &[String]) -> String {
    let existing = if existing.is_empty() {
        "(empty)".to_owned()
    } else {
        existing.iter().map(|m| format!("- {m}")).collect::<Vec<_>>().join("\n")
    };
    let side = |s: &str| if s.trim().is_empty() { "(no message)".to_owned() } else { s.trim().to_owned() };
    let body = turns
        .iter()
        .map(|t| format!("({})\nUser: {}\nAssistant: {}", ymd(t.ts), side(&t.user), side(&t.agent)))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Existing memory:\n{existing}\n\nLatest exchanges, oldest first:\n\n{body}")
}

pub fn consolidation_system_prompt() -> String {
    [
        format!("You maintain the long-term memory of a personal assistant. Its profile of the user (enduring facts about who they are and how to work with them) has grown past {PROFILE_PROMPT_LIMIT} facts and must be merged down to at most {PROFILE_TARGET}."),
        "Merge duplicates and near-duplicates into one self-contained statement, drop facts clearly superseded by a newer one (each fact shows the date it was learned), and move anything that is history rather than who the user is (projects, events, one-off details) to the log.".to_owned(),
        "Keep every distinct enduring fact and keep the user's wording where you can. Never invent facts.".to_owned(),
        "Output one fact per line: \"profile: <fact>\" for the new profile, \"log: <fact>\" for what moves to the log. Output nothing else.".to_owned(),
    ]
    .join("\n")
}

pub fn consolidation_user_prompt(facts: &[Fact]) -> String {
    let mut facts: Vec<&Fact> = facts.iter().collect();
    facts.sort_by_key(|f| f.created_at);
    let body = facts.iter().map(|f| format!("- ({}) {}", ymd(f.created_at), f.content)).collect::<Vec<_>>().join("\n");
    format!("Profile, oldest first:\n{body}")
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Extraction {
    pub additions: Vec<(String, Kind)>,
    pub removals: Vec<String>,
}

pub fn parse_extraction(raw: &str, existing: &[String]) -> Extraction {
    let mut out = Extraction::default();
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case(NONE) {
        return out;
    }
    let mut seen: HashSet<String> = existing.iter().map(|m| dedupe_key(m)).collect();
    for line in raw.lines() {
        let line = strip_bullet(line);
        let (tag, rest) = match line.split_once(':') {
            Some((t, r)) if ["profile", "log", "note", "remove"].contains(&t.trim().to_lowercase().as_str()) => {
                (Some(t.trim().to_lowercase()), r)
            }
            _ => (None, line),
        };
        let bare = normalize(rest);
        if bare.is_empty() || bare.eq_ignore_ascii_case(NONE) {
            continue;
        }
        if tag.as_deref() == Some("remove") {
            out.removals.push(bare);
            continue;
        }
        let content = if tag.as_deref() == Some("note") { normalize(&format!("{NOTE_PREFIX}{bare}")) } else { bare };
        if seen.insert(dedupe_key(&content)) {
            out.additions.push((content, if tag.as_deref() == Some("profile") { Kind::Profile } else { Kind::Log }));
        }
    }
    out
}

fn strip_bullet(line: &str) -> &str {
    let t = line.trim_start();
    for b in ["- ", "* ", "• "] {
        if let Some(r) = t.strip_prefix(b) {
            return r;
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(r) = t[digits..].strip_prefix(". ").or_else(|| t[digits..].strip_prefix(") "))
    {
        return r;
    }
    t
}

/// What the keeper is shown as "existing memory": everything in the prompt plus
/// archived facts that share words with the exchange.
pub fn existing_for_extraction(mem: &Memory, exchange: &str) -> Result<Vec<String>> {
    let recall = mem.recall(RECENT_PROMPT_LIMIT)?;
    let in_prompt: Vec<&Fact> = recall.profile.iter().chain(&recall.recent).collect();
    let shown: HashSet<String> = in_prompt.iter().map(|f| dedupe_key(&f.content)).collect();
    let relevant = mem.relevant(exchange, RELEVANT_LIMIT)?;
    Ok(in_prompt
        .into_iter()
        .map(|f| f.content.clone())
        .chain(relevant.into_iter().filter(|f| !shown.contains(&dedupe_key(&f.content))).map(|f| f.content))
        .collect())
}

pub fn apply(mem: &Memory, ex: &Extraction, known: &[String], now: i64) -> Result<(usize, usize)> {
    let known: HashSet<String> = known.iter().map(|k| dedupe_key(k)).collect();
    let mut removed = 0;
    for r in &ex.removals {
        if known.contains(&dedupe_key(r)) && mem.remove_by_content(r)? {
            removed += 1;
        }
    }
    let mut added = 0;
    for (content, kind) in &ex.additions {
        if mem.add(content, *kind, now)? {
            added += 1;
        }
    }
    Ok((added, removed))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EpisodeTurn {
    pub ts: i64,
    pub user: String,
    pub agent: String,
    #[serde(default)]
    pub session: String,
}

pub fn episode_system_prompt(bot_name: &str) -> String {
    [
        format!("You maintain the long-term memory of a personal assistant named {bot_name}."),
        format!("You are given the most recent turns of a conversation between the user and {bot_name}, in order, each tagged with its date."),
        format!("Write a concise session summary of goals, decisions, outcomes and unfinished next steps capturing what the user and {bot_name} were actually working on across these turns — the throughline, key decisions, and outcomes — so it stays useful months from now."),
        "Anchor any time references with the absolute dates shown, never relative words like \"yesterday\". Drop greetings, acknowledgements, and anything ephemeral. Never invent details.".to_owned(),
        "Output only the summary, no preamble. Output exactly NONE if nothing in this stretch is worth remembering.".to_owned(),
    ]
    .join("\n")
}

pub fn episode_user_prompt(bot_name: &str, turns: &[EpisodeTurn]) -> String {
    let body = turns
        .iter()
        .map(|t| {
            let mut parts = vec![format!("({})", ymd(t.ts))];
            if !t.user.trim().is_empty() {
                parts.push(format!("User: {}", t.user.trim()));
            }
            if !t.agent.trim().is_empty() {
                parts.push(format!("{bot_name}: {}", t.agent.trim()));
            }
            parts.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Recent turns, oldest first:\n\n{body}")
}

fn episode_key(bot_id: &str) -> String {
    format!("memory.episode.{bot_id}")
}

pub fn pending_episode(store: &Store, bot_id: &str) -> Vec<EpisodeTurn> {
    store.kv_get(&episode_key(bot_id)).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

pub fn set_pending_episode(store: &Store, bot_id: &str, turns: &[EpisodeTurn]) -> Result<()> {
    store.kv_set(&episode_key(bot_id), &serde_json::to_string(turns)?)
}

#[cfg(test)]
mod tests {
    use super::super::dates::parse_ymd;
    use super::*;

    #[test]
    fn extraction_output_is_parsed() {
        let existing = vec!["Lives in Taipei".to_owned()];
        let ex = parse_extraction(
            "- profile: The user's name is Ian\n2. log: Planning a trip\nnote: likes tea\nremove: Lives in Taipei\nprofile: lives in taipei\nNONE",
            &existing,
        );
        assert_eq!(
            ex.additions,
            vec![
                ("The user's name is Ian".to_owned(), Kind::Profile),
                ("Planning a trip".to_owned(), Kind::Log),
                ("[note] likes tea".to_owned(), Kind::Log),
            ]
        );
        assert_eq!(ex.removals, vec!["Lives in Taipei".to_owned()]);
        assert_eq!(parse_extraction(" none ", &[]), Extraction::default());
    }

    #[test]
    fn batched_exchanges_are_dated_in_order() {
        let t = parse_ymd("2026-09-30").expect("valid date");
        let turns = [
            EpisodeTurn { ts: t, user: "I use Swift 6".into(), agent: "Noted".into(), session: String::new() },
            EpisodeTurn { ts: t, user: String::new(), agent: "Done".into(), session: String::new() },
        ];
        let prompt = extraction_user_prompt(&turns, &[]);
        assert!(prompt.contains("Existing memory:\n(empty)"));
        assert!(
            prompt.contains("(2026-09-30)\nUser: I use Swift 6\nAssistant: Noted\n\n(2026-09-30)\nUser: (no message)")
        );
    }

    #[test]
    fn small_talk_is_not_memorable() {
        assert!(!is_memorable("thanks!"));
        assert!(!is_memorable("  OK. "));
        assert!(is_memorable("my name is Kai"));
        assert!(is_memorable("why?"));
    }
}
