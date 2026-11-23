//! Quote replies: a message the user sends in answer to an earlier one of the same chat (theirs or a bot's).
//! `send` stores the answered message as the new one's `replyTo`; the agent reads it as a note before the text.
//! A bot quotes the same way, with `reply_to` on `send_message` and `send_file` (`chat::outbox`).
//! A reply can answer one part of the message, the text the user selected in it: the quote is then that part.

use crate::hub::Hub;
use crate::store::{EntryKind, Store, message_text};
use anyhow::{Result, anyhow, ensure};
use codync_agent::acp;
use serde_json::{Value, json};

/// How much of the answered message's text the reply keeps.
const QUOTE_BYTES: usize = 500;
/// How much of a selected part it keeps: the user picked it on purpose, so it gets more room than a whole message.
const PART_BYTES: usize = 4000;

/// What `send` stores as `replyTo`: the message answered (`id`, in `chat`), who wrote it (`"user"` or a bot id), its
/// text (or its files' names), and the artifact when it's an artifact card. With `part`, the text the user selected in
/// that message, the quote is that part (`part: true`). The part is taken as sent: it comes from the user's own app, and
/// what they select in the rendered message isn't a substring of its Markdown source.
pub fn quote(store: &Store, chat: &str, id: &str, part: Option<&str>) -> Result<Value> {
    let e = store.entry(id).filter(|e| e.bot_id == chat).ok_or_else(|| anyhow!("unknown message"))?;
    let user = e.kind == EntryKind::User.as_str();
    ensure!(e.is_message(), "only messages take replies");
    let author = if user { "user" } else { e.data["author"].as_str().unwrap_or(chat) };
    let mut quote = quote_of(&e.id, author, message_text(&e.data).trim());
    if let Some(artifact) = e.data["artifact"]["id"].as_str() {
        // Quoted under the artifact's current title: the card's text keeps the one it was published with.
        if let Some(current) = crate::chat::artifacts::get(store, author, artifact) {
            quote["text"] = crate::chat::artifacts::card_text(&current.title).into();
        }
        quote["artifact"] = artifact.into();
    } else if let Some(part) = part.map(str::trim).filter(|p| !p.is_empty()) {
        quote["text"] = acp::truncate(part, PART_BYTES).into();
        quote["part"] = true.into();
    }
    Ok(quote)
}

/// A `replyTo` quoting `text` of message `id` by `author`, as long as a quote keeps.
pub fn quote_of(id: &str, author: &str, text: &str) -> Value {
    json!({"id": id, "author": author, "text": acp::truncate(text, QUOTE_BYTES)})
}

/// A bot message's `data` with the message it quotes (`reply_to`, a `quote`), if any, as its `replyTo`.
pub fn with_quote(mut data: Value, quote: Option<Value>) -> Value {
    if let Some(quote) = quote {
        data["replyTo"] = quote;
    }
    data
}

/// The note `me` reads before a reply's text: `[Codync: this replies to your message: "…"]` (`… to this part of your
/// message: "…"]` when the user selected a part), or for its own artifact's
/// card, where that artifact is (`artifacts::note`). Coming from Codync, it stands even when the quoted message is no longer
/// in the agent's session.
pub fn note(hub: &Hub, me: &str, reply: &Value) -> Option<String> {
    if reply["author"] == me
        && let Some(note) =
            reply["artifact"].as_str().and_then(|artifact| crate::chat::artifacts::note(&hub.store, me, artifact))
    {
        return Some(note);
    }
    let text = reply["text"].as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
    let whose = match reply["author"].as_str().unwrap_or_default() {
        "user" => "the user's earlier message".to_owned(),
        author if author == me => "your message".to_owned(),
        author => format!("{}'s message", hub.bot_name(author)),
    };
    let part = if reply["part"] == true { "this part of " } else { "" };
    Some(format!("[Codync: this replies to {part}{whose}: \"{text}\"]"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Lane;

    #[test]
    fn quotes_messages_of_the_same_chat_only() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let chat = Lane::main("b");
        let reply =
            store.insert_entry(&chat, EntryKind::Agent, 1, &json!({"text": "Plan ready", "final": true})).unwrap();
        let narration = store.insert_entry(&chat, EntryKind::Agent, 1, &json!({"text": "reading…"})).unwrap();
        let files = store
            .insert_entry(&chat, EntryKind::User, 2, &json!({"text": "", "attachments": [{"name": "a.png"}]}))
            .unwrap();
        assert_eq!(
            quote(&store, "b", &reply.id, None).unwrap(),
            json!({"id": reply.id, "author": "b", "text": "Plan ready"})
        );
        assert_eq!(
            quote(&store, "b", &files.id, None).unwrap()["text"],
            "a.png",
            "a files-only message quotes its files"
        );
        assert_eq!(quote(&store, "b", &files.id, None).unwrap()["author"], "user");
        assert!(quote(&store, "b", &narration.id, None).is_err(), "the trace isn't in the chat");
        assert!(quote(&store, "other", &reply.id, None).is_err(), "another chat's message");
    }

    #[test]
    fn a_selected_part_replaces_the_whole_message_in_the_quote() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let chat = Lane::main("b");
        let reply = store
            .insert_entry(
                &chat,
                EntryKind::Agent,
                1,
                &json!({"text": "Step one.\n\n**Step two** is risky.", "final": true}),
            )
            .unwrap();
        // The rendered text, without the Markdown markers of the source.
        let part = quote(&store, "b", &reply.id, Some("  Step two is risky. ")).unwrap();
        assert_eq!(part, json!({"id": reply.id, "author": "b", "text": "Step two is risky.", "part": true}));
        assert_eq!(quote(&store, "b", &reply.id, Some("  ")).unwrap()["text"], "Step one.\n\n**Step two** is risky.");
        let long = "x".repeat(PART_BYTES + 100);
        let kept = quote(&store, "b", &reply.id, Some(&long)).unwrap();
        assert!(kept["text"].as_str().unwrap().len() <= PART_BYTES + 4, "a long part is cut");
    }
}
