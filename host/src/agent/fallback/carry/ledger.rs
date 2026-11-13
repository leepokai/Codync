//! Mechanical handoff: the current conversation and the context manifest, gathered by rule (no model call).
//! Extends the handover-ledger design in leepokai/Codync PR #417. Codync 2.11.0 and earlier were MIT licensed;
//! the current project is Apache-2.0 (see LICENSE and NOTICE).
//! The Context Manifest concept was informed by rlaope/oh-my-hermes, Copyright (c) 2026
//! oh-my-hermes contributors, under the MIT License.
mod files;
mod render;
use crate::store::{Entry, EntryKind, message_text};
pub use render::render;
use serde_json::Value;
use std::path::Path;

const QUOTES: usize = 6;
#[cfg(test)]
const QUOTE_CHARS: usize = 240;
/// Hard collection limit. Items past it are named individually with their path, size and reason.
const FILES: usize = 12;
const COMMANDS: usize = 8;
const REFS: usize = 6;
const WAITING: usize = 5;
const RUNNING: usize = 12;
const FACT_CHARS: usize = 160;
pub const READS: i64 = 400;
/// The actor waits this long for the blocking inspection; a timeout is disclosed as an uncollected item.
const COLLECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub constraints: Vec<String>,
    pub references: Vec<String>,
    pub files: Vec<Touched>,
    pub commands: Vec<Command>,
    pub running: Vec<String>,
    pub waiting: Vec<String>,
    /// Short labels only; never an omitted quote's original content.
    pub omitted: Vec<String>,
}

impl Ledger {
    /// How many rows the manifest would expand, for a bounded handoff's omission notice.
    pub fn rows_len(&self) -> usize {
        self.constraints.len()
            + self.references.len()
            + self.running.len()
            + self.waiting.len()
            + self.files.len()
            + self.commands.len()
    }

    /// Short, itemized labels for a handoff that cannot expand the manifest itself. No original file contents.
    pub fn labels(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, value) in self.constraints.iter().enumerate() {
            out.push(format!("user constraint #{} ({} B): no original quote", i + 1, value.len()));
        }
        for (i, value) in self.references.iter().enumerate() {
            out.push(format!("user reference #{} ({} B): {value}", i + 1, value.len()));
        }
        for (i, value) in self.running.iter().enumerate() {
            out.push(format!("running item #{} ({} B): {value}", i + 1, value.len()));
        }
        for (i, value) in self.waiting.iter().enumerate() {
            out.push(format!("pending approval #{} ({} B): {value}", i + 1, value.len()));
        }
        for file in &self.files {
            out.push(render::file_label(file));
        }
        for (i, command) in self.commands.iter().enumerate() {
            out.push(format!("command #{} ({} B): {}", i + 1, command.line.len(), command.line));
        }
        out
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Touched {
    pub path: String,
    pub bytes: Option<u64>,
    pub sha256: Option<String>,
    pub why: &'static str,
    pub unavailable: Option<&'static str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub line: String,
    pub status: Option<i32>,
    pub state: String,
}

fn is_constraint(text: &str) -> bool {
    const MARKERS: [&str; 24] = [
        "不要",
        "别",
        "必须",
        "只能",
        "务必",
        "记得",
        "千万别",
        "不准",
        "禁止",
        "一律",
        "保持",
        "以后",
        "每次",
        "先",
        "don't",
        "do not",
        "never",
        "must",
        "always",
        "only",
        "make sure",
        "keep ",
        "remember",
        "instead",
    ];
    let lower = text.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Keep display names bounded while preserving user quotes and references verbatim elsewhere.
fn clipped(text: &str, cap: usize) -> String {
    if text.chars().count() <= cap {
        text.to_owned()
    } else {
        format!("{} [truncated; {} original bytes]", text.chars().take(cap).collect::<String>(), text.len())
    }
}

fn constraints(entries: &[Entry], omitted: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries.iter().rev().filter(|e| e.kind == EntryKind::User.as_str()) {
        let text = message_text(&e.data);
        let text = text.trim();
        if !is_constraint(text) {
            continue;
        }
        if out.len() < QUOTES {
            out.push(text.to_owned());
        } else {
            omitted.push(format!("rule at {} ({} B; collection limit)", e.seq, text.len()));
        }
    }
    out
}

fn references(entries: &[Entry], omitted: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in entries.iter().rev().filter(|e| e.kind == EntryKind::User.as_str()) {
        let text = message_text(&e.data);
        let plain = quoted_references(&text, e.seq, &mut seen, &mut out, omitted);
        let words: Vec<_> = plain
            .split(|c: char| c.is_whitespace() || matches!(c, '，' | '。' | '、' | '(' | ')' | '「' | '」'))
            .collect();
        for (index, word) in words.iter().enumerate() {
            push_reference(word, e.seq, &mut seen, &mut out, omitted);
            let word = clean_reference(word);
            let previous =
                index.checked_sub(1).and_then(|i| words.get(i)).map(|word| clean_reference(word).to_lowercase());
            if word.bytes().all(|b| b.is_ascii_digit())
                && previous.as_deref().is_some_and(|word| matches!(word, "issue" | "pr" | "pull"))
            {
                push_reference(
                    &format!("{} {word}", previous.unwrap_or_default()),
                    e.seq,
                    &mut seen,
                    &mut out,
                    omitted,
                );
            }
        }
    }
    out
}

fn quoted_references(
    text: &str,
    seq: i64,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
    omitted: &mut Vec<String>,
) -> String {
    let mut chars: Vec<_> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let Some(close) = matching_quote(chars[index]) else {
            index += 1;
            continue;
        };
        let Some(offset) = chars[index + 1..].iter().position(|c| *c == close) else {
            index += 1;
            continue;
        };
        let end = index + 1 + offset;
        let value: String = chars[index + 1..end].iter().collect();
        push_reference(&value, seq, seen, out, omitted);
        chars[index..=end].fill(' ');
        index = end + 1;
    }
    chars.into_iter().collect()
}

fn matching_quote(value: char) -> Option<char> {
    match value {
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        '「' => Some('」'),
        _ => None,
    }
}

fn push_reference(
    value: &str,
    seq: i64,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
    omitted: &mut Vec<String>,
) {
    let value = clean_reference(value);
    if !looks_like_reference(value) || !seen.insert(value.to_owned()) {
        return;
    }
    if out.len() < REFS {
        out.push(value.to_owned());
    } else {
        omitted.push(format!("reference at {seq} ({} B; collection limit)", value.len()));
    }
}

fn clean_reference(value: &str) -> &str {
    value.trim_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '，' | '。' | '；' | '：' | '！' | '？'
            )
    })
}

