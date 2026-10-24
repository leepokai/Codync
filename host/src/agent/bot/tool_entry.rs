//! Tool-call entries: ACP content folded into output and diffs, the working line a call shows,
//! and which calls are the bot's own bookkeeping.

use crate::agent::acp;
use crate::chat::memory::files;
use crate::store::BotConfig;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Folds ACP tool-call `content` into `{output, diffs}` on an entry's data.
pub(super) fn merge_tool_content(data: &mut Value, content: &Value) {
    let Some(items) = content.as_array() else { return };
    let mut out = String::new();
    let mut diffs = vec![];
    for item in items {
        match item["type"].as_str() {
            Some("diff") => {
                let path = item["path"].as_str().unwrap_or_default();
                let old = item["oldText"].as_str().unwrap_or_default();
                let new = item["newText"].as_str().unwrap_or_default();
                diffs.push(diff_summary(path, old, new));
            }
            Some("terminal") => {}
            _ => {
                let t = acp::content_text(item);
                if !t.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&t);
                }
            }
        }
    }
    if !out.is_empty() {
        data["output"] = acp::truncate(&out, 6000).into();
    }
    if !diffs.is_empty() {
        data["diffs"] = diffs.into();
    }
}

/// Changed-region summary: trims the common prefix/suffix and reports the middle.
fn diff_summary(path: &str, old: &str, new: &str) -> Value {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    let removed = &a[prefix..a.len() - suffix];
    let added = &b[prefix..b.len() - suffix];
    let mut unified = String::new();
    for (sign, lines) in [('-', removed), ('+', added)] {
        for l in lines {
            unified.push(sign);
            unified.push_str(l);
            unified.push('\n');
        }
    }
    json!({
        "path": path,
        "added": added.len(),
        "removed": removed.len(),
        "isNew": old.is_empty(),
        "startLine": prefix + 1,
        "patch": acp::truncate(&unified, 6000),
    })
}

/// Composing the bot's message: the `chat` MCP server's `send_message`, by the name the agent titles the call with.
fn sends_message(title: &str) -> bool {
    ["mcp__chat__send_message", "chat.send_message"].iter().any(|name| title.starts_with(name))
}

/// Keeping the bot's memory: a call of the `memory` MCP server, or one that names a file in a memory folder the
/// bot sees (in its `locations` or diffs, as `chat::memory::turns::touched` reads them).
fn keeps_memory(cfg: &BotConfig, data: &Value) -> bool {
    let title = data["title"].as_str().unwrap_or_default();
    if ["mcp__memory__", "memory."].iter().any(|server| title.starts_with(server)) {
        return true;
    }
    let folders: Vec<PathBuf> = files::layers(cfg).iter().map(|layer| files::folder(layer.as_deref())).collect();
    data["locations"]
        .as_array()
        .into_iter()
        .chain(data["diffs"].as_array())
        .flatten()
        .filter_map(|item| item["path"].as_str())
        .any(|path| folders.iter().any(|dir| Path::new(path).starts_with(dir)))
}

/// Flags whether a call is the bot's own bookkeeping (composing its message, keeping its memory), which the
/// working steps a chat unfolds leave out. An update can change the title or paths, so the flag follows them.
/// Always written, so clients tell an older host's unflagged call from one that isn't bookkeeping.
pub(super) fn mark_bookkeeping(cfg: &BotConfig, data: &mut Value) {
    let flag = sends_message(data["title"].as_str().unwrap_or_default()) || keeps_memory(cfg, data);
    data["bookkeeping"] = flag.into();
}

/// The working line for a flagged tool call: composing a message reads as typing, and memory stays out of the
/// chat, its working line included (the memory settings show it).
pub(super) fn activity(data: &Value) -> &str {
    let title = data["title"].as_str().unwrap_or("Tool");
    if sends_message(title) {
        "Typing…"
    } else if data["bookkeeping"] == true {
        "Thinking…"
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(mut data: Value) -> Value {
        let cfg = serde_json::from_value(json!({"id": "b", "name": "Bot", "backend": "claude", "cwd": "/tmp"}))
            .expect("a bot config");
        mark_bookkeeping(&cfg, &mut data);
        data
    }

    #[test]
    fn the_bots_own_calls_are_bookkeeping() {
        let note = files::folder(Some("b")).join("trip.md").display().to_string();
        for data in [
            json!({"title": "mcp__chat__send_message"}),
            json!({"title": "chat.send_message"}),
            json!({"title": "mcp__memory__edit_memory"}),
            json!({"title": "memory.search_memory"}),
            json!({"title": "Write", "locations": [{"path": note}]}),
            json!({"title": "Edit", "diffs": [{"path": note}]}),
        ] {
            assert_eq!(marked(data.clone())["bookkeeping"], true, "{data}");
        }
    }

    #[test]
    fn work_that_only_names_memory_or_messages_is_not_bookkeeping() {
        for data in [
            json!({"title": "Read /Users/me/app/src/memory.rs", "locations": [{"path": "/Users/me/app/src/memory.rs"}]}),
            json!({"title": "rg -n send_message src/"}),
            json!({"title": "Edit docs/design/bot-memory.md", "diffs": [{"path": "/repo/docs/design/bot-memory.md"}]}),
            json!({"title": "Terminal"}),
        ] {
            assert_eq!(marked(data.clone())["bookkeeping"], false, "{data}");
        }
    }

    #[test]
    fn bookkeeping_follows_the_title() {
        let mut data = marked(json!({"title": "Tool"}));
        assert_eq!(data["bookkeeping"], false);
        data["title"] = "mcp__memory__edit_memory".into();
        assert_eq!(marked(data)["bookkeeping"], true);
    }

    #[test]
    fn memory_work_reads_as_thinking() {
        assert_eq!(activity(&marked(json!({"title": "mcp__chat__send_message"}))), "Typing…");
        assert_eq!(activity(&marked(json!({"title": "mcp__memory__edit_memory"}))), "Thinking…");
        assert_eq!(activity(&marked(json!({"title": "Read src/memory.rs"}))), "Read src/memory.rs");
        assert_eq!(activity(&marked(json!({"title": "Terminal"}))), "Terminal");
    }

    #[test]
    fn diff_counts_changed_middle() {
        let d = diff_summary("f", "a\nb\nc\n", "a\nB\nB2\nc\n");
        assert_eq!(d["added"], 2);
        assert_eq!(d["removed"], 1);
        assert_eq!(d["startLine"], 2);
        assert_eq!(d["patch"], "-b\n+B\n+B2\n");
    }

    #[test]
    fn tool_content_merges_text_and_diffs() {
        let mut data = json!({"output": "", "diffs": []});
        merge_tool_content(
            &mut data,
            &json!([
                {"type": "content", "content": {"type": "text", "text": "ok"}},
                {"type": "diff", "path": "/a.rs", "oldText": "", "newText": "x\n"}
            ]),
        );
        assert_eq!(data["output"], "ok");
        assert_eq!(data["diffs"][0]["isNew"], true);
    }
}
