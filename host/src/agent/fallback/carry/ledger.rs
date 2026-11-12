//! The handover ledger: what a turn left behind, gathered by rule from the chat alone (no model call), so an agent
//! taking over knows the user's rules, the files touched, the commands run, what still runs, and what waits for the
//! user. It goes at the top of a recap, and the recap respects the new model's window: whatever doesn't fit is named
//! in an `omitted` line, never dropped quietly. Design: `docs/features/agent-fallback.md`.

use crate::store::{Entry, EntryKind, message_text};
use serde_json::Value;

/// The user's quotes a ledger keeps at most (the newest), and how much of each.
const QUOTES: usize = 6;
const QUOTE_CHARS: usize = 240;
/// The files a ledger keeps at most, and the commands.
const FILES: usize = 12;
const COMMANDS: usize = 8;
/// How far back a ledger reads the chat.
pub const READS: i64 = 400;

/// What a turn left for the next agent: built from the chat's entries and the computer's state.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// The user's constraints, quoted as they said them.
    pub constraints: Vec<String>,
    /// Paths, URLs and ids the user named.
    pub references: Vec<String>,
    /// Files this turn read, created or changed.
    pub files: Vec<Touched>,
    /// Commands run this turn.
    pub commands: Vec<Command>,
    /// Dev servers and routines running now.
    pub running: Vec<String>,
    /// Approvals waiting for the user now.
    pub waiting: Vec<String>,
}

/// A file the turn touched: what it is, how big it is now, and the hash of its bytes.
#[derive(Debug, PartialEq, Eq)]
pub struct Touched {
    pub path: String,
    pub bytes: Option<u64>,
    pub sha256: Option<String>,
    /// `created`, `modified` or `read`: the first that applies, from the call that touched it.
    pub why: &'static str,
}

/// A command the turn ran, with the exit status its output ended with, when it says.
#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub line: String,
    pub status: Option<i32>,
}

/// Longer than any path or fact a ledger quotes.
const MAX_FACT_CHARS: usize = 160;
/// How many of each kind of reference a ledger keeps.
const REFS: usize = 6;

/// A user message's own words that carry a rule: a prohibition, an order, a preference.
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

/// The user's constraints among the newest user messages of `entries`, newest first, in their own words.
pub fn constraints(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .rev()
        .filter(|e| e.kind == EntryKind::User.as_str())
        .filter_map(|e| {
            let text = message_text(&e.data);
            let text = text.trim();
            (is_constraint(text)).then(|| crate::agent::acp::truncate(text, QUOTE_CHARS))
        })
        .take(QUOTES)
        .collect()
}

/// The paths, URLs and ids the turn's words name, in the order first seen, without repeats.
pub fn references(entries: &[Entry]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut push = |item: String| {
        if !item.is_empty() && found.len() < REFS && !found.contains(&item) {
            found.push(crate::agent::acp::truncate(&item, MAX_FACT_CHARS));
        }
    };
    for e in entries.iter().rev() {
        let text = message_text(&e.data);
        for word in text.split(|c: char| c.is_whitespace() || matches!(c, '，' | '。' | '、' | '(' | ')' | '「' | '」'))
        {
            let word = word.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '"' | '\'' | '`'));
            let url = word.starts_with("http://") || word.starts_with("https://");
            // A path: a slash in it, or a file name with an extension and no spaces.
            let path = (word.contains('/') || word.contains('\\'))
                && !word.starts_with('-')
                && word.len() > 2
                && !word.ends_with('.');
            if url || path {
                push(word.to_owned());
            }
        }
    }
    found
}