fn looks_like_reference(value: &str) -> bool {
    if value.len() < 3 || value.starts_with('-') {
        return false;
    }
    let lower = value.to_lowercase();
    let path = (value.contains('/') || value.contains('\\')) && value != "/";
    let url = lower.starts_with("http://") || lower.starts_with("https://");
    let uuid = uuid::Uuid::parse_str(value).is_ok();
    let issue =
        value.strip_prefix('#').is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
    let issue_phrase = lower.split_once(' ').is_some_and(|(kind, number)| {
        matches!(kind, "issue" | "pr" | "pull") && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
    });
    let file = value.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && (1..=16).contains(&extension.len())
            && extension.chars().all(|c| c.is_ascii_alphanumeric())
            && !stem.ends_with('/')
            && !stem.ends_with('\\')
    });
    let code = value.contains('-')
        && value.chars().any(|c| c.is_ascii_alphabetic())
        && value.chars().any(|c| c.is_ascii_digit())
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    path || url || uuid || issue || issue_phrase || file || code
}

fn commands(entries: &[Entry], omitted: &mut Vec<String>) -> Vec<Command> {
    let mut out = Vec::new();
    for e in entries.iter().rev().filter(|e| e.kind == EntryKind::Tool.as_str()) {
        let output = e.data["output"].as_str().unwrap_or_default();
        if !matches!(e.data["toolKind"].as_str(), Some("execute" | "terminal")) && exit_status(output).is_none() {
            continue;
        }
        let Some(line) = e.data["title"].as_str().map(str::trim).filter(|s| !s.is_empty()) else { continue };
        if out.len() < COMMANDS {
            out.push(Command {
                line: clipped(line, FACT_CHARS),
                status: exit_status(output),
                state: e.data["status"].as_str().unwrap_or("unreported").to_owned(),
            });
        } else {
            omitted.push(format!("command at {} ({} B; collection limit)", e.seq, line.len()));
        }
    }
    out
}

fn exit_status(output: &str) -> Option<i32> {
    for line in output.lines().rev().take(40) {
        let lower = line.to_lowercase();
        for marker in ["exit code", "exit status", "exited with code"] {
            let Some(at) = lower.rfind(marker) else { continue };
            let rest =
                lower[at + marker.len()..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '=' | ':'));
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
            if let Ok(status) = digits.parse() {
                return Some(status);
            }
        }
    }
    None
}

fn running(servers: &Value, routines: &Value, omitted: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for server in servers
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| matches!(s["status"].as_str(), Some("running" | "starting")))
    {
        let ports = server["ports"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["port"].as_u64())
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let line = format!(
            "dev server {} (ports: {})",
            clipped(server["name"].as_str().unwrap_or("unnamed"), FACT_CHARS),
            if ports.is_empty() { "unknown" } else { &ports }
        );
        collect_line(&mut out, line, RUNNING, "dev server", omitted);
    }
    let definitions = routines["routines"].as_array().into_iter().flatten().collect::<Vec<_>>();
    let mut seen = std::collections::HashSet::new();
    for run in routines["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|run| matches!(run["status"].as_str(), Some("pending" | "starting" | "running" | "recovering")))
    {
        let Some(id) = run["routineId"].as_str() else { continue };
        if !seen.insert(id) {
            continue;
        }
        let name = definitions
            .iter()
            .find(|routine| routine["id"] == id)
            .and_then(|routine| routine["name"].as_str())
            .unwrap_or(id);
        let status = run["status"].as_str().unwrap_or("active");
        let line = format!("routine {} ({status})", clipped(name, FACT_CHARS));
        collect_line(&mut out, line, RUNNING, "routine", omitted);
    }
    out
}

