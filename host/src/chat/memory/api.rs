//! The apps' memory methods: the two layers' `MEMORY.md` as facts, the notes, the dream log and Claude Code's own
//! notes, read-only. The user changes memory by telling the bot.

use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::{Result, anyhow, bail};
use codync_mem::files::{self as mf, Author, Fact, MAIN};
use serde_json::{Value, json};
use std::sync::Arc;

/// Dream runs the memory screen lists.
const DREAMS_SHOWN: usize = 20;
/// The most of a project's `MEMORY.md` the screen shows.
const PROJECT_MEMORY_BYTES: u64 = 64 * 1024;

pub async fn call(hub: &Arc<Hub>, method: &str, b: &Value) -> Result<Value> {
    let hub = hub.clone();
    let method = method.to_owned();
    let b = b.clone();
    tokio::task::spawn_blocking(move || dispatch(&hub, &method, &b)).await?
}

fn dispatch(hub: &Hub, method: &str, b: &Value) -> Result<Value> {
    match method {
        "memory" => view(hub, &bot(hub, str_arg(b, "botId")?)?),
        "memoryFile" => super::file_api::detail(hub, &bot(hub, str_arg(b, "botId")?)?, b),
        _ => bail!("unknown method: {method}"),
    }
}

fn view(hub: &Hub, cfg: &BotConfig) -> Result<Value> {
    super::files::scan(hub, cfg);
    let bot = cfg.id.as_str();
    let layer = |layer: Option<&str>| -> Result<Value> {
        let (facts, content) = hub
            .store
            .memory_read(|c| Ok((mf::facts_of(c, layer)?, mf::current(c, layer, MAIN)?.unwrap_or_default())))?;
        // The same fact twice is one memory to the apps, which key memories by id.
        let mut seen = std::collections::HashSet::new();
        let entries: Vec<Value> =
            facts.iter().filter(|fact| seen.insert(mf::fact_id(&fact.text))).map(|fact| entry(layer, fact)).collect();
        Ok(json!({"entries": entries, "used": codync_mem::tokens::estimate(&content), "cap": mf::cap(layer)}))
    };
    Ok(json!({
        "shared": super::shared(cfg),
        "yours": layer(None)?,
        "bot": layer(Some(bot))?,
        "files": super::file_api::listing(hub, cfg)?,
        "dreams": hub.store.memory_read(|c| codync_mem::dream::recent(c, bot, DREAMS_SHOWN))?,
        "projectMemory": project_memory(cfg),
        "memoryRev": hub.store.memory_rev(),
    }))
}

/// A fact as the apps show a memory: who wrote it and where it was learned. One the user changed in the file
/// themselves (outside Codync) is theirs.
fn entry(layer: Option<&str>, fact: &Fact) -> Value {
    let author = match fact.author {
        Author::User | Author::Computer => "user",
        Author::Bot => "bot",
        Author::Dream => "dream",
        Author::Imported => "imported",
    };
    json!({
        "id": mf::fact_id(&fact.text),
        "bot": layer,
        "state": "active",
        "text": fact.text,
        "author": author,
        "authorBot": fact.author_bot,
        "pinned": false,
        "evidence": fact.message.iter().collect::<Vec<_>>(),
        "createdAt": fact.at,
        "updatedAt": fact.at,
    })
}

/// Claude Code's own memory of the bot's folder (`~/.claude/projects/<folder>/memory/MEMORY.md`), where the user
/// kept it on, shown read-only so nothing a bot remembers is out of the user's sight. Empty text until Claude Code
/// saves something, so the apps show its section as soon as the switch is on.
fn project_memory(cfg: &BotConfig) -> Value {
    if cfg.backend != "claude" || !crate::agent::own_memory::kept(cfg) {
        return Value::Null;
    }
    let folder: String = cfg.cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let Some(path) = dirs::home_dir().map(|home| home.join(".claude/projects").join(folder).join("memory/MEMORY.md"))
    else {
        return Value::Null;
    };
    let read = std::fs::File::open(&path).and_then(|file| {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, PROJECT_MEMORY_BYTES), &mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    });
    match read {
        Ok(text) => json!({"path": path.to_string_lossy(), "text": text}),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({"path": path.to_string_lossy(), "text": ""}),
        Err(error) => {
            tracing::warn!(%error, "couldn't read Claude Code's memory");
            Value::Null
        }
    }
}

fn bot(hub: &Hub, id: &str) -> Result<BotConfig> {
    let cfg = hub.store.bot(id)?.filter(|b| !b.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    if cfg.is_group() {
        bail!("a group has no memory of its own");
    }
    Ok(cfg)
}

fn str_arg<'a>(b: &'a Value, k: &str) -> Result<&'a str> {
    b[k].as_str().ok_or_else(|| anyhow!("{k} required"))
}
