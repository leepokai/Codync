//! The handover ledger: an index of what the agents before left behind, read by rule from the chat's tool calls (no
//! model call): the files they changed, the commands they ran, the dev servers still running. Paths and command
//! lines only, never contents: the next agent opens a file or the history for those. What the user asked for isn't
//! here: standing rules reach every session through the bot's memory, and the handoff repeats the user's own
//! messages. Design: `docs/features/agent-fallback.md`.

use crate::agent::acp::truncate;
use crate::store::Entry;

/// Files, commands and dev servers a ledger lists at most; the rest is counted.
const FILES: usize = 10;
const COMMANDS: usize = 6;
const SERVERS: usize = 6;
/// Longest path or command line listed, in bytes.
const LINE: usize = 160;
/// Tool calls a ledger reads at most, the newest.
pub const READS: i64 = 200;

/// What the agents before left for the next one.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Files their finished edits changed, newest first.
    pub files: Vec<Changed>,
    /// Commands they ran, failed ones first, then newest first.
    pub commands: Vec<Command>,
    /// Dev servers the host keeps running now.
    pub running: Vec<String>,
}

/// A file an edit changed.
#[derive(Debug, PartialEq, Eq)]
pub struct Changed {
    pub path: String,
    pub change: Change,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Created,
    Modified,
}

/// A command a tool call ran, and how the call ended.
#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub line: String,
    pub ended: Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    Completed,
    Failed,
    /// Still pending or running when its agent stopped.
    Unfinished,
}

impl Ledger {
    /// The ledger of `tools` (the tool calls of a chat's work, newest first: `Store::tools_after`) and `servers`, the
    /// bot's dev servers that outlive its agent, by name and ports (`DevServers::active`).
    pub fn of(tools: &[Entry], servers: &[(String, Vec<u16>)]) -> Self {
        Self { files: files(tools), commands: commands(tools), running: servers.iter().map(running).collect() }
    }

    /// The ledger as the next agent reads it; `None` when the agents before left nothing.
    pub fn render(&self) -> Option<String> {
        let sections = [
            section("Files changed, newest first:", self.files.iter().map(Changed::line), FILES),
            section("Commands run, failed ones first:", self.commands.iter().map(Command::line), COMMANDS),
            section("Running now:", self.running.iter().map(|server| format!("- {server}")), SERVERS),
        ];
        let sections: Vec<String> = sections.into_iter().flatten().collect();
        (!sections.is_empty()).then(|| {
            format!(
                "<ledger>\nWhat the agents before you left behind, read from this chat's tool calls. It's an index, not the contents: read a file before you change it, and check a result before you rely on it.\n{}\n</ledger>",
                sections.join("\n")
            )
        })
    }
}

impl Changed {
    fn line(&self) -> String {
        let change = match self.change {
            Change::Created => "created",
            Change::Modified => "modified",
        };
        format!("- {} ({change})", truncate(&self.path, LINE))
    }
}

impl Command {
    fn line(&self) -> String {
        let line = truncate(&self.line, LINE);
        match self.ended {
            Ended::Completed => format!("- {line}"),
            Ended::Failed => format!("- {line} (failed)"),
            Ended::Unfinished => format!("- {line} (didn't finish)"),
        }
    }
}

/// `heading` over the first `most` of `lines`, the rest counted; `None` when there are none.
fn section(heading: &str, lines: impl ExactSizeIterator<Item = String>, most: usize) -> Option<String> {
    let more = lines.len().saturating_sub(most);
    let mut out: Vec<String> = lines.take(most).collect();
    if out.is_empty() {
        return None;
    }
    out.insert(0, heading.to_owned());
    if more > 0 {
        out.push(format!("- and {more} more"));
    }
    Some(out.join("\n"))
}

/// The files `tools`' finished edits changed, each once. An edit that failed or never ran changed nothing.
fn files(tools: &[Entry]) -> Vec<Changed> {
    let mut out: Vec<Changed> = Vec::new();
    let diffs = tools
        .iter()
        .filter(|e| e.data["status"] == "completed")
        .flat_map(|e| e.data["diffs"].as_array().into_iter().flatten());
    for diff in diffs {
        let Some(path) = diff["path"].as_str().filter(|p| !p.is_empty()) else { continue };
        let change = if diff["isNew"] == true { Change::Created } else { Change::Modified };
        match out.iter_mut().find(|seen| seen.path == path) {
            // An earlier call made the file: it's new to the next agent, whatever changed it since.
            Some(seen) if change == Change::Created => seen.change = change,
            Some(_) => {}
            None => out.push(Changed { path: path.to_owned(), change }),
        }
    }
    out
}

/// The commands `tools` ran, each line once as its newest call ended. A command of several lines (a script, a
/// heredoc) is listed on one.
fn commands(tools: &[Entry]) -> Vec<Command> {
    let mut ran: Vec<Command> = Vec::new();
    for e in tools.iter().filter(|e| e.data["toolKind"] == "execute") {
        let line = e.data["title"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() || ran.iter().any(|c| c.line == line) {
            continue;
        }
        let ended = match e.data["status"].as_str() {
            Some("completed") => Ended::Completed,
            Some("failed") => Ended::Failed,
            _ => Ended::Unfinished,
        };
        ran.push(Command { line, ended });
    }
    let (failed, rest): (Vec<_>, Vec<_>) = ran.into_iter().partition(|c| c.ended == Ended::Failed);
    failed.into_iter().chain(rest).collect()
}

/// A running dev server, with the ports it listens on.
fn running((name, ports): &(String, Vec<u16>)) -> String {
    let name = truncate(name, LINE);
    let ports: Vec<String> = ports.iter().map(u16::to_string).collect();
    match ports.as_slice() {
        [] => format!("dev server {name}"),
        [port] => format!("dev server {name} on port {port}"),
        ports => format!("dev server {name} on ports {}", ports.join(", ")),
    }
}

#[cfg(test)]
mod tests;
