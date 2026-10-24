//! The built-in `memory` MCP server, the same for every harness: the bot's memory files ([`super::file_tools`]),
//! search over them, and search over its chat history.

use super::dates::ymd;
use crate::agent::acp;
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};
use codync_mem::files::{self as mf, MAIN};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

const SEARCH_DEFAULT: i64 = 10;
const SEARCH_MAX: i64 = 30;
const SEARCH_TERMS: usize = 8;
const SEARCH_TEXT_CHARS: usize = 1_500;

pub const INSTRUCTIONS: &str = "Your memory holds what you know about the user, and every session opens with it. \
It is Markdown files you keep yourself: MEMORY.md, one short fact about the user per line, in every session in \
full; and notes, one topic each, for detail too long for MEMORY.md (lists, itineraries, histories), listed in your \
instructions. When the user tells you something lasting about themselves, or asks you to remember, change or \
forget something, change the files with edit_memory (or your own file tools, where you were given the memory \
folders). The user reads memory in the app but can't edit it: they ask you. Project and code knowledge worth \
keeping goes in a note on that project, not MEMORY.md; don't make the project's CLAUDE.md or AGENTS.md your memory \
unless the user asks. Only in a turn the user started in your own chat can memory lose or change a line; at other \
times you can only add. search_memory finds lines of your memory files; search_history finds what was actually \
said in your past chats. When the user asks you to forget something, take it out of MEMORY.md and your notes, and \
don't bring it back from your chat history either.";

pub fn tools() -> Value {
    let mut tools = json!([
        {
            "name": "search_memory",
            "description": "Search your memory files for lines containing the query (case-insensitive). A MEMORY.md line lists the message it was learned from as source: pass it to search_history as from to read it.",
            "inputSchema": {
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"],
            },
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "search_history",
            "description": "Read your full chat history with the user, newest first: messages containing every given word (case-insensitive), or without a query, every message. Returns dated messages with ids, and next: pass it as from for the page before.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Words that must all appear, separated by spaces."},
                    "from": {"type": "string", "description": "A message id (from a result, next, or a memory's source): start at that message and go back."},
                    "limit": {"type": "integer", "description": "Maximum results (default 10, at most 30)."},
                },
            },
            "annotations": {"readOnlyHint": true},
        },
    ]);
    if let Some(list) = tools.as_array_mut() {
        list.extend(super::file_tools::definitions());
    }
    tools
}

pub async fn call(hub: &Arc<Hub>, bot_id: &str, name: &str, args: &Value) -> Result<Value> {
    let cfg = hub.store.bot(bot_id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    match name {
        "search_memory" => search_memory(hub, &cfg, args["query"].as_str().unwrap_or_default()).await,
        "search_history" => search_history(hub, &cfg, args).await,
        "read_memory" | "edit_memory" => super::file_tools::call(hub, &cfg, name, args).await,
        _ => bail!("unknown memory tool: {name}"),
    }
}

async fn search_memory(hub: &Arc<Hub>, cfg: &BotConfig, query: &str) -> Result<Value> {
    if query.trim().is_empty() {
        bail!("query required");
    }
    let (hub, cfg, query) = (hub.clone(), cfg.clone(), query.to_owned());
    let results = tokio::task::spawn_blocking(move || -> Result<Vec<Value>> {
        super::files::scan(&hub, &cfg);
        let shared = super::shared(&cfg);
        let (hits, mut sources) = hub.store.memory_read(|c| {
            let hits = mf::search(c, &cfg.id, shared, &query, 20)?;
            // Where each MEMORY.md fact was learned, by layer and line.
            let mut sources = HashMap::new();
            for layer in super::files::layers(&cfg) {
                for fact in mf::facts_of(c, layer.as_deref())? {
                    if let Some(message) = fact.message {
                        sources.insert((layer.clone(), mf::fact_id(&fact.text)), message);
                    }
                }
            }
            Ok((hits, sources))
        })?;
        // A shared fact may come from another bot's chat, which search_history can't open.
        sources.retain(|(layer, _), message| {
            layer.is_some() || hub.store.entry(message).is_some_and(|e| e.bot_id == cfg.id)
        });
        Ok(hits
            .into_iter()
            .map(|hit| {
                let source =
                    (hit.name == MAIN).then(|| sources.get(&(hit.bot.clone(), mf::fact_id(&hit.text)))).flatten();
                json!({
                    "file": hit.name,
                    "shared": hit.bot.is_none(),
                    "line": hit.line,
                    "text": acp::truncate(&hit.text, 300),
                    "source": source,
                })
            })
            .collect())
    })
    .await??;
    Ok(json!({"results": results}))
}

async fn search_history(hub: &Arc<Hub>, cfg: &BotConfig, args: &Value) -> Result<Value> {
    let terms: Vec<String> =
        args["query"].as_str().unwrap_or_default().split_whitespace().take(SEARCH_TERMS).map(str::to_owned).collect();
    let from = args["from"].as_str().map(str::to_owned);
    let limit = args["limit"].as_i64().unwrap_or(SEARCH_DEFAULT).clamp(1, SEARCH_MAX);
    let hub = hub.clone();
    let bot = cfg.id.clone();
    let (rows, next) = tokio::task::spawn_blocking(move || -> Result<_> {
        let from = match from {
            Some(id) => Some(
                hub.store
                    .entry(&id)
                    .filter(|e| e.bot_id == bot)
                    .ok_or_else(|| anyhow!("no message {id} in your chat"))?
                    .seq,
            ),
            None => None,
        };
        // One more than asked tells whether there is a page before.
        let mut rows = hub.store.search_messages(&bot, &terms, from, limit + 1)?;
        let next = if rows.len() > usize::try_from(limit)? { rows.pop().map(|e| e.id) } else { None };
        Ok((rows, next))
    })
    .await??;
    let results: Vec<Value> = rows
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "date": ymd(e.created_at),
                "from": if e.kind == crate::store::EntryKind::User.as_str() { "user" } else { "you" },
                "text": acp::truncate(e.data["text"].as_str().unwrap_or_default(), SEARCH_TEXT_CHARS),
            })
        })
        .collect();
    Ok(json!({"results": results, "next": next}))
}
