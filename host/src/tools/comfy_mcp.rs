//! The `comfyui` MCP tools a bot gets when its owner turns the ComfyUI connector on for it.
//! Calls reach the host as `comfyCall`; the host talks to the local ComfyUI. Nothing is generated
//! unless the bot calls `run_workflow`.

use super::comfy::{self, CONNECTOR_ID};
use super::comfy_api::Api;
use crate::hub::Hub;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::sync::Arc;

pub const INSTRUCTIONS: &str = "Make images with the ComfyUI on this computer. list_models shows the checkpoints, LoRAs and VAEs \
installed; list_nodes finds node names. run_workflow takes a workflow in ComfyUI's API format (example_workflow returns one that works) \
and returns at once: the finished images arrive in this chat by themselves. Check job_status, or stop it with cancel_job. \
One job runs at a time. Use upload_image to give a workflow an input picture (its returned name goes in a LoadImage node).";

/// The example graph: the default workflow of the repo's `script_examples/basic_api_example.py`.
const EXAMPLE: &str = include_str!("comfy_example.json");
const MAX_UPLOAD: u64 = 20 * 1024 * 1024;

pub fn tools() -> Value {
    json!([
        tool("list_models", "List the installed checkpoints, LoRAs and VAEs.", &json!({}), &[]),
        tool(
            "list_nodes",
            "List ComfyUI node names, optionally only those containing `query`.",
            &json!({"query": {"type": "string"}}),
            &[]
        ),
        tool("example_workflow", "A working text-to-image workflow in API format to start from.", &json!({}), &[]),
        tool(
            "run_workflow",
            "Queue a workflow (ComfyUI API format). Returns at once; the images are sent to this chat when done.",
            &json!({"workflow": {"type": "object", "description": "Numbered nodes with class_type and inputs."}}),
            &["workflow"],
        ),
        tool(
            "upload_image",
            "Copy a local image into ComfyUI's inputs; returns the name for a LoadImage node.",
            &json!({"path": {"type": "string"}, "name": {"type": "string"}}),
            &["path"],
        ),
        tool(
            "job_status",
            "The state of the current or last job: queued, running, done, cancelled or error.",
            &json!({}),
            &[]
        ),
        tool("cancel_job", "Stop the job that is running or waiting.", &json!({}), &[]),
    ])
}

fn tool(name: &str, description: &str, properties: &Value, required: &[&str]) -> Value {
    let read_only = matches!(name, "list_models" | "list_nodes" | "example_workflow" | "job_status");
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
        "annotations": {"readOnlyHint": read_only, "idempotentHint": read_only},
    })
}

/// One tool call from `bot`.
pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    if name == "example_workflow" {
        return serde_json::from_str(EXAMPLE).context("the bundled example workflow");
    }
    let jobs = hub.tools.comfy_jobs();
    match name {
        "run_workflow" => jobs.submit(hub, bot, &args["workflow"]).await,
        "job_status" => Ok(jobs.status()),
        "cancel_job" => jobs.cancel(hub).await,
        "list_models" => {
            let info = api(hub)?.object_info().await?;
            Ok(json!({
                "checkpoints": options(&info, "CheckpointLoaderSimple", "ckpt_name"),
                "loras": options(&info, "LoraLoader", "lora_name"),
                "vae": options(&info, "VAELoader", "vae_name"),
                "modelsFolder": comfy::checkpoints_dir(),
            }))
        }
        "list_nodes" => {
            let info = api(hub)?.object_info().await?;
            let query = args["query"].as_str().unwrap_or_default().to_lowercase();
            let names: Vec<&String> = info
                .as_object()
                .into_iter()
                .flat_map(|all| all.keys())
                .filter(|k| k.to_lowercase().contains(&query))
                .take(200)
                .collect();
            Ok(json!({ "nodes": names }))
        }
        "upload_image" => upload(hub, args).await,
        other => bail!("unknown {CONNECTOR_ID} tool: {other}"),
    }
}

/// The first image of the last finished job as a PNG/JPEG data URL, for the card's thumbnail.
pub async fn job_image(hub: &Arc<Hub>) -> Result<Value> {
    use base64::Engine as _;
    let Some(file) = hub.tools.comfy_jobs().first_file() else { return Ok(json!({ "image": null })) };
    let bytes = tokio::fs::read(&file).await.with_context(|| format!("reading {}", file.display()))?;
    let mime = match file.extension().and_then(|e| e.to_str()) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Ok(json!({ "image": format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)) }))
}

fn api(hub: &Hub) -> Result<Api> {
    anyhow::ensure!(hub.tools.comfy_running(), "ComfyUI is not running: start it from Extensions › Apps");
    Ok(Api::new(hub.tools.comfy_port()))
}

/// The choices of one input of one node class.
fn options(info: &Value, class: &str, input: &str) -> Value {
    info[class]["input"]["required"][input][0].clone()
}

async fn upload(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let path = std::path::PathBuf::from(args["path"].as_str().context("`path` is required")?);
    let meta = tokio::fs::metadata(&path).await.with_context(|| format!("reading {}", path.display()))?;
    anyhow::ensure!(meta.is_file() && meta.len() <= MAX_UPLOAD, "the image must be a file of at most 20 MB");
    let name = match args["name"].as_str() {
        Some(name) => crate::chat::files::name(name)?,
        None => path.file_name().and_then(|n| n.to_str()).context("the path has no file name")?.to_owned(),
    };
    let stored = api(hub)?.upload_image(&name, tokio::fs::read(&path).await?).await?;
    Ok(json!({ "name": stored }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_is_a_graph_of_known_nodes() {
        let graph: Value = serde_json::from_str(EXAMPLE).expect("valid JSON");
        assert_eq!(graph.as_object().expect("an object").len(), 7);
        assert_eq!(graph["9"]["class_type"], "SaveImage");
    }

    #[test]
    fn every_tool_has_a_name_and_a_schema() {
        for tool in tools().as_array().expect("a list") {
            assert!(tool["name"].is_string() && tool["inputSchema"]["type"] == "object");
        }
    }
}
