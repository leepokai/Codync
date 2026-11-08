//! ComfyUI jobs: one at a time. A job is a workflow queued with `POST /prompt`, followed through
//! `/queue` and `/history`, cancelled with `/interrupt` (or removed from the queue), and, when it
//! finishes, its images go to the chat of the bot that asked.

use super::comfy::{self, MODELS_URL};
use super::comfy_api::Api;
use crate::LockExt;
use crate::hub::Hub;
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

const POLL_EVERY: Duration = Duration::from_secs(1);
/// A job that runs longer than this is stopped.
const MAX_RUN: Duration = Duration::from_mins(30);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Queued,
    Running,
    Done,
    Cancelled,
    Error,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
        }
    }

    fn active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Clone)]
struct Job {
    id: String,
    bot: String,
    phase: Phase,
    error: Option<String>,
    files: Vec<PathBuf>,
}

impl Job {
    fn public(&self) -> Value {
        json!({
            "id": self.id,
            "phase": self.phase.name(),
            "error": self.error,
            "files": self.files.iter().map(|f| f.to_string_lossy()).collect::<Vec<_>>(),
        })
    }
}

pub struct Jobs {
    /// Held while a job is validated and queued, so two bots asking at once can't both get the slot.
    gate: tokio::sync::Mutex<()>,
    current: Mutex<Option<Job>>,
    events: broadcast::Sender<Value>,
}

impl Jobs {
    pub fn new(events: broadcast::Sender<Value>) -> Self {
        Self { gate: tokio::sync::Mutex::new(()), current: Mutex::new(None), events }
    }

    /// The last job (finished or not), for the card and the `job_status` tool.
    pub fn status(&self) -> Value {
        self.current.locked().as_ref().map_or(Value::Null, Job::public)
    }

    /// The first image of the last finished job.
    pub fn first_file(&self) -> Option<PathBuf> {
        self.current.locked().as_ref().filter(|j| j.phase == Phase::Done).and_then(|j| j.files.first().cloned())
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut Job)) {
        let mut guard = self.current.locked();
        let Some(job) = guard.as_mut().filter(|j| j.id == id && j.phase.active()) else { return };
        change(job);
        let _ = self.events.send(json!({ "type": "toolJob", "tool": "comfy", "job": job.public() }));
    }

    /// Validates and queues `graph` for `bot`; the job is followed in the background.
    pub async fn submit(self: &Arc<Self>, hub: &Arc<Hub>, bot: &str, graph: &Value) -> Result<Value> {
        let _slot = self.gate.lock().await;
        ensure!(
            !self.current.locked().as_ref().is_some_and(|j| j.phase.active()),
            "ComfyUI runs one job at a time: wait for the current one or cancel it"
        );
        let api = running_api(hub)?;
        validate(graph, &api.object_info().await?)?;
        let id = api.submit(graph, "codync").await?;
        let job = Job { id: id.clone(), bot: bot.to_owned(), phase: Phase::Queued, error: None, files: vec![] };
        *self.current.locked() = Some(job.clone());
        let _ = self.events.send(json!({ "type": "toolJob", "tool": "comfy", "job": job.public() }));
        tokio::spawn(self.clone().follow(hub.clone(), id));
        Ok(job.public())
    }

    /// Stops the current job: interrupts it when it runs, removes it when it waits.
    pub async fn cancel(&self, hub: &Arc<Hub>) -> Result<Value> {
        let Some(job) = self.current.locked().clone().filter(|j| j.phase.active()) else {
            bail!("there is no ComfyUI job to cancel");
        };
        let api = running_api(hub)?;
        if job.phase == Phase::Running {
            api.interrupt().await?;
        } else {
            api.dequeue(&job.id).await?;
        }
        self.update(&job.id, |j| j.phase = Phase::Cancelled);
        Ok(self.status())
    }

    async fn follow(self: Arc<Self>, hub: Arc<Hub>, id: String) {
        let started = tokio::time::Instant::now();
        loop {
            tokio::time::sleep(POLL_EVERY).await;
            let Some(job) = self.current.locked().clone().filter(|j| j.id == id && j.phase.active()) else { return };
            let Ok(api) = running_api(&hub) else {
                self.update(&id, |j| {
                    j.phase = Phase::Error;
                    j.error = Some("ComfyUI stopped while the job was running".into());
                });
                return;
            };
            if started.elapsed() > MAX_RUN {
                let _ = api.interrupt().await;
                self.update(&id, |j| {
                    j.phase = Phase::Error;
                    j.error = Some("the job took longer than 30 minutes and was stopped".into());
                });
                return;
            }
            match self.poll(&api, &id).await {
                Ok(Some(done)) => {
                    let files = done.files.clone();
                    self.update(&id, |j| *j = done);
                    for file in files {
                        let args = json!({ "path": file });
                        if let Err(e) = crate::chat::outbox::call(&hub, &job.bot, "send_file", &args).await {
                            tracing::warn!(error = format!("{e:#}"), "sending the ComfyUI image to the chat");
                        }
                    }
                    return;
                }
                Ok(None) => {}
                Err(e) => tracing::debug!(error = format!("{e:#}"), "following the ComfyUI job"),
            }
        }
    }

    /// One look at the server: the finished job, or `None` while it still waits or runs.
    async fn poll(&self, api: &Api, id: &str) -> Result<Option<Job>> {
        let record = api.history(id).await?;
        let mut job = self.current.locked().clone().filter(|j| j.id == id).unwrap_or_else(|| Job {
            id: id.to_owned(),
            bot: String::new(),
            phase: Phase::Queued,
            error: None,
            files: vec![],
        });
        if !record.is_null() {
            if record["status"]["status_str"] == "error" {
                job.phase = Phase::Error;
                job.error = Some(failure(&record));
            } else {
                job.phase = Phase::Done;
                job.files = output_files(&record);
            }
            return Ok(Some(job));
        }
        let running = api.queue().await?["queue_running"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row.get(1).and_then(Value::as_str) == Some(id)));
        if running {
            self.update(id, |j| j.phase = Phase::Running);
        }
        Ok(None)
    }
}

