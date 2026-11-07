//! The bot's side, on the `chat` helper: a separate MCP server would add a process to every session.

use super::run::{create, launch, stop};
use super::{View, check_room, logs, report};
use crate::analytics::Event;
use crate::hub::Hub;
use crate::store::{BotConfig, Permission};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

pub const START: &str = "start_dev_server";
pub const TOOLS: [&str; 4] = [START, "list_dev_servers", "stop_dev_server", "dev_server_logs"];

/// How long a start waits for a port or an early exit before it answers.
const WAIT_FOR_PORT: Duration = Duration::from_secs(20);
const WAIT_POLL: Duration = Duration::from_millis(200);
/// Log lines a start's answer carries.
const ANSWER_LINES: usize = 20;

/// What a bot's instructions say about them.
pub const BRIEF: &str = "Long-running programs: start a dev server, a watch build or anything else that keeps running with start_dev_server, not in the background of your shell. It keeps running when your session or agent stops, and the user sees its port and can stop it. Short commands stay in your shell.";

pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": START,
            "description": "Start a program that keeps running (a dev server, a watch build), run by Codync so it outlives your session and the user sees its port. Give the command as you'd run it in the foreground: it ends when its first process ends, so don't add & or nohup. Starting the same command in the same folder again returns the one already running. Waits up to 20 s for a port and answers with the status, ports and the end of its log. Keep secrets in the project's env files, not in the command: the user's apps show it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The command line, e.g. npm run dev."},
                    "name": {"type": "string", "description": "A short name the user sees, e.g. web. Default: the folder's name."},
                    "folder": {"type": "string", "description": "Where to run it, absolute or relative to your working directory. Default: your working directory."},
                },
                "required": ["command"],
            },
            "annotations": {"readOnlyHint": false, "openWorldHint": true},
        }),
        json!({
            "name": "list_dev_servers",
            "description": "Your dev servers: id, name, command, status and ports. Includes what you left running through your shell, with managed false: that stops when your agent does.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        }),
        json!({
            "name": "stop_dev_server",
            "description": "Stop one of your dev servers and everything it started.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": true},
        }),
        json!({
            "name": "dev_server_logs",
            "description": "The end of a dev server's output, to see why it failed or what it's doing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "lines": {"type": "integer", "description": "How many lines. Default 200, at most 1000."},
                },
                "required": ["id"],
            },
            "annotations": {"readOnlyHint": true},
        }),
    ]
}

pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        START => start(hub, bot, args).await,
        "list_dev_servers" => Ok(json!({"servers": hub.dev_servers.wire(bot)})),
        "stop_dev_server" => {
            stop(hub, bot, arg(args, "id")?, "bot").await?;
            Ok(json!({"stopped": true}))
        }
        "dev_server_logs" => {
            let id = arg(args, "id")?;
            hub.dev_servers.view(bot, id)?;
            let lines = args["lines"].as_u64().map_or(logs::SHOWN_LINES, |n| usize::try_from(n).unwrap_or(usize::MAX));
            Ok(json!({"log": logs::tail(id, lines.min(logs::MOST_LINES)).await?}))
        }
        _ => bail!("unknown tool"),
    }
}

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key].as_str().map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| anyhow!("{key} is required"))
}

async fn start(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let command = arg(args, "command")?.to_owned();
    let cfg = hub.store.bot(bot)?.filter(|row| !row.deleted).map(|row| row.config).context("unknown bot")?;
    let folder = folder_for(&cfg, args["folder"].as_str()).await?;
    let on_a_card = cfg.permission != Permission::Auto;
    let servers = &hub.dev_servers;
    if let Some(id) = servers.same(bot, &folder, &command) {
        // A stopped one comes back, with no second card if someone allowed it; under Auto nobody did.
        if !servers.view(bot, &id)?.status.active() {
            if !servers.approved(&id) {
                check_room(hub, bot)?;
                ask(hub, &cfg, &command, &folder).await?;
                if on_a_card {
                    servers.note_approved(&hub.store, &id);
                }
            }
            launch(hub, &id).await?;
        }
        return answer(hub, bot, &id, true).await;
    }
    check_room(hub, bot)?;
    ask(hub, &cfg, &command, &folder).await?;
    let name = args["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| Path::new(&folder).file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "dev server".to_owned());
    let (id, reused) = create(hub, bot, name, command, folder, on_a_card).await?;
    if reused {
        // A parallel start added it while this one waited on its card, and it may have ended since.
        if on_a_card {
            servers.note_approved(&hub.store, &id);
        }
        launch(hub, &id).await?;
    }
    answer(hub, bot, &id, reused).await
}

/// Under Ask, waits on the approval card with the command and folder.
async fn ask(hub: &Hub, cfg: &BotConfig, command: &str, folder: &str) -> Result<()> {
    let detail = format!("{command}\n{folder}");
    let answered = crate::chat::manage::approve(hub, cfg, "Start a dev server".to_owned(), detail).await;
    if answered.is_err() {
        report(hub, Event::DevServerStarted, "outcome", "refused");
    }
    answered
}

/// A folder must exist: the error then says so, not a failed start. Written one way, so a folder written two ways
/// (`.`, a trailing separator) is one server.
async fn folder_for(cfg: &BotConfig, asked: Option<&str>) -> Result<String> {
    let asked = asked.map(str::trim).filter(|folder| !folder.is_empty());
    let path = asked.map_or_else(|| PathBuf::from(&cfg.cwd), |folder| Path::new(&cfg.cwd).join(folder));
    let found = tokio::fs::metadata(&path).await.is_ok_and(|meta| meta.is_dir());
    if !found {
        bail!("no folder at {}", path.display());
    }
    Ok(tidied(&path).to_string_lossy().into_owned())
}

pub(super) fn tidied(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Waits for a port or for it to end, then says how it went. A restart's answer comes the same way.
async fn answer(hub: &Arc<Hub>, bot: &str, id: &str, reused: bool) -> Result<Value> {
    let deadline = Instant::now() + WAIT_FOR_PORT;
    let waiting = |view: &View| view.status.active() && view.ports.is_empty();
    let mut view = hub.dev_servers.view(bot, id)?;
    while waiting(&view) && Instant::now() < deadline {
        tokio::time::sleep(WAIT_POLL).await;
        view = hub.dev_servers.view(bot, id)?;
    }
    let outcome = match (view.status.active(), view.ports.is_empty()) {
        (true, false) => "port",
        (true, true) => "no_port",
        (false, _) => "exited",
    };
    report(hub, Event::DevServerStarted, "outcome", outcome);
    let mut answer = json!(view);
    answer["reused"] = reused.into();
    // How the start went is settled by now: a log that can't be shown mustn't turn that into a failed start.
    let log = logs::tail(id, ANSWER_LINES).await;
    answer["log"] = log.unwrap_or_else(|error| format!("couldn't read its log: {error:#}")).into();
    Ok(answer)
}
