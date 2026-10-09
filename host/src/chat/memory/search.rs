//! History search, the built-in `memory` MCP server.

use super::facts::ymd;
use crate::agent::acp;
use crate::hub::Hub;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;

const SEARCH_DEFAULT: i64 = 10;
const SEARCH_MAX: i64 = 30;
const SEARCH_TERMS: usize = 8;
const SEARCH_TEXT_CHARS: usize = 1_500;

pub const INSTRUCTIONS: &str = "Use search_history to find what was actually said in your past chats with the user \
(their messages and yours, including threads), beyond what your memory notes kept. \
Search for distinctive words; every word must appear. Results are newest first.";

pub fn tools() -> Value {
    json!([{
        "name": "search_history",
        "description": "Search your full chat history with the user for messages containing every given word (case-insensitive). Returns dated messages, newest first.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Words that must all appear, separated by spaces."},
                "limit": {"type": "integer", "description": "Maximum results (default 10, at most 30)."},
            },
            "required": ["query"],
        },
        "annotations": {"readOnlyHint": true},
    }])
}

pub async fn call(hub: &Arc<Hub>, bot_id: &str, name: &str, args: &Value) -> Result<Value> {
    hub.store.bot(bot_id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    if name.starts_with("mem_") {
        let read_only = matches!(
            name,
            "mem_search"
                | "mem_context"
                | "mem_timeline"
                | "mem_get_observation"
                | "mem_stats"
                | "mem_current_project"
                | "mem_list_projects"
                | "mem_doctor"
                | "mem_suggest_topic_key"
        ) || (name == "mem_review" && args["action"] == "list");
        let mutation = super::maintenance::lock(bot_id);
        let _guard = if read_only { None } else { Some(mutation.lock().await) };
        let result = super::engram::call(bot_id, name, args).await?;
        if matches!(name, "mem_update" | "mem_delete" | "mem_merge_projects") && result["isError"] != true {
            super::maintenance::changed(&hub.store, bot_id)?;
            super::maintenance::discard_pending(&hub.store, bot_id)?;
        }
        return Ok(result);
    }
    if name != "search_history" {
        bail!("unknown memory tool: {name}");
    }
    let bot = hub.store.bot(bot_id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    let terms: Vec<String> =
        args["query"].as_str().unwrap_or_default().split_whitespace().take(SEARCH_TERMS).map(str::to_owned).collect();
    if terms.is_empty() {
        bail!("query must contain at least one word");
    }
    let limit = args["limit"].as_i64().unwrap_or(SEARCH_DEFAULT).clamp(1, SEARCH_MAX);
    let hub = hub.clone();
    let id = bot.config.id.clone();
    let rows = tokio::task::spawn_blocking(move || hub.store.search_messages(&id, &terms, limit)).await??;
    let results: Vec<Value> = rows
        .iter()
        .map(|e| {
            json!({
                "date": ymd(e.created_at),
                "from": if e.kind == crate::store::EntryKind::User.as_str() { "user" } else { "you" },
                "inThread": e.thread_id.is_some(),
                "text": acp::truncate(e.data["text"].as_str().unwrap_or_default(), SEARCH_TEXT_CHARS),
            })
        })
        .collect();
    Ok(json!({"results": results}))
}

pub async fn available_tools(hub: &Arc<Hub>, bot_id: &str) -> Result<Value> {
    hub.store.bot(bot_id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
    let mut native = super::engram::tools(bot_id).await?;
    let list = native["tools"].as_array_mut().ok_or_else(|| anyhow!("Engram returned no tools"))?;
    list.extend(tools().as_array().into_iter().flatten().cloned());
    Ok(native)
}
