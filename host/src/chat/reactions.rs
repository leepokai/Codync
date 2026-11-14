//! A bot reacts to the message it's answering with one emoji (the `chat` server's `react`):
//! the user's message in its own chat, or someone's in a group's room turn. The bot's actor
//! picks the message (`Actor::react`), since it knows what the turn answers.

use crate::hub::Hub;
use crate::store::{Entry, EntryKind, Lane};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

pub const TOOL: &str = "react";
/// Room messages searched for the one to react to.
const RECENT: i64 = 24;
/// A bot reacts at most once among a chat's latest this many messages.
const SPACING: i64 = 6;

pub fn tool() -> Value {
    json!({
        "name": TOOL,
        "description": "React to the message you're answering with one emoji, as a person would in a chat app. It replaces your earlier reaction on that message.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "emoji": {"type": "string", "description": "One emoji, e.g. ❤️ 👍 🎉 😂."},
                "to": {"type": "string", "description": "Group chats: whose newest message you react to, \"user\" or a participant's name. Without it: the newest message that isn't yours."},
            },
            "required": ["emoji"],
        },
        "annotations": {"readOnlyHint": false, "idempotentHint": true},
    })
}

/// In a room, the newest message up to `shown` (the newest the bot's prompt showed) from `to`, or from anyone
/// but `bot` when `None`.
pub fn in_room(hub: &Hub, lane: &Lane, bot: &str, to: Option<&str>, shown: i64) -> Result<String> {
    let to = to.map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
    let from = |e: &Entry| match (&to, e.data["author"].as_str()) {
        (_, Some(author)) if author == bot => false,
        (None, _) => true,
        (Some(to), _) if e.kind == EntryKind::User.as_str() => to == "user",
        (Some(to), Some(author)) => hub.bot_name(author).trim().to_lowercase() == *to,
        (Some(_), None) => false,
    };
    hub.store
        .messages_after(lane, 0, RECENT)?
        .into_iter()
        .rev()
        .find(|e| e.seq <= shown && from(e))
        .map(|e| e.id)
        .ok_or_else(|| anyhow!("no recent message from {} in this chat", to.as_deref().unwrap_or("anyone else")))
}

/// Whether `bot` reacted to one of the lane's last [`SPACING`] messages other than `target`: reactions stay rare
/// whatever the agent makes of its instructions.
pub fn reacted_lately(hub: &Hub, lane: &Lane, bot: &str, target: &str) -> Result<bool> {
    let mine = |e: &Entry| e.data["botReactions"].as_array().is_some_and(|rs| rs.iter().any(|r| r["bot"] == bot));
    Ok(hub.store.messages_after(lane, 0, SPACING)?.iter().any(|e| e.id != target && mine(e)))
}

/// A room line's reactions, as members read them: ` (reactions: User 👍, Owen 🎉)`, or nothing.
pub fn describe(hub: &Hub, e: &Entry) -> String {
    let user =
        e.data["reactions"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|r| format!("User {r}"));
    let bots = e.data["botReactions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some(format!("{} {}", hub.bot_name(r["bot"].as_str()?), r["emoji"].as_str()?)));
    let all: Vec<String> = user.chain(bots).collect();
    if all.is_empty() { String::new() } else { format!(" (reactions: {})", all.join(", ")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Store};

    #[test]
    fn room_reactions_pick_the_message_and_keep_it_read() {
        let dir = std::env::temp_dir().join(format!("codync-reactions-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        for bot in [
            json!({"id": "a", "name": "Ann", "backend": "claude", "cwd": dir}),
            json!({"id": "b", "name": "Ben", "backend": "claude", "cwd": dir}),
            json!({"id": "g", "kind": "group", "name": "Crew", "members": ["a", "b"]}),
        ] {
            store.save_bot(&serde_json::from_value::<BotConfig>(bot).unwrap()).unwrap();
        }
        let hub = Hub::new(store, "reactions-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        let room = Lane::main("g");
        let said = |kind, data: Value| hub.add_entry(&room, kind, 1, &data).unwrap();
        let asked = said(EntryKind::User, json!({"text": "ship it?"}));
        let reply = said(EntryKind::Agent, json!({"text": "Shipped.", "final": true, "author": "a"}));
        let mine = said(EntryKind::Agent, json!({"text": "Nice.", "final": true, "author": "b"}));
        let later = said(EntryKind::User, json!({"text": "one more thing"}));
        hub.mark_read("g").unwrap();
        let unread = || hub.bot_value_for_test("g")["unread"].as_i64().unwrap();

        // Ben's turn was shown up to his own reply: the newest message from someone else before it is Ann's.
        let target = |to| in_room(&hub, &room, "b", to, mine.seq);
        assert_eq!(target(None).unwrap(), reply.id);
        assert_eq!(target(Some("User")).unwrap(), asked.id, "not the message his prompt didn't show");
        assert!(target(Some("Ben")).is_err(), "not his own message");
        assert_eq!(in_room(&hub, &room, "b", None, later.seq).unwrap(), later.id);

        // One reaction per bot; a read message stays read.
        hub.bot_react(&reply.id, "b", "👍").unwrap();
        let reacted = hub.bot_react(&reply.id, "b", "🎉").unwrap();
        assert_eq!(reacted.data["botReactions"], json!([{"bot": "b", "emoji": "🎉"}]));
        assert_eq!(unread(), 0);
        assert_eq!(describe(&hub, &reacted), " (reactions: Ben 🎉)");
        assert!(hub.bot_react(&reply.id, "b", "ok").is_err(), "only emoji");

        // Reactions stay rare: one per bot among the chat's latest messages (changing it is fine).
        assert!(reacted_lately(&hub, &room, "b", &later.id).unwrap());
        assert!(!reacted_lately(&hub, &room, "b", &reply.id).unwrap(), "changing its own reaction");
        assert!(!reacted_lately(&hub, &room, "a", &later.id).unwrap(), "another bot");
        // Ben's reaction is on the 2nd of 4 messages: it leaves the latest SPACING after 4 more.
        let more = |n: i64| (0..n).for_each(|_| drop(said(EntryKind::User, json!({"text": "more"}))));
        more(SPACING - 3);
        assert!(reacted_lately(&hub, &room, "b", &later.id).unwrap(), "still among the latest");
        more(1);
        assert!(!reacted_lately(&hub, &room, "b", &later.id).unwrap(), "long enough ago");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
