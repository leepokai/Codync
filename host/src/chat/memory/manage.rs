//! Memory management shared by the phone, desktop and terminal clients.

use super::{Memory, maintenance};
use crate::hub::Hub;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::sync::Arc;

pub async fn call(hub: &Arc<Hub>, method: &str, args: &Value) -> Result<Value> {
    let bot = args["botId"].as_str().context("botId is required")?.to_owned();
    let row = hub.store.bot(&bot)?.filter(|b| !b.deleted && !b.config.is_group()).context("unknown bot")?;
    let clears = method == "clearMemory";
    let corrects = matches!(method, "saveMemory" | "forgetMemory");
    // Not corrections: the keeper's work stands, only what the instructions show changes.
    let reorders = matches!(method, "pinMemory" | "importMemory");
    let mutation = maintenance::lock(&bot);
    let _guard = mutation.lock().await;
    if clears {
        // Invalidate before deletion too: partial native failures must not resurrect old queued facts.
        maintenance::changed(&hub.store, &bot)?;
        maintenance::discard_pending(&hub.store, &bot)?;
    }
    if method == "exportMemory" {
        let text = serde_json::to_string_pretty(&super::engram::export(&bot).await?)?;
        if text.len() <= 384 * 1024 {
            return Ok(json!({"json":text}));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let root = crate::chat::uploads::root(&row.config);
        let upload = id.clone();
        tokio::task::spawn_blocking(move || {
            crate::chat::uploads::append(&root, &upload, "engram-backup.json", 0, text.as_bytes(), true)
        })
        .await??;
        return Ok(json!({"uploadId":id}));
    }
    if method == "importMemory" {
        let data = if let Some(upload) = args["uploadId"].as_str() {
            let root = crate::chat::uploads::root(&row.config);
            let upload = upload.to_owned();
            tokio::task::spawn_blocking(move || -> Result<Value> {
                let (path, _) = crate::chat::uploads::resolve(&root, &upload)?;
                ensure!(std::fs::metadata(&path)?.len() <= 100 * 1024 * 1024, "memory backup exceeds 100 MB");
                Ok(serde_json::from_slice(&std::fs::read(path)?)?)
            })
            .await??
        } else {
            match args["json"].as_str() {
                Some(text) => serde_json::from_str(text).context("invalid Engram JSON backup")?,
                None => args["data"].clone(),
            }
        };
        let result = super::engram::import(&bot, &data).await?;
        crate::chat::context::invalidate(&hub.store, &bot)?;
        return Ok(result);
    }
    let (method, args, id) = (method.to_owned(), args.clone(), bot.clone());
    let result = tokio::task::spawn_blocking(move || {
        let memory = Memory::for_bot(&id)?;
        dispatch(&memory, &method, &args)
    })
    .await??;
    if clears || corrects {
        maintenance::changed(&hub.store, &bot)?;
        maintenance::discard_pending(&hub.store, &bot)?;
    } else if reorders {
        crate::chat::context::invalidate(&hub.store, &bot)?;
    }
    Ok(result)
}

fn dispatch(memory: &Memory, method: &str, args: &Value) -> Result<Value> {
    match method {
        "memory" => listing(memory, args),
        "saveMemory" => save(memory, args),
        "memoryDetail" => {
            let fact = memory.find(required(args, "id")?)?;
            let id = fact.id.parse::<i64>()?;
            let mut history_args = json!({"id":id, "include_history":true, "project":fact.project});
            if let Some(cursor) = args.get("historyCursor") {
                history_args["history_cursor"] = cursor.clone();
            }
            let history = memory.tool("mem_get_observation", &history_args)?;
            let timeline = memory.tool("mem_timeline", &json!({"observation_id":id, "project":fact.project}))?;
            Ok(json!({"fact":fact, "history":payload(&history), "timeline":payload(&timeline)}))
        }
        "pinMemory" => {
            let fact = memory.find(required(args, "id")?)?;
            memory.tool(
                if args["pinned"] == true { "mem_pin" } else { "mem_unpin" },
                &json!({"id":fact.id.parse::<i64>()?}),
            )?;
            Ok(json!({}))
        }
        "reviewMemory" => {
            let fact = memory.find(required(args, "id")?)?;
            memory.tool(
                "mem_review",
                &json!({"action":"mark_reviewed", "observation_id":fact.id.parse::<i64>()?, "project":fact.project}),
            )?;
            Ok(json!({}))
        }
        "forgetMemory" => Ok(json!({"removed":memory.remove(required(args, "id")?)?})),
        "clearMemory" => {
            memory.clear()?;
            Ok(json!({}))
        }
        _ => bail!("unknown memory operation"),
    }
}

fn listing(memory: &Memory, args: &Value) -> Result<Value> {
    let query = args["query"].as_str().unwrap_or_default().trim();
    let offset = usize::try_from(args["offset"].as_u64().unwrap_or(0)).context("offset is too large")?;
    // Old clients only send botId and have no Load more control.
    let legacy = ["query", "filter", "offset", "limit"].iter().all(|key| args.get(key).is_none());
    let limit = if legacy { usize::MAX } else { usize::try_from(args["limit"].as_u64().unwrap_or(50).clamp(1, 100))? };
    let filter = args["filter"].as_str().unwrap_or("all");
    let page = memory.browse(query, filter, limit, offset)?;
    let next = (offset + page.facts.len() < page.total).then_some(offset + page.facts.len());
    Ok(json!({"location":memory.location(), "engine":"engram", "version":super::engram::VERSION,
        "facts":page.facts, "total":page.total, "nextOffset":next,
        "query":query, "filter":filter, "offset":offset}))
}

fn save(memory: &Memory, args: &Value) -> Result<Value> {
    let title = required(args, "title")?.trim();
    let content = required(args, "content")?.trim();
    ensure!(!title.is_empty() && !content.is_empty(), "title and content cannot be empty");
    let scope = args["scope"].as_str().unwrap_or("project");
    ensure!(["project", "personal", "global"].contains(&scope), "invalid memory scope");
    let mut fields = json!({"title":title, "content":content, "scope":scope,
        "type":args["memoryType"].as_str().unwrap_or("learning")});
    if let Some(topic) = args["topicKey"].as_str() {
        fields["topic_key"] = topic.into();
    }
    let name = if let Some(id) = args["id"].as_str() {
        let existing = memory.find(id)?;
        fields["id"] = id.parse::<i64>()?.into();
        fields["expected_project"] = existing.project.into();
        "mem_update"
    } else {
        fields["project"] = memory.project().into();
        fields["capture_prompt"] = false.into();
        fields["session_id"] = format!("{}:codync-background", memory.project()).into();
        "mem_save"
    };
    Ok(payload(&memory.tool(name, &fields)?))
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key].as_str().with_context(|| format!("{key} is required"))
}

pub(super) fn payload(result: &Value) -> Value {
    let text = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&text).unwrap_or_else(|_| json!({"result":text}))
}
