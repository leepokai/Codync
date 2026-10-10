//! How a bot talks to the user, Grok Bot's way: in its chat (and threads) the user sees
//! only the messages the bot sends with the built-in `chat` MCP server's `send_message`,
//! each one its own bubble. What the agent writes as its reply stays in the trace, unless
//! it sent nothing in the turn: then its last text becomes the reply, as before.

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// How to talk to the user: part of every bot's instructions (`context`), and the `chat` server's.
pub const INSTRUCTIONS: &str = "In your chat with the user and its threads, the user sees only what you send with send_message; \
text you write as your reply never reaches them. Send a message whenever you have something worth telling them: \
short and plain, one thought per message, in the user's language. Lead with the result, then what needs the user. \
Use send_file(path, name?) to share a generated regular file (any type, up to 100 MiB); a local path in text is not a download. Don't send just to acknowledge, to say you started or are still working, or to repeat yourself.";

/// Longest message accepted, in bytes.
const MAX_MESSAGE_BYTES: usize = 32 * 1024;
/// How long the bot's actor has to take the message.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

pub fn tools() -> Value {
    json!([{
        "name": "send_message",
        "description": "Send the user one chat message (Markdown). It shows right away as its own bubble on their phone and computer; send several for several thoughts.",
        "inputSchema": {
            "type": "object",
            "properties": {"text": {"type": "string", "description": "The message, as the user should read it."}},
            "required": ["text"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    }, {
        "name": "send_file",
        "description": "Share one regular file with the user in your own chat or thread. Any file type, including empty files, up to 100 MiB. The host saves an immutable copy. Paths are absolute or relative to your working directory. A successful response means the file message was published; do not repeat it.",
        "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "name": {"type": "string", "description": "Optional display filename, without folders."}}, "required": ["path"]},
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    }])
}

pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    if name == "send_file" {
        return send_file(hub, bot, args).await;
    }
    if name != "send_message" {
        bail!("unknown chat tool: {name}");
    }
    let text = args["text"].as_str().unwrap_or_default().trim();
    if text.is_empty() || text.len() > MAX_MESSAGE_BYTES {
        bail!("text must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    let (reply, delivered) = tokio::sync::oneshot::channel();
    hub.send_cmd(bot, Cmd::SendToUser { text: text.to_owned(), reply })?;
    tokio::time::timeout(DELIVERY_TIMEOUT, delivered)
        .await
        .map_err(|_| anyhow!("the bot didn't take the message in time"))?
        .map_err(|_| anyhow!("the bot shut down"))??;
    Ok(json!({"sent": true}))
}

struct CancelCopy(Arc<std::sync::atomic::AtomicBool>);
impl Drop for CancelCopy {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}

async fn send_file(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let path = args["path"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("path is required"))?;
    let name = match args.get("name") {
        Some(value) => Some(crate::chat::files::name(value.as_str().ok_or_else(|| anyhow!("name must be a string"))?)?),
        None => None,
    };
    let guard = CancelCopy(Arc::new(std::sync::atomic::AtomicBool::new(false)));
    let (reply, delivered) = tokio::sync::oneshot::channel();
    hub.send_cmd(bot, Cmd::SendFile { path: path.to_owned(), name, cancelled: guard.0.clone(), reply })?;
    let result = tokio::time::timeout(Duration::from_secs(60), delivered).await;
    let delivered = result.map_err(|_| anyhow!("file sharing timed out; check the conversation before retrying"))?;
    delivered.map_err(|_| anyhow!("file sharing interrupted"))??;
    Ok(json!({"sent": true}))
}
