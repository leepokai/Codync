//! Built-in `chat` MCP tools for:
//! - Idea 8: Open item tracking (`track_item`, `update_item`, `resolve_item`, `list_open_items`)
//! - Idea 9: Routine deduplication (`mark_reported`, `was_reported`)
//! - Idea 11: Background shell jobs (`shell_job`)
//! - Idea 13: Interactive human handoff/takeover (`request_handoff`)

use crate::LockExt;
use crate::hub::Hub;
use crate::store::{EntryKind, Lane, now_ms};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt};

pub const TOOLS: [&str; 8] = [
    "track_item",
    "update_item",
    "resolve_item",
    "list_open_items",
    "mark_reported",
    "was_reported",
    "shell_job",
    "request_handoff",
];

struct ShellJobState {
    bot_id: String,
    command: String,
    started_at: i64,
    output: Arc<Mutex<String>>,
    exit_code: Arc<Mutex<Option<i32>>>,
    abort: tokio::task::AbortHandle,
}

static JOBS: LazyLock<Mutex<HashMap<String, ShellJobState>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn tools() -> [Value; 8] {
    [
        json!({
            "name": "track_item",
            "description": "Track an open task, PR, build, package or follow-up item across sessions so proactive check-ins and the user can see what is still waiting.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Short item title (deduplicated case-insensitively)."},
                    "note": {"type": "string", "description": "Current status or what to check next."},
                    "sourceUrl": {"type": "string"},
                    "checkAfterHours": {"type": "number", "description": "Hours before checking on this item again."}
                },
                "required": ["title"]
            }
        }),
        json!({
            "name": "update_item",
            "description": "Update the note or next check time for an open item by id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "note": {"type": "string"},
                    "checkAfterHours": {"type": "number"}
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "resolve_item",
            "description": "Mark a tracked open item as resolved or dismissed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "status": {"type": "string", "enum": ["resolved", "dismissed"]},
                    "note": {"type": "string"}
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "list_open_items",
            "description": "List all currently open tracked items for this bot.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "mark_reported",
            "description": "Record that a specific item key (e.g. email ID, PR number, URL) was already reported to the user so future routine runs don't repeat it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {"type": "string"},
                    "note": {"type": "string"}
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "was_reported",
            "description": "Check a batch of candidate keys against recently reported items; returns which keys were already reported and which are fresh.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "keys": {"type": "array", "items": {"type": "string"}},
                    "withinHours": {"type": "number"}
                },
                "required": ["keys"]
            },
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "shell_job",
            "description": "Start, poll, list or kill an asynchronous background shell command in your workspace without blocking the turn.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["start", "status", "list", "kill"]},
                    "command": {"type": "string", "description": "For action=start, the shell command to run."},
                    "jobId": {"type": "string", "description": "For action=status or kill, the job id."}
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "request_handoff",
            "description": "Pause and ask the user to take over a step that requires human intervention (CAPTCHA, 2FA, login, payment confirmation).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "reason": {"type": "string", "description": "Why human takeover is needed right now."},
                    "url": {"type": "string", "description": "Optional page URL where the user should act."}
                },
                "required": ["reason"]
            }
        }),
    ]
}

