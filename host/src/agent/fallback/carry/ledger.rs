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

/// Preserve the verbatim prefix and disclose clipping.
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
            out.push(clipped(text, QUOTE_CHARS));
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
        for word in text.split(|c: char| c.is_whitespace() || matches!(c, '，' | '。' | '、' | '(' | ')' | '「' | '」'))
        {
            let word = word.trim_matches([',', '.', ';', ':', '"', '\'', char::from(96)]);
            let path = (word.contains('/') || word.contains('\\')) && !word.starts_with('-') && word.len() > 2;
            let id = uuid::Uuid::parse_str(word).is_ok()
                || word.strip_prefix('#').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            if !(path || id) || !seen.insert(word.to_owned()) {
                continue;
            }
            if out.len() < REFS {
                out.push(clipped(word, FACT_CHARS));
            } else {
                omitted.push(format!("reference at {} ({} B; collection limit)", e.seq, word.len()));
            }
        }
    }
    out
}

fn is_command_tool(entry: &Entry) -> bool {
    matches!(entry.data["toolKind"].as_str(), Some("execute" | "terminal"))
        || entry.data["command"].as_str().is_some_and(|cmd| !cmd.trim().is_empty())
}

fn commands(entries: &[Entry], omitted: &mut Vec<String>) -> Vec<Command> {
    let mut out = Vec::new();
    for e in entries.iter().rev().filter(|e| e.kind == EntryKind::Tool.as_str()) {
        if !is_command_tool(e) {
            continue;
        }
        let output = e.data["output"].as_str().unwrap_or_default();
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
    running_servers(servers, &mut out, omitted);
    running_routines(routines, &mut out, omitted);
    out
}

fn running_servers(servers: &Value, out: &mut Vec<String>, omitted: &mut Vec<String>) {
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
        collect_line(out, line, RUNNING, "dev server", omitted);
    }
}

/// The routines a bot is in the middle of, from `Routines::active_runs`: a run still owed its turn can be older than
/// every run in `Routines::list`'s newest 200, so it would be invisible to a handoff that read only that list.
fn running_routines(routines: &Value, out: &mut Vec<String>, omitted: &mut Vec<String>) {
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
        collect_line(out, line, RUNNING, "routine", omitted);
    }
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

fn is_unstarted_user(entry: &Entry) -> bool {
    entry.kind == EntryKind::User.as_str() && matches!(entry.data["status"].as_str(), Some("queued" | "cancelled"))
}

/// The new message is already in the prompt; current tools win, otherwise use the last started turn.
fn scoped(mut entries: Vec<Entry>, fresh: i64, current: &[String]) -> (Vec<Entry>, Vec<Entry>) {
    entries.retain(|e| e.seq > fresh);
    let turn = entries
        .iter()
        .rev()
        .find(|e| {
            !current.contains(&e.id)
                && !is_unstarted_user(e)
                && matches!(e.kind.as_str(), "user" | "agent" | "tool" | "permission")
        })
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

fn memory_roots(cfg: &crate::store::BotConfig) -> Vec<std::path::PathBuf> {
    crate::chat::memory::files::layers(&cfg.as_owner())
        .iter()
        .map(|layer| crate::chat::memory::files::folder(layer.as_deref()))
        .collect()
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
    let roots = memory_roots(cfg);
    ledger.constraints = constraints(&entries, &mut ledger.omitted);
    ledger.references = references(&entries, &mut ledger.omitted);
    ledger.files = files::collect(&work, Path::new(&cfg.cwd), &roots, &mut ledger.omitted);
    ledger.commands = commands(&work, &mut ledger.omitted);
    ledger.running = running(&hub.dev_servers.wire(&cfg.id), &hub.routines.active_runs(&cfg.id), &mut ledger.omitted);
    ledger.waiting = waiting(&entries, &mut ledger.omitted);
    Ok(ledger)
}

#[cfg(test)]
mod tests;
