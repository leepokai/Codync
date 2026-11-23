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

/// The tool that shares a file with the user.
pub const SEND_FILE: &str = "send_file";

/// How to talk to the user: part of every bot's instructions (`context`), and the `chat` server's.
pub const INSTRUCTIONS: &str = "In your chat with the user, the user sees only what you send with send_message; \
text you write as your reply never reaches them. Send a message whenever you have something worth telling them: \
short and plain, one thought per message, in the user's language. Lead with the result, then what needs the user. \
Use send_file(path, name?) to share a generated regular file (any type, up to 100 MiB); a local path in text is not a download. To speak your message, send_file(path, text) with the voice note's audio and the message: it plays at the top of that message. Don't send just to acknowledge, to say you started or are still working, or to repeat yourself. \
For something the user will read as a whole or come back to (a report, plan, comparison, tracker or dashboard), make an Artifact \
instead of a long message: write one self-contained HTML file (inline CSS and JS; it may load libraries from a CDN) and \
publish_artifact(path, title). It shows as a card in your chat and in its Library. To change an artifact, edit a copy of its HTML and \
publish_artifact(path, artifact: its id), with notify: false when the change needs no new card in your chat (a routine refresh of a tracker); never make a second artifact for the same thing. list_artifacts shows your artifacts and where each one's HTML is. \
Reactions are rare: react(emoji) only where a person would (❤️ to thanks, 👍 to a plain OK, 🎉 to big news, 😂 to a real joke), \
never to a question or a request. When a message needs no words, the reaction is the whole reply; when it needs neither, \
end your turn by replying exactly \"(pass)\".";
/// The `chat` server's instructions: [`INSTRUCTIONS`] plus how to ask the user to choose.
pub fn instructions() -> String {
    format!("{INSTRUCTIONS} {}", crate::chat::decision::INSTRUCTIONS)
}

/// Longest message accepted, in bytes.
const MAX_MESSAGE_BYTES: usize = 32 * 1024;
/// How long the bot's actor has to take the message.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// `reply_to` in the send tools' input schemas.
const REPLY_TO: &str = "Optional: the id of a message in this chat to quote above yours, so a late answer shows what it answers. \
A user message reaches you with its id ([Codync: message <id>]); search_history returns older ones, yours included. \
Leave it out when you answer the message you were just sent.";

pub fn tools() -> Value {
    json!([{
        "name": "send_message",
        "description": "Send the user one chat message (Markdown). It shows right away as its own bubble on their phone and computer; send several for several thoughts.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "The message, as the user should read it."},
                "reply_to": {"type": "string", "description": REPLY_TO},
            },
            "required": ["text"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    }, {
        "name": SEND_FILE,
        "description": "Share one regular file with the user in your own chat. Any file type, including empty files, up to 100 MiB. The host saves an immutable copy. Paths are absolute or relative to your working directory. With text, the file is a voice note (mp3, m4a, wav or aac, up to 20 MiB) that plays at the top of that message, which counts as one you sent. A successful response means the file message was published; do not repeat it.",
        "inputSchema": {"type": "object", "properties": {
            "path": {"type": "string"},
            "name": {"type": "string", "description": "Optional display filename, without folders."},
            "text": {"type": "string", "description": "Optional message (Markdown) the voice note belongs to."},
            "reply_to": {"type": "string", "description": REPLY_TO},
        }, "required": ["path"]},
        "annotations": {"readOnlyHint": false, "idempotentHint": false},
    }, crate::chat::decision::tool()])
}

pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    if name == SEND_FILE {
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
    let text = message_text(args["text"].as_str().unwrap_or_default())?;
    let (quote, sent) = reply_quote(hub, bot, args);
    deliver(hub, bot, |reply| Cmd::SendToUser { text, quote, reply }).await?;
    Ok(sent)
}

/// The id a send tool's `reply_to` names, if it names one.
pub fn reply_to(args: &Value) -> Option<&str> {
    args["reply_to"].as_str().map(str::trim).filter(|id| !id.is_empty())
}

/// `reply_to`'s quote (`replies::quote`) of a message in `chat`, where the bot's message lands, and what the tool
/// answers: a message it can't quote (unknown, deleted, not in that chat) still goes out, without the quote, and the
/// answer warns the bot.
fn reply_quote(hub: &Hub, chat: &str, args: &Value) -> (Option<Value>, Value) {
    let Some(id) = reply_to(args) else { return (None, json!({"sent": true})) };
    match crate::chat::replies::quote(&hub.store, chat, id, None) {
        Ok(quote) => (Some(quote), json!({"sent": true})),
        Err(e) => (None, json!({"sent": true, "warning": format!("sent without a quote: can't quote {id} ({e:#})")})),
    }
}

/// A message for the user, trimmed: nonempty and at most `MAX_MESSAGE_BYTES`.
fn message_text(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_MESSAGE_BYTES {
        bail!("text must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    Ok(text.to_owned())
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
    let text = match args.get("text") {
        Some(value) => Some(message_text(value.as_str().ok_or_else(|| anyhow!("text must be a string"))?)?),
        None => None,
    };
    // A thread's file shows in its bot's chat (`Actor::add_for_user`), so that is the chat it quotes from.
    let (quote, sent) = reply_quote(hub, &crate::chat::work::owner(&hub.store, bot), args);
    let guard = CancelCopy(Arc::new(std::sync::atomic::AtomicBool::new(false)));
    let (reply, delivered) = oneshot::channel();
    hub.send_cmd(bot, Cmd::SendFile { path: path.to_owned(), name, text, quote, cancelled: guard.0.clone(), reply })?;
    let result = tokio::time::timeout(Duration::from_mins(1), delivered).await;
    let delivered = result.map_err(|_| anyhow!("file sharing timed out; check the conversation before retrying"))?;
    delivered.map_err(|_| anyhow!("file sharing interrupted"))??;
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #402: a bot can reply-quote a message, so `send_message` and `send_file` take an optional `reply_to` id.
    #[test]
    fn send_tools_take_an_optional_reply_to() {
        let tools = tools();
        for name in ["send_message", SEND_FILE] {
            let tool = tools.as_array().into_iter().flatten().find(|t| t["name"] == name).expect("tool is listed");
            let schema = &tool["inputSchema"];
            assert_eq!(schema["properties"]["reply_to"]["type"], "string", "{name} takes reply_to");
            assert!(
                !schema["required"].as_array().into_iter().flatten().any(|r| r == "reply_to"),
                "{name}'s reply_to is optional"
            );
        }
    }
}