pub fn call(hub: &Arc<Hub>, bot_id: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "track_item" => {
            let title = args["title"].as_str().context("title is required")?;
            let note = args["note"].as_str().unwrap_or_default();
            let url = args["sourceUrl"].as_str();
            let hours = args["checkAfterHours"].as_f64();
            let item = hub.store.track_open_item(bot_id, title, note, url, hours)?;
            Ok(json!({"item": item}))
        }
        "update_item" => {
            let id = args["id"].as_str().context("id is required")?;
            let item =
                hub.store.update_open_item(bot_id, id, args["note"].as_str(), args["checkAfterHours"].as_f64())?;
            Ok(json!({"item": item}))
        }
        "resolve_item" => {
            let id = args["id"].as_str().context("id is required")?;
            let status = args["status"].as_str().unwrap_or("resolved");
            let closed = hub.store.close_open_item(bot_id, id, status, args["note"].as_str())?;
            Ok(json!({"closed": closed, "id": id, "status": status}))
        }
        "list_open_items" => {
            let items = hub.store.list_open_items(bot_id)?;
            Ok(json!({"items": items}))
        }
        "mark_reported" => {
            let key = args["key"].as_str().context("key is required")?;
            hub.store.mark_reported(bot_id, key, args["note"].as_str())?;
            Ok(json!({"ok": true, "key": key}))
        }
        "was_reported" => {
            let keys: Vec<String> = args["keys"]
                .as_array()
                .context("keys array is required")?
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            hub.store.was_reported(bot_id, &keys, args["withinHours"].as_f64())
        }
        "shell_job" => shell_job(hub, bot_id, args),
        "request_handoff" => {
            let reason = args["reason"].as_str().context("reason is required")?.trim();
            if reason.is_empty() {
                bail!("reason is required");
            }
            let url = args["url"].as_str().map(str::trim).filter(|s| !s.is_empty());
            let data = super::takeover::card(bot_id, reason, url, now_ms());
            let entry = hub
                .add_entry(&Lane::main(bot_id), EntryKind::Notice, hub.store.max_turn(bot_id), &data)
                .ok_or_else(|| anyhow!("couldn't post handoff card"))?;
            super::takeover::notify(hub, bot_id, reason);
            Ok(json!({
                "status": "waiting_for_user",
                "entryId": entry.id,
                "message": "The user has been notified to take over and will reply when finished."
            }))
        }
        _ => bail!("unknown tool: {name}"),
    }
}

fn shell_job(hub: &Hub, bot_id: &str, args: &Value) -> Result<Value> {
    let action = args["action"].as_str().unwrap_or("list");
    match action {
        "start" => {
            let command = args["command"].as_str().context("command is required")?.trim().to_owned();
            if command.is_empty() {
                bail!("command is required");
            }
            if let Some(pattern) = crate::agent::blocklist::blocked(&command) {
                bail!("That command is never allowed (\"{pattern}\")");
            }
            let row = hub.store.bot(bot_id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?;
            let job_id = format!("job_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
            let output = Arc::new(Mutex::new(String::new()));
            let exit_code = Arc::new(Mutex::new(None));
            let (out_clone, exit_clone, cwd, cmd_text) =
                (output.clone(), exit_code.clone(), row.config.cwd.clone(), command.clone());

            let handle = tokio::spawn(async move {
                let Ok((shell, _script)) = crate::shell::expanding(&cmd_text, &[]).await else {
                    *exit_clone.locked() = Some(-1);
                    return;
                };
                let mut cmd = shell.tokio();
                cmd.current_dir(&cwd);
                let (output, status) = match Box::pin(run_shell_command(cmd)).await {
                    Ok(result) => result,
                    Err(error) => (format!("Shell job failed: {error:#}"), -1),
                };
                *out_clone.locked() = output;
                *exit_clone.locked() = Some(status);
            });

            JOBS.locked().insert(
                job_id.clone(),
                ShellJobState {
                    bot_id: bot_id.to_owned(),
                    command,
                    started_at: now_ms(),
                    output,
                    exit_code,
                    abort: handle.abort_handle(),
                },
            );
            Ok(json!({"jobId": job_id, "status": "running"}))
        }
        "status" => {
            let job_id = args["jobId"].as_str().context("jobId is required")?;
            let jobs = JOBS.locked();
            let job = jobs.get(job_id).filter(|j| j.bot_id == bot_id).context("unknown jobId")?;
            let code = *job.exit_code.locked();
            let out = job.output.locked().clone();
            Ok(json!({
                "jobId": job_id,
                "command": job.command,
                "startedAt": job.started_at,
                "status": if code.is_some() { "completed" } else { "running" },
                "exitCode": code,
                "output": out,
            }))
        }
        "kill" => {
            let job_id = args["jobId"].as_str().context("jobId is required")?;
            let mut jobs = JOBS.locked();
            if let Some(job) = jobs.remove(job_id) {
                job.abort.abort();
                Ok(json!({"jobId": job_id, "killed": true}))
            } else {
                Ok(json!({"jobId": job_id, "killed": false}))
            }
        }
        _ => {
            let jobs = JOBS.locked();
            let list: Vec<Value> = jobs
                .iter()
                .filter(|(_, j)| j.bot_id == bot_id)
                .map(|(id, j)| {
                    let code = *j.exit_code.locked();
                    json!({
                        "jobId": id,
                        "command": j.command,
                        "startedAt": j.started_at,
                        "status": if code.is_some() { "completed" } else { "running" },
                        "exitCode": code,
                    })
                })
                .collect();
            Ok(json!({"jobs": list}))
        }
    }
}

const SHELL_OUTPUT_LIMIT: usize = 64 * 1024;

/// Retain a bounded prefix, but keep draining so a verbose command can still exit.
async fn drain_shell_output(mut stream: impl AsyncRead + Unpin) -> Result<Vec<u8>> {
    let mut retained = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok(retained);
        }
        let keep = count.min(SHELL_OUTPUT_LIMIT - retained.len());
        retained.extend_from_slice(&buffer[..keep]);
    }
}

