//! How a bot talks to the user, Grok Bot's way: in its chat the user sees
//! only the messages the bot sends with the built-in `chat` MCP server's `send_message`,
//! each one its own bubble. What the agent writes as its reply stays in the trace, unless
//! it sent nothing in the turn: then its last text becomes the reply, as before.

use crate::agent::bot::Cmd;
use crate::hub::Hub;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

/// How to talk to the user: part of every bot's instructions (`context`), and the `chat` server's.
pub const INSTRUCTIONS: &str = "In your chat with the user, the user sees only what you send with send_message; \
text you write as your reply never reaches them. Send a message whenever you have something worth telling them: \
short and plain, one thought per message, in the user's language. Lead with the result, then what needs the user. \
Use send_file(path, name?) to share a generated regular file (any type, up to 100 MiB); a local path in text is not a download. Don't send just to acknowledge, to say you started or are still working, or to repeat yourself. \
For something the user will read as a whole or come back to (a report, plan, comparison, tracker or dashboard), make an Artifact \
instead of a long message: write one self-contained HTML file (inline CSS and JS; it may load libraries from a CDN) and \
publish_artifact(path, title). It shows as a card in your chat and in its Library. To change an artifact, edit a copy of its HTML and \
publish_artifact(path, artifact: its id); never make a second artifact for the same thing. list_artifacts shows your artifacts and where each one's HTML is. \
Reactions are rare: react(emoji) only where a person would (❤️ to thanks, 👍 to a plain OK, 🎉 to big news, 😂 to a real joke), \
never to a question or a request. When a message needs no words, the reaction is the whole reply; when it needs neither, \
end your turn by replying exactly \"(pass)\".";

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
        "description": "Share one regular file with the user in your own chat. Any file type, including empty files, up to 100 MiB. The host saves an immutable copy. Paths are absolute or relative to your working directory. A successful response means the file message was published; do not repeat it.",
        "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "name": {"type": "string", "description": "Optional display filename, without folders."}}, "required": ["path"]},
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    }])
}

pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    if name == "send_file" {
        return send_file(hub, bot, args).await;
    }
    if name == crate::chat::reactions::TOOL {
        let (emoji, to) =
            (args["emoji"].as_str().unwrap_or_default().to_owned(), args["to"].as_str().map(str::to_owned));
        deliver(hub, bot, |reply| Cmd::React { emoji, to, reply }).await?;
        return Ok(json!({"reacted": true}));
    }
    if name != "send_message" {
        bail!("unknown chat tool: {name}");
    }
    let text = args["text"].as_str().unwrap_or_default().trim();
    if text.is_empty() || text.len() > MAX_MESSAGE_BYTES {
        bail!("text must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    deliver(hub, bot, |reply| Cmd::SendToUser { text: text.to_owned(), reply }).await?;
    Ok(json!({"sent": true}))
}

/// Hands the bot's actor a command for its running turn and waits for its answer.
async fn deliver(hub: &Hub, bot: &str, cmd: impl FnOnce(oneshot::Sender<Result<()>>) -> Cmd) -> Result<()> {
    let (reply, delivered) = oneshot::channel();
    hub.send_cmd(bot, cmd(reply))?;
    tokio::time::timeout(DELIVERY_TIMEOUT, delivered)
        .await
        .map_err(|_| anyhow!("the bot didn't take the message in time"))?
        .map_err(|_| anyhow!("the bot shut down"))?
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
    let (reply, delivered) = oneshot::channel();
    hub.send_cmd(bot, Cmd::SendFile { path: path.to_owned(), name, cancelled: guard.0.clone(), reply })?;
    let result = tokio::time::timeout(Duration::from_mins(1), delivered).await;
    let delivered = result.map_err(|_| anyhow!("file sharing timed out; check the conversation before retrying"))?;
    delivered.map_err(|_| anyhow!("file sharing interrupted"))??;
    Ok(json!({"sent": true}))
}