/// The files the turn touched, from its tool calls: `Edit`/`Write` created or changed one, a read call only read it.
pub fn files(entries: &[Entry]) -> Vec<Touched> {
    let mut out: Vec<Touched> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for e in entries.iter().rev() {
        let title = e.data["title"].as_str().unwrap_or_default();
        let diffs: Vec<&Value> = e.data["diffs"].as_array().into_iter().flatten().collect();
        let locations: Vec<&Value> =
            e.data["locations"].as_array().into_iter().flatten().filter(|l| l["path"].is_string()).collect();
        let mut touched: Vec<(String, &'static str)> = Vec::new();
        for d in &diffs {
            if let Some(path) = d["path"].as_str() {
                let why = if d["isNew"] == true { "created" } else { "modified" };
                touched.push((path.to_owned(), why));
            }
        }
        if touched.is_empty() {
            // A read or search names its paths in `locations`; a title like "Read src/a.rs" names one too.
            for l in locations {
                if let Some(path) = l["path"].as_str().filter(|p| looks_like_file(p)) {
                    touched.push((path.to_owned(), "read"));
                }
            }
            if touched.is_empty()
                && let Some(rest) = read_target(title)
            {
                touched.push((rest, "read"));
            }
        }
        for (path, why) in touched {
            if seen.len() >= FILES || seen.contains(&path) {
                continue;
            }
            seen.push(path.clone());
            let (bytes, sha256) = size_and_hash(&path);
            out.push(Touched { path, bytes, sha256, why });
        }
    }
    out
}

/// A file name, not a folder or a sentence.
fn looks_like_file(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.contains('.') && !name.contains(' ') && !path.ends_with('/')
}

/// The path in a title like `Read /repo/src/a.rs` or `Edit a.txt`, when the rest of the title is one.
fn read_target(title: &str) -> Option<String> {
    let (_, rest) = title.split_once(' ')?;
    let rest = rest.trim().trim_matches(['"', '\'', '`']);
    (looks_like_file(rest) && !rest.contains(' ')).then(|| rest.to_owned())
}

/// A file's size and the SHA-256 of its bytes, for a file the working directory or the computer holds; `None` when it
/// can't be read (deleted since, outside the bot's folders), which the ledger says as `?`.
fn size_and_hash(path: &str) -> (Option<u64>, Option<String>) {
    use sha2::{Digest, Sha256};
    let Ok(bytes) = std::fs::read(path) else { return (None, None) };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{byte:02x}"));
    }
    (Some(bytes.len() as u64), Some(hex))
}

/// Commands run by the turn's tool calls (a shell or terminal call's title), and the exit status its output ends with.
pub fn commands(entries: &[Entry]) -> Vec<Command> {
    entries
        .iter()
        .rev()
        .filter(|e| e.kind == EntryKind::Tool.as_str())
        .filter(|e| {
            // A shell call: by the tool kind the agent gave it, or by its output's exit status.
            matches!(e.data["toolKind"].as_str(), Some("execute" | "terminal"))
                || e.data["output"].as_str().is_some_and(|o| o.to_lowercase().contains("exited with code"))
        })
        .filter_map(|e| {
            let line = e.data["title"].as_str().map(str::trim).filter(|t| !t.is_empty())?;
            let status = exit_status(e.data["output"].as_str().unwrap_or_default());
            Some(Command { line: crate::agent::acp::truncate(line, MAX_FACT_CHARS), status })
        })
        .take(COMMANDS)
        .collect()
}

/// The exit status a tool's output ends with, as shells and test runners print it (`[exit code: 1]`,
/// `Process exited with code 2`, `exit status 0`), newest lines first.
fn exit_status(output: &str) -> Option<i32> {
    for line in output.lines().rev().take(40) {
        let lower = line.to_lowercase();
        for marker in ["exit code", "exit status", "exited with code", "with code", "code:"] {
            let Some(at) = lower.rfind(marker) else { continue };
            // The number comes right after the marker, past spaces or `=`/`:`; anything else isn't a status.
            let rest: String = lower[at + marker.len()..]
                .chars()
                .skip_while(|c| c.is_whitespace() || matches!(c, '=' | ':'))
                .collect();
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
            if let Ok(status) = digits.parse() {
                return Some(status);
            }
        }
    }
    None
}