fn running_api(hub: &Hub) -> Result<Api> {
    ensure!(hub.tools.comfy_running(), "ComfyUI is not running: start it from Extensions › Apps");
    Ok(Api::new(hub.tools.comfy_port()))
}

/// The server's error text from a failed run's `status.messages`.
fn failure(record: &Value) -> String {
    record["status"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m[0] == "execution_error")
        .and_then(|m| m[1]["exception_message"].as_str())
        .unwrap_or("the workflow failed")
        .trim()
        .to_owned()
}

/// The saved images of a finished run, as paths inside ComfyUI's output folder.
pub(super) fn output_files(record: &Value) -> Vec<PathBuf> {
    let root = comfy::output_dir();
    record["outputs"]
        .as_object()
        .into_iter()
        .flat_map(|nodes| nodes.values())
        .flat_map(|node| node["images"].as_array().into_iter().flatten())
        .filter(|image| image["type"] == "output")
        .filter_map(|image| {
            let (name, sub) = (image["filename"].as_str()?, image["subfolder"].as_str().unwrap_or_default());
            let unsafe_part = |s: &str| s.split(['/', '\\']).any(|p| p == "..") || s.contains(':');
            (!unsafe_part(name) && !unsafe_part(sub)).then(|| root.join(sub).join(name))
        })
        .collect()
}

/// Checks a graph against the node classes the server knows, for a message better than a bare 400:
/// unknown nodes come with near names, and a missing checkpoint says where to put it.
pub fn validate(graph: &Value, info: &Value) -> Result<()> {
    let Some(nodes) = graph.as_object().filter(|n| !n.is_empty()) else {
        bail!("the workflow must be a ComfyUI API-format graph: an object of numbered nodes");
    };
    for (id, node) in nodes {
        let Some(class) = node["class_type"].as_str() else { bail!("node {id} has no class_type") };
        let Some(spec) = info.get(class) else {
            bail!("node {id}: ComfyUI has no node called `{class}`.{}", near(class, info));
        };
        let Some(wanted) = node["inputs"]["ckpt_name"].as_str() else { continue };
        let installed: Vec<&str> = spec["input"]["required"]["ckpt_name"][0]
            .as_array()
            .map(|all| all.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !installed.contains(&wanted) {
            bail!(
                "checkpoint `{wanted}` is not installed. Put the file in {} (the Models button opens it) or download one from {MODELS_URL}. Installed: {}",
                comfy::checkpoints_dir().display(),
                if installed.is_empty() { "none".into() } else { installed.join(", ") }
            );
        }
    }
    Ok(())
}

/// Up to five known node names that contain the start of `class`.
fn near(class: &str, info: &Value) -> String {
    let head: String = class.chars().take(5).collect::<String>().to_lowercase();
    let found: Vec<&str> = info
        .as_object()
        .into_iter()
        .flat_map(|all| all.keys())
        .filter(|k| k.to_lowercase().contains(&head))
        .take(5)
        .map(String::as_str)
        .collect();
    if found.is_empty() { String::new() } else { format!(" Similar: {}.", found.join(", ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Value {
        json!({
            "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["a.safetensors"]]}}},
            "KSampler": {"input": {"required": {}}},
        })
    }

    #[test]
    fn a_known_graph_passes() {
        let graph = json!({"1": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "a.safetensors"}}});
        validate(&graph, &info()).expect("valid");
    }

    #[test]
    fn an_unknown_node_gets_near_names() {
        let graph = json!({"1": {"class_type": "KSamplerAdvanced", "inputs": {}}});
        let err = validate(&graph, &info()).expect_err("unknown").to_string();
        assert!(err.contains("no node called `KSamplerAdvanced`") && err.contains("Similar: KSampler"), "{err}");
    }

    #[test]
    fn a_missing_checkpoint_says_where_to_put_it() {
        let graph = json!({"1": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "b.safetensors"}}});
        let err = validate(&graph, &info()).expect_err("missing").to_string();
        assert!(err.contains("`b.safetensors` is not installed") && err.contains("Installed: a.safetensors"), "{err}");
    }

    #[test]
    fn output_paths_stay_inside_the_output_folder() {
        let record = json!({"outputs": {"9": {"images": [
            {"filename": "ok.png", "subfolder": "", "type": "output"},
            {"filename": "..\\evil.png", "subfolder": "", "type": "output"},
            {"filename": "temp.png", "subfolder": "", "type": "temp"},
        ]}}});
        let files = output_files(&record);
        assert_eq!(files, vec![comfy::output_dir().join("").join("ok.png")]);
    }
}
