use super::{Capability, terminal, workspace};
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

pub const METHODS: &[&str] = &["computerPanel", "listComputerFiles", "readComputerFile", "runComputerCommand"];
pub fn is_tool(name: &str) -> bool {
    matches!(name, "workspace_list" | "workspace_read" | "workspace_shell")
}
pub fn tools() -> Vec<Value> {
    vec![
        json!({"name":"workspace_list","description":"List up to 500 entries in a bot workspace directory. Paths are relative; no parent traversal.","inputSchema":{"type":"object","properties":{"path":{"type":"string"}}},"annotations":{"readOnlyHint":true}}),
        json!({"name":"workspace_read","description":"Preview a UTF-8 workspace file (128 KiB maximum).","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]},"annotations":{"readOnlyHint":true}}),
        json!({"name":"workspace_shell","description":"Run one command in the bot workspace (30 seconds; output limited). Requires shell permission. This is the real host, not a sandbox.","inputSchema":{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}}),
    ]
}
pub async fn call(hub: &Arc<Hub>, method: &str, body: &Value, actor_kind: &str, actor_id: &str) -> Result<Value> {
    let bot = body["botId"].as_str().context("botId is required")?;
    if method == "computerPanel" {
        let row = hub.store.bot(bot)?.filter(|row| !row.deleted && !row.config.is_group()).context("unknown bot")?;
        return Ok(json!({"enabled":row.config.computer,"permissions":row.config.computer_permissions,
            "workspace":row.config.cwd,"screen":hub.screen.state(),"activity":hub.store.computer_activity(bot)?,"protectedBrowserAvailable":cfg!(windows),
            "services":{"research":crate::research::configuration_state(&hub.store),"slack":crate::slack::configuration_state(&hub.store)}}));
    }
    let name = match method {
        "listComputerFiles" => "workspace_list",
        "readComputerFile" => "workspace_read",
        "runComputerCommand" => "workspace_shell",
        _ => bail!("unknown computer panel method"),
    };
    let result = tool(hub, bot, name, body).await;
    let outcome = if result.as_ref().is_ok_and(|value| value["success"] != false) {
        crate::store::ComputerActivityResult::Success
    } else {
        crate::store::ComputerActivityResult::Failed
    };
    if let Err(error) = hub.store.record_computer_activity_as(bot, name, outcome, actor_kind, actor_id) {
        tracing::warn!(%error, "couldn't retain owner computer activity");
    }
    result
}
pub async fn tool(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    let capability = if name == "workspace_shell" { Capability::Shell } else { Capability::Files };
    let mut lease = hub.computer.begin(hub, bot, capability)?;
    let root = PathBuf::from(hub.store.bot(bot)?.context("unknown bot")?.config.cwd);
    if name == "workspace_shell" {
        return terminal::run(&root, args["command"].as_str().context("command is required")?, lease).await;
    }
    let guard = lease.clone();
    let work = async {
        match name {
            "workspace_list" => workspace::list(&root, args["path"].as_str().unwrap_or(""), &guard).await,
            "workspace_read" => {
                workspace::read(&root, args["path"].as_str().context("path is required")?, &guard).await
            }
            _ => bail!("unknown workspace tool"),
        }
    };
    tokio::select! { biased; () = lease.cancelled() => bail!("File permission revoked; request cancelled."), result = work => result }
}