fn collect_line(out: &mut Vec<String>, line: String, cap: usize, kind: &str, omitted: &mut Vec<String>) {
    if out.len() < cap {
        out.push(line);
    } else {
        omitted.push(format!("{kind} ({} B; collection limit)", line.len()));
    }
}

fn waiting(entries: &[Entry], omitted: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries.iter().rev().filter(|e| e.kind == EntryKind::Permission.as_str() && e.data["status"] == "pending")
    {
        let line = format!(
            "approval at {}: {}",
            e.seq,
            clipped(e.data["title"].as_str().unwrap_or("a tool call"), FACT_CHARS)
        );
        collect_line(&mut out, line, WAITING, "pending approval", omitted);
    }
    out
}

/// The new message is already in the prompt; current tools win, otherwise use the last answered turn.
fn scoped(mut entries: Vec<Entry>, fresh: i64, current: &[String]) -> (Vec<Entry>, Vec<Entry>) {
    entries.retain(|e| e.seq > fresh);
    let turn = entries
        .iter()
        .rev()
        .find(|e| !current.contains(&e.id) && matches!(e.kind.as_str(), "user" | "agent" | "tool" | "permission"))
        .map(|e| e.turn);
    let work = entries.iter().filter(|e| Some(e.turn) == turn).cloned().collect();
    (entries, work)
}

pub async fn build(
    hub: std::sync::Arc<crate::hub::Hub>,
    cfg: crate::store::BotConfig,
    fresh: i64,
    current: Vec<String>,
) -> Ledger {
    let bot = cfg.id.clone();
    let events = hub.clone();
    let worker = tokio::task::spawn_blocking(move || build_blocking(&hub, &cfg, fresh, &current));
    match tokio::time::timeout(COLLECTION_TIMEOUT, worker).await {
        Ok(Ok(Ok(ledger))) => {
            report(
                &events,
                "collected",
                ledger.omitted.len(),
                ledger.files.iter().filter(|f| f.unavailable.is_some()).count(),
            );
            ledger
        }
        outcome => {
            let category = match outcome {
                Err(_) => "time_limit",
                Ok(Err(_)) => "worker_failed",
                _ => "history_failed",
            };
            tracing::warn!(bot, category, "handoff ledger unavailable");
            report(&events, category, 0, 0);
            Ledger {
                omitted: vec![format!("ledger unavailable ({category}); read history before continuing")],
                ..Ledger::default()
            }
        }
    }
}

fn report(hub: &crate::hub::Hub, outcome: &str, omitted: usize, unavailable: usize) {
    let properties =
        serde_json::json!({"outcome": outcome, "collection_omitted": omitted, "files_unavailable": unavailable});
    crate::analytics::capture(
        hub,
        crate::analytics::Event::HandoffLedgerBuilt,
        properties.as_object().cloned().unwrap_or_default(),
    );
}

fn build_blocking(
    hub: &crate::hub::Hub,
    cfg: &crate::store::BotConfig,
    fresh: i64,
    current: &[String],
) -> anyhow::Result<Ledger> {
    let entries = hub.store.history(&cfg.id, i64::MAX, READS + 1)?;
    let (entries, _) = scoped(entries, fresh, current);
    let mut ledger = Ledger::default();
    let mut scoped_entries = entries;
    if scoped_entries.len() > usize::try_from(READS).unwrap_or_default() {
        scoped_entries.remove(0);
        ledger.omitted.push("older history not scanned (400-entry limit); search_history for earlier items".into());
    }
    let (entries, work) = scoped(scoped_entries, fresh, current);
    let roots = crate::chat::memory::files::layers(cfg)
        .iter()
        .map(|layer| crate::chat::memory::files::folder(layer.as_deref()))
        .collect::<Vec<_>>();
    ledger.constraints = constraints(&entries, &mut ledger.omitted);
    ledger.references = references(&entries, &mut ledger.omitted);
    ledger.files = files::collect(&work, Path::new(&cfg.cwd), &roots, &mut ledger.omitted);
    ledger.commands = commands(&work, &mut ledger.omitted);
    ledger.running = running(&hub.dev_servers.wire(&cfg.id), &hub.routines.list(&cfg.id), &mut ledger.omitted);
    ledger.waiting = waiting(&entries, &mut ledger.omitted);
    Ok(ledger)
}

#[cfg(test)]
mod tests;
