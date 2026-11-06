//! Quote replies: a message the user sends in answer to an earlier one of the same chat (theirs or a bot's).
//! `send` stores the answered message as the new one's `replyTo`; the agent reads it as a note before the text.

use crate::agent::acp;
use crate::hub::Hub;
use crate::store::{EntryKind, Store, message_text};
use anyhow::{Result, anyhow, ensure};
use serde_json::{Value, json};

/// How much of the answered message's text the reply keeps.
const QUOTE_BYTES: usize = 500;

/// What `send` stores as `replyTo`: the message answered (`id`, in `chat`), who wrote it (`"user"` or a bot id), its
/// text (or its files' names), and the artifact when it's an artifact card.
pub fn quote(store: &Store, chat: &str, id: &str) -> Result<Value> {
    let e = store.entry(id).filter(|e| e.bot_id == chat).ok_or_else(|| anyhow!("unknown message"))?;
    let user = e.kind == EntryKind::User.as_str();
    ensure!(user || (e.kind == EntryKind::Agent.as_str() && e.data["final"] == true), "only messages take replies");
    let author = if user { "user" } else { e.data["author"].as_str().unwrap_or(chat) };
    let mut quote =
        json!({"id": e.id, "author": author, "text": acp::truncate(message_text(&e.data).trim(), QUOTE_BYTES)});
    if let Some(artifact) = e.data["artifact"]["id"].as_str() {
        // Quoted under the artifact's current title: the card's text keeps the one it was published with.
        if let Some(current) = crate::chat::artifacts::get(store, author, artifact) {
            quote["text"] = crate::chat::artifacts::card_text(&current.title).into();
        }
        quote["artifact"] = artifact.into();
    }
    Ok(quote)
}

/// The note `me` reads before a reply's text: `[Codync: this replies to your message: "…"]`, or for its own artifact's
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
    Some(format!("[Codync: this replies to {whose}: \"{text}\"]"))
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
            quote(&store, "b", &reply.id).unwrap(),
            json!({"id": reply.id, "author": "b", "text": "Plan ready"})
        );
        assert_eq!(quote(&store, "b", &files.id).unwrap()["text"], "a.png", "a files-only message quotes its files");
        assert_eq!(quote(&store, "b", &files.id).unwrap()["author"], "user");
        assert!(quote(&store, "b", &narration.id).is_err(), "the trace isn't in the chat");
        assert!(quote(&store, "other", &reply.id).is_err(), "another chat's message");
    }
}