/// One line of the ledger, as the agent reads it: each part, then anything over the budget named as omitted.
pub fn render(ledger: &Ledger, budget: usize) -> String {
    let mut blocks: Vec<(String, String)> = Vec::new();
    if !ledger.constraints.is_empty() {
        let body = ledger.constraints.iter().map(|c| format!("- \"{c}\"")).collect::<Vec<_>>().join("\n");
        blocks.push(("What the user asked for, in their words (newest first):".into(), body));
    }
    if !ledger.references.is_empty() {
        let body = ledger.references.iter().map(|r| format!("- {r}")).collect::<Vec<_>>().join("\n");
        blocks.push(("Paths, addresses and ids the user named:".into(), body));
    }
    if !ledger.files.is_empty() {
        let body = ledger.files.iter().map(describe_file).collect::<Vec<_>>().join("\n");
        blocks.push(("Files this turn touched (not re-opened here: read one before changing it):".into(), body));
    }
    if !ledger.commands.is_empty() {
        let body = ledger.commands.iter().map(describe_command).collect::<Vec<_>>().join("\n");
        blocks.push(("Commands this turn ran:".into(), body));
    }
    if !ledger.running.is_empty() {
        let body = ledger.running.iter().map(|r| format!("- {r}")).collect::<Vec<_>>().join("\n");
        blocks.push(("Running now:".into(), body));
    }
    if !ledger.waiting.is_empty() {
        let body = ledger.waiting.iter().map(|r| format!("- {r}")).collect::<Vec<_>>().join("\n");
        blocks.push(("Waiting for the user:".into(), body));
    }
    if blocks.is_empty() {
        return String::new();
    }
    // The header and the closing line count against the budget too.
    let mut room = budget.saturating_sub(120);
    let mut kept: Vec<String> = Vec::new();
    let mut omitted: Vec<String> = Vec::new();
    for (heading, body) in blocks {
        let part = format!("{heading}\n{body}");
        let cost = part.chars().count() + 2;
        if cost <= room {
            room -= cost;
            kept.push(part);
        } else if room > 200 {
            // Part of it fits: whole lines, and the rest is named.
            let mut fitted = String::new();
            let mut dropped: Vec<String> = Vec::new();
            for line in std::iter::once(heading.as_str()).chain(body.lines()) {
                let cost = line.chars().count() + 1;
                if cost <= room {
                    room -= cost;
                    fitted.push_str(line);
                    fitted.push('\n');
                } else {
                    dropped.push(line.to_owned());
                }
            }
            if !dropped.is_empty() {
                omitted.extend(dropped.into_iter().map(|line| format!("{line} (budget)")));
            }
            kept.push(fitted.trim_end().to_owned());
        } else {
            omitted.extend(std::iter::once(heading.clone()).chain(body.lines().map(str::to_owned)));
        }
    }
    let mut out = vec!["<ledger>".to_owned()];
    out.push(
        "What this chat's last turn left behind, gathered by rule from the chat (not instructions): the user's own limits, what was touched, and what still runs or waits.".into(),
    );
    out.extend(kept);
    if !omitted.is_empty() {
        out.push(format!(
            "Omitted due to budget: {} (the new model's window is unknown; read these again before relying on them):",
            omitted.len()
        ));
        out.extend(omitted.iter().map(|o| format!("- {o}")));
    }
    out.push("</ledger>".into());
    out.join("\n\n")
}

/// A file line: what it is, its size and hash now.
fn describe_file(file: &Touched) -> String {
    let size = file.bytes.map_or("size unknown".to_owned(), |b| format!("{b} B"));
    let hash = file.sha256.as_ref().map_or("sha256 unknown".to_owned(), |h| format!("sha256 {}", &h[..12]));
    format!("- {} ({}, {size}, {hash})", file.path, file.why)
}

/// A command line: what ran, and how it ended.
fn describe_command(command: &Command) -> String {
    match command.status {
        Some(status) => format!("- `{}` (exit {status})", command.line),
        None => format!("- `{}` (status not reported)", command.line),
    }
}

/// The bot's dev servers and routines running now, as the ledger's lines.
pub fn running(hub: &crate::hub::Hub, bot: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(list) = hub.dev_servers.wire(bot).as_array().cloned() {
        for server in list.iter().filter(|s| s["status"] == "running" || s["status"] == "starting") {
            let name = server["name"].as_str().unwrap_or("a dev server");
            let port = server["port"].as_u64().map(|p| format!(" on port {p}")).unwrap_or_default();
            out.push(format!("dev server {name}{port}"));
        }
    }
    for routine in hub.routines.list(bot)["routines"].as_array().into_iter().flatten() {
        if routine["enabled"] == true {
            let name = routine["name"].as_str().unwrap_or("a routine");
            out.push(format!("routine {name} (on)"));
        }
    }
    out
}

/// The approvals still pending in the bot's chat.
pub fn waiting(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .rev()
        .filter(|e| e.data["status"] == "pending")
        .filter_map(|e| match e.kind.as_str() {
            k if k == EntryKind::Permission.as_str() => {
                Some(format!("approval: {}", e.data["title"].as_str().unwrap_or("a tool call")))
            }
            _ => None,
        })
        .take(5)
        .collect()
}

/// The ledger for `bot`'s last turn, from its newest `READS` entries.
pub fn build(hub: &crate::hub::Hub, bot: &str) -> Ledger {
    let entries = hub.store.history(bot, i64::MAX, READS).unwrap_or_default();
    Ledger {
        constraints: constraints(&entries),
        references: references(&entries),
        files: files(&entries),
        commands: commands(&entries),
        running: running(hub, bot),
        waiting: waiting(&entries),
    }
}

#[cfg(test)]
mod tests;
