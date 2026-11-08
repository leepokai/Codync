//! ComfyUI's HTTP API as the pinned version's `server.py` defines it: `/prompt`, `/queue`,
//! `/history`, `/interrupt`, `/view`, `/upload/image` and `/object_info`. The server listens on
//! loopback only and has no auth, so a call is a plain request.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::time::Duration;

const CALL_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Api {
    base: String,
}

impl Api {
    pub fn new(port: u16) -> Self {
        Self { base: format!("http://127.0.0.1:{port}") }
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let resp = crate::http().get(format!("{}{path}", self.base)).timeout(CALL_TIMEOUT).send().await;
        let resp = resp.with_context(|| format!("ComfyUI {path}"))?;
        resp.error_for_status()
            .with_context(|| format!("ComfyUI {path}"))?
            .json()
            .await
            .context("reading ComfyUI's reply")
    }

    /// Every node class the running server knows, with its inputs (built-in and custom).
    pub async fn object_info(&self) -> Result<Value> {
        self.get("/object_info").await
    }

    /// Queues a graph in ComfyUI's API format; returns its `prompt_id`. A graph the server refuses
    /// comes back as the server's own message.
    pub async fn submit(&self, graph: &Value, client_id: &str) -> Result<String> {
        let resp = crate::http()
            .post(format!("{}/prompt", self.base))
            .timeout(CALL_TIMEOUT)
            .json(&json!({ "prompt": graph, "client_id": client_id }))
            .send()
            .await
            .context("ComfyUI /prompt")?;
        let ok = resp.status().is_success();
        let body: Value = resp.json().await.context("reading ComfyUI's reply")?;
        if !ok {
            bail!("ComfyUI refused the workflow: {}", refusal(&body));
        }
        body["prompt_id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("ComfyUI did not return a prompt_id"))
    }

    /// `queue_running` and `queue_pending` as ComfyUI lists them.
    pub async fn queue(&self) -> Result<Value> {
        self.get("/queue").await
    }

    /// The finished record of one prompt; `Null` while it has none.
    pub async fn history(&self, prompt_id: &str) -> Result<Value> {
        Ok(self.get(&format!("/history/{prompt_id}")).await?[prompt_id].clone())
    }

    /// Stops the prompt that is running.
    pub async fn interrupt(&self) -> Result<()> {
        let resp = crate::http().post(format!("{}/interrupt", self.base)).timeout(CALL_TIMEOUT).send().await;
        resp.context("ComfyUI /interrupt")?.error_for_status().context("ComfyUI /interrupt")?;
        Ok(())
    }

    /// Removes a prompt that has not started.
    pub async fn dequeue(&self, prompt_id: &str) -> Result<()> {
        let resp = crate::http()
            .post(format!("{}/queue", self.base))
            .timeout(CALL_TIMEOUT)
            .json(&json!({ "delete": [prompt_id] }))
            .send()
            .await;
        resp.context("ComfyUI /queue")?.error_for_status().context("ComfyUI /queue")?;
        Ok(())
    }

    /// An image ComfyUI produced or received, as bytes.
    pub async fn view(&self, filename: &str, subfolder: &str, kind: &str) -> Result<Vec<u8>> {
        let mut url = reqwest::Url::parse(&format!("{}/view", self.base)).context("ComfyUI /view")?;
        url.query_pairs_mut()
            .append_pair("filename", filename)
            .append_pair("subfolder", subfolder)
            .append_pair("type", kind);
        let resp = crate::http().get(url).timeout(Duration::from_secs(60)).send().await.context("ComfyUI /view")?;
        let bytes = resp.error_for_status().context("ComfyUI /view")?.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Puts an input image in ComfyUI's input folder; returns the name a `LoadImage` node uses.
    pub async fn upload_image(&self, name: &str, bytes: Vec<u8>) -> Result<String> {
        let part = reqwest::multipart::Part::bytes(bytes).file_name(name.to_owned());
        let form = reqwest::multipart::Form::new().part("image", part).text("overwrite", "true");
        let resp = crate::http()
            .post(format!("{}/upload/image", self.base))
            .timeout(Duration::from_secs(60))
            .multipart(form)
            .send()
            .await
            .context("ComfyUI /upload/image")?;
        let body: Value = resp.error_for_status().context("ComfyUI /upload/image")?.json().await?;
        body["name"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("ComfyUI did not return the uploaded name"))
    }
}

/// The server's reason for a 400: its error text plus the per-node errors.
fn refusal(body: &Value) -> String {
    let reason = body["error"]["message"].as_str().or_else(|| body["error"].as_str()).unwrap_or("invalid workflow");
    let nodes = body["node_errors"].as_object().into_iter().flatten().flat_map(|(id, node)| {
        let class = node["class_type"].as_str().unwrap_or("?");
        node["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .map(move |error| format!("; node {id} ({class}): {}", error["message"].as_str().unwrap_or("error")))
    });
    std::iter::once(reason.to_owned()).chain(nodes).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_names_the_node_that_failed() {
        let body = json!({
            "error": {"message": "Prompt outputs failed validation"},
            "node_errors": {"3": {"class_type": "KSampler", "errors": [{"message": "Required input is missing: model"}]}},
        });
        assert_eq!(
            refusal(&body),
            "Prompt outputs failed validation; node 3 (KSampler): Required input is missing: model"
        );
    }
}
