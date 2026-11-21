//! What a user message asks the agent: its id, the note on the message it replies to, its text with a
//! `/skill …` command expanded, then the paths of the files attached to it.

use crate::chat::{replies, uploads};
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, Store};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// `skills`: the asked bot's skills on. `root`: the chat's uploads (`uploads::root`; a group's
/// own in a group). `attachments`: the message's `{id, name}` upload metadata, as saved in the chat.
pub fn of_message(store: &Store, skills: &[String], root: &Path, text: &str, attachments: &[Value]) -> String {
    let asked = crate::market::expand_command(store, skills, text);
    let asked = asked.as_deref().unwrap_or(text);
    let paths: Vec<PathBuf> = attachments.iter().filter_map(|meta| uploads::path_of(root, meta)).collect();
    if paths.is_empty() { asked.to_owned() } else { format!("{asked}{}", uploads::prompt_suffix(&paths)) }
}

/// What a user message `e` in `bot`'s own chat asks the agent, led by its id, which `send_message`'s `reply_to` quotes.
/// An agent's own `/command` goes without the id: it only runs while it starts the prompt. So does what the host
/// wrote (a task's brief): the user never sees it, so there is nothing to quote.
pub fn of_entry(hub: &Hub, bot: &BotConfig, e: &Entry) -> String {
    let data = &e.data;
    let text = data["text"].as_str().unwrap_or_default();
    let attachments = data["attachments"].as_array().map_or(&[][..], Vec::as_slice);
    let asked = of_message(&hub.store, &bot.skills, &uploads::root(bot), text, attachments);
    // What a bot's chat passed on to its task's thread (`tell_task`) says so to the thread.
    let asked = if data["relay"] == true { crate::chat::work::relayed(&asked) } else { asked };
    let id = (!asked.starts_with('/') && data["from"] != "host").then(|| format!("[Codync: message {}]", e.id));
    let note = replies::note(hub, &bot.id, &data["replyTo"]);
    [id, note, Some(asked)].into_iter().flatten().collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use crate::store::{EntryKind, Lane};
    use serde_json::json;

    /// Issue #402: the turn carries the user message's id, so the bot can quote it with `reply_to`.
    #[test]
    fn a_user_message_reaches_the_agent_with_its_id() {
        let hub = hub();
        let bot = bot("b", "Ann", "/tmp");
        hub.store.save_bot(&bot).unwrap();
        let chat = Lane::main("b");
        let earlier = hub.add_entry(&chat, EntryKind::Agent, 1, &json!({"text": "Plan ready", "final": true})).unwrap();
        let plain = hub.add_entry(&chat, EntryKind::User, 2, &json!({"text": "ship it"})).unwrap();
        assert_eq!(of_entry(&hub, &bot, &plain), format!("[Codync: message {}]\nship it", plain.id));
        let quote = replies::quote(&hub.store, "b", &earlier.id, None).unwrap();
        let reply = hub.add_entry(&chat, EntryKind::User, 3, &json!({"text": "why?", "replyTo": quote})).unwrap();
        assert_eq!(
            of_entry(&hub, &bot, &reply),
            format!("[Codync: message {}]\n[Codync: this replies to your message: \"Plan ready\"]\nwhy?", reply.id)
        );
        // The agent's own commands still lead the prompt.
        let command = hub.add_entry(&chat, EntryKind::User, 4, &json!({"text": "/compact keep notes"})).unwrap();
        assert_eq!(of_entry(&hub, &bot, &command), "/compact keep notes");
        let brief = hub.add_entry(&chat, EntryKind::User, 5, &json!({"text": "[Task] x", "from": "host"})).unwrap();
        assert_eq!(of_entry(&hub, &bot, &brief), "[Task] x", "a task's brief isn't a message to quote");
    }

    #[test]
    fn what_a_chat_passes_on_to_a_thread_says_so() {
        let hub = hub();
        let bot = bot("thread-1", "Ann", "/tmp");
        hub.store.save_bot(&bot).unwrap();
        let data = json!({"text": "Also the API page.", "from": "host", "relay": true});
        let relayed = hub.add_entry(&Lane::main("thread-1"), EntryKind::User, 1, &data).unwrap();
        assert_eq!(of_entry(&hub, &bot, &relayed), crate::chat::work::relayed("Also the API page."));
    }
}