async fn run_shell_command(mut command: tokio::process::Command) -> Result<(String, i32)> {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    crate::agent::process::Process::prepare(&mut command);
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().context("shell stdout is unavailable")?;
    let stderr = child.stderr.take().context("shell stderr is unavailable")?;
    // This also resumes the suspended Windows process; dropping it kills owned descendants.
    let mut owned = crate::agent::process::Process::new(child)?;
    let (mut out, err, status) =
        tokio::try_join!(drain_shell_output(stdout), drain_shell_output(stderr), owned.wait_owned())?;
    out.extend(err);
    Ok((String::from_utf8_lossy(&out).into_owned(), status.code().unwrap_or(-1)))
}

/// Host API methods for UI clients (`listOpenItems`, `dismissOpenItem`, `listBookmarks`, `saveBookmark`, `deleteBookmark`).
pub fn api_call(hub: &Hub, method: &str, b: &Value) -> Result<Value> {
    let bot_id = b["botId"].as_str().context("`botId` is required")?;
    match method {
        "listOpenItems" => Ok(json!({"items": hub.store.list_open_items(bot_id)?})),
        "dismissOpenItem" => {
            let id = b["id"].as_str().context("`id` is required")?;
            let status = b["status"].as_str().unwrap_or("dismissed");
            let closed = hub.store.close_open_item(bot_id, id, status, b["note"].as_str())?;
            Ok(json!({"closed": closed}))
        }
        "listBookmarks" => Ok(json!({"bookmarks": hub.store.list_bookmarks(bot_id)?})),
        "saveBookmark" => {
            let entry_id = b["entryId"].as_str().context("`entryId` is required")?;
            let bookmark = hub.store.save_bookmark(
                bot_id,
                entry_id,
                b["threadId"].as_str(),
                b["author"].as_str().unwrap_or(bot_id),
                b["preview"].as_str().unwrap_or_default(),
                b["note"].as_str().unwrap_or_default(),
            )?;
            Ok(json!({"bookmark": bookmark}))
        }
        "deleteBookmark" => {
            let entry_id = b["entryId"].as_str().or_else(|| b["id"].as_str()).context("`entryId` is required")?;
            let deleted = hub.store.delete_bookmark(bot_id, entry_id)?;
            Ok(json!({"deleted": deleted}))
        }
        _ => bail!("unknown open item api method: {method}"),
    }
}

#[cfg(test)]
mod shell_job_tests {
    use super::*;

    #[tokio::test]
    async fn verbose_shell_job_finishes_with_bounded_stdout_and_stderr() {
        let python = if cfg!(windows) { "python" } else { "python3" };
        let lookup = tokio::process::Command::new(python)
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            .await
            .unwrap();
        assert!(lookup.status.success());
        let executable = String::from_utf8(lookup.stdout).unwrap();
        let mut command = tokio::process::Command::new(executable.trim());
        command.args([
            "-c",
            "import sys; sys.stdout.write('a' * 262144); sys.stdout.flush(); sys.stderr.write('b' * 262144); sys.stderr.flush()",
        ]);
        let (output, status) =
            tokio::time::timeout(std::time::Duration::from_secs(30), Box::pin(run_shell_command(command)))
                .await
                .expect("verbose commands must not deadlock on their output pipes")
                .unwrap();
        assert_eq!(status, 0);
        assert_eq!(output, format!("{}{}", "a".repeat(SHELL_OUTPUT_LIMIT), "b".repeat(SHELL_OUTPUT_LIMIT)));
    }
}
