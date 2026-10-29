//! GPT-Live session creation. Only the SDP answer and opaque session ID leave the host.

use super::{Provider, checked, key, send};
use crate::store::Store;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

pub(crate) const BACKEND_MODEL: &str = "gpt-6-luna";
const MAX_SDP_BYTES: usize = 64 * 1024;

pub(crate) fn is_model(model: &str) -> bool {
    checked(model).is_ok()
        && model.strip_prefix("gpt-live-").is_some_and(|s| s.starts_with(|c: char| c.is_ascii_digit()))
        && !["transcribe", "translate", "tts"].iter().any(|s| model.contains(s))
}

/// Settings tests check model access without creating a billable, silent media session.
pub(crate) async fn validate_access(key: &str, model: &str) -> Result<()> {
    if !is_model(model) {
        bail!("Choose a GPT-Live conversation model.");
    }
    let answer =
        send(Provider::OpenAi, crate::http().get(format!("https://api.openai.com/v1/models/{model}")).bearer_auth(key))
            .await?;
    if answer["id"] != model {
        bail!("OpenAI did not confirm access to the selected GPT-Live model.");
    }
    Ok(())
}

pub(crate) fn request(
    model: &str,
    voice: &str,
    sdp: &str,
    instructions: &str,
    backend: &str,
    tools: &Value,
) -> Result<Value> {
    if !is_model(model) {
        bail!("Choose a GPT-Live conversation model.");
    }
    checked(voice)?;
    if !sdp.starts_with("v=0") || sdp.len() > MAX_SDP_BYTES {
        bail!("A valid WebRTC SDP offer of at most 64 KiB is required.");
    }
    let tools: Vec<Value> = tools
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            json!({
                "type": "function", "name": t["name"], "description": t["description"],
                "parameters": t["parameters"], "strict": false,
            })
        })
        .collect();
    Ok(json!({
        "session": {
            "model": model, "instructions": instructions, "store": false,
            "audio": {"output": {"voice": voice}},
            "delegation": {"type": "responses", "responses": {
                "model": BACKEND_MODEL, "instructions": backend, "tools": tools,
                "parallel_tool_calls": false,
            }},
            "client": {"data_channel": {
                "allowed_client_events": ["response.item.create", "response.create", "session.commentary.append",
                    "session.thinking.append", "session.instructions.append", "session.close"],
                // Token deltas are unused by the host and can saturate the clients' ordered RPC queues.
                "allowed_server_events": [
                    {"type": "session.started"}, {"type": "session.closed"}, {"type": "error"},
                    {"type": "session.input_transcript.delta"}, {"type": "session.output_transcript.delta"},
                    {"type": "response.event", "response_event": "response.created"},
                    {"type": "response.event", "response_event": "response.output_item.done"},
                    {"type": "response.event", "response_event": "response.completed"},
                    {"type": "response.event", "response_event": "response.failed"},
                    {"type": "response.event", "response_event": "response.incomplete"},
                    {"type": "response.event", "response_event": "response.cancelled"},
                ],
            }},
        },
        "transport": {"type": "webrtc", "sdp": sdp},
    }))
}

pub(crate) async fn create(store: &Store, body: Value) -> Result<(String, String)> {
    let key = key(store, Provider::OpenAi)?;
    let result = send(
        Provider::OpenAi,
        crate::http().post("https://api.openai.com/v1/live/sessions").bearer_auth(key).json(&body),
    )
    .await?;
    let id = result["session"]["id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or_else(|| anyhow!("OpenAI sent no Live session ID."))?;
    let sdp = result["transport"]["sdp"]
        .as_str()
        .filter(|s| s.starts_with("v=0") && s.len() <= MAX_SDP_BYTES)
        .ok_or_else(|| anyhow!("OpenAI sent no valid Live SDP answer."))?;
    Ok((id.to_owned(), sdp.to_owned()))
}

/// A call ended during SDP setup. Close the newly-created session rather than orphaning it.
pub(crate) async fn close_abandoned(store: &Store, id: &str) {
    use futures::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest as _};
    let close = async {
        let key = key(store, Provider::OpenAi)?;
        let mut url = reqwest::Url::parse("wss://api.openai.com/v1/live/sessions/")?;
        url.path_segments_mut().map_err(|()| anyhow!("Invalid Live URL."))?.pop_if_empty().push(id).push("attach");
        let mut request = url.as_str().into_client_request()?;
        request.headers_mut().insert("Authorization", format!("Bearer {key}").parse()?);
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await?;
        socket
            .send(Message::Text(
                json!({"type": "session.close", "event_id": uuid::Uuid::new_v4().to_string()}).to_string().into(),
            ))
            .await?;
        while let Some(message) = socket.next().await {
            if let Message::Text(text) = message?
                && serde_json::from_str::<Value>(&text).is_ok_and(|v| v["type"] == "session.closed")
            {
                return Ok::<_, anyhow::Error>(());
            }
        }
        bail!("Live session closed without finalization.")
    };
    if !matches!(tokio::time::timeout(std::time::Duration::from_secs(3), close).await, Ok(Ok(()))) {
        tracing::warn!("couldn't confirm closure of an abandoned GPT-Live session");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_live_conversation_models_are_accepted() {
        assert!(is_model("gpt-live-1"));
        assert!(is_model("gpt-live-1.1"));
        for id in ["gpt-live-transcribe", "gpt-realtime-2.1", "gpt-live-1-transcribe", "gpt-live-1/other"] {
            assert!(!is_model(id), "{id}");
        }
    }

    #[test]
    fn live_uses_server_owned_tools_without_a_client_credential() {
        let body =
            request("gpt-live-1", "marin", "v=0\r\n", "Speak", "Route", &crate::chat::calls::tools::declarations())
                .unwrap();
        assert_eq!(body["session"]["delegation"]["responses"]["model"], BACKEND_MODEL);
        assert_eq!(body["session"]["delegation"]["responses"]["tools"].as_array().unwrap().len(), 6);
        assert_eq!(body["session"]["store"], false);
        assert!(body["session"]["credential"].is_null());
        assert!(request("gpt-live-1", "marin", &"x".repeat(MAX_SDP_BYTES + 1), "", "", &json!([])).is_err());
    }

    #[test]
    fn data_channel_keeps_tool_lifecycle_without_streaming_backend_tokens() {
        let body = request("gpt-live-1", "marin", "v=0\r\n", "", "", &json!([])).unwrap();
        let allowed = body["session"]["client"]["data_channel"]["allowed_server_events"].as_array().unwrap();
        let receives = |kind: &str, nested: Option<&str>| {
            allowed.iter().any(|selector| selector["type"] == kind && selector["response_event"].as_str() == nested)
        };
        // A long argument/text generation must not enqueue thousands of client-to-host RPCs.
        for kind in ["response.function_call_arguments.delta", "response.output_text.delta", "response.in_progress"] {
            assert!(!receives("response.event", Some(kind)));
        }
        for kind in [
            "response.created",
            "response.output_item.done",
            "response.completed",
            "response.failed",
            "response.incomplete",
            "response.cancelled",
        ] {
            assert!(receives("response.event", Some(kind)), "missing lifecycle event: {kind}");
        }
        for kind in [
            "session.started",
            "session.closed",
            "error",
            "session.input_transcript.delta",
            "session.output_transcript.delta",
        ] {
            assert!(receives(kind, None), "missing call event: {kind}");
        }
    }
}
