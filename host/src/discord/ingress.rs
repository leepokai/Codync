//! One Discord message: who may write, whether the bot answers, and the prompt that is queued.
//!
//! A bound channel answers a mention of the bot, a message in a conversation it already had, and
//! (for a bound person) a direct message. Everything else is dropped without error, so the bot can
//! sit in a busy server.

use super::{Config, Context, Hub, Result, Value, json};
use sha2::{Digest, Sha256};

pub(super) fn stable_id(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    format!("discord-{}", crate::chat::files::digest_hex(&digest.finalize()))
}

pub(super) fn accept(hub: &Hub, c: &Config, message: &Value) -> Result<()> {
    if message["author"]["bot"] == true {
        return Ok(());
    }
    let (Some(id), Some(channel)) = (message["id"].as_str(), message["channel_id"].as_str()) else {
        return Ok(());
    };
    if !valid_id(id) || !valid_id(channel) {
        return Ok(());
    }
    let Some(author) = message["author"]["id"].as_str().filter(|u| valid_id(u)) else { return Ok(()) };
    let direct = message["guild_id"].is_null();
    let allowed = |b: &super::Binding| b.allowed_users.iter().any(|u| u == author);
    let Some(binding) = (if direct {
        // A direct message has no channel to bind: any bound person may use it, with the first bot
        // that lets them in.
        c.bindings.iter().find(|b| allowed(b))
    } else if message["guild_id"] == c.guild_id.as_str() {
        c.bindings.iter().find(|b| b.channel_id == channel && allowed(b))
    } else {
        None
    }) else {
        return Ok(());
    };
    hub.store
        .bot(&binding.bot_id)?
        .filter(|bot| !bot.deleted && !bot.config.is_group())
        .context("Bound bot unavailable")?;
    let mention =
        message["mentions"].as_array().is_some_and(|list| list.iter().any(|m| m["id"] == c.bot_user_id.as_str()));
    let thread = hub.store.discord_thread_exists(&c.generation, channel)?;
    if !mention && !direct && !thread {
        return Ok(());
    }
    let text = strip_mentions(message["content"].as_str().unwrap_or_default(), &c.bot_user_id);
    let audio = crate::channels::voice_notes::Platform::Discord.attachment(message);
    let has_audio = crate::channels::voice_notes::Platform::Discord.has_audio(message);
    if (text.is_empty() && !has_audio) || text.len() > 32 * 1024 {
        return Ok(());
    }
    let prompt = prompt(message, &text);
    let prompt = if has_audio && audio.is_none() { crate::channels::voice_notes::unavailable(&prompt) } else { prompt };
    let event = json!({
        "source": "discord", "event": if mention {"message.mention"} else {"message.created"},
        "channel": channel, "user": author, "text": text, "messageId": id,
        "url": format!("https://discord.com/channels/{}/{channel}/{id}", c.guild_id),
    });
    // A routine can ask for a mention or for any message: the mention wins, as on Slack and Telegram.
    hub.routines.receive_event(&hub.store, &event, Some(id.to_owned()));
    let incoming = crate::store::discord::DiscordIncoming {
        event: id.into(),
        generation: c.generation.clone(),
        bot: binding.bot_id.clone(),
        channel: channel.into(),
        entry: stable_id(&[&c.generation, channel, id]),
        text: prompt,
        user: author.into(),
        message: id.into(),
        audio,
    };
    hub.store.enqueue_discord(&incoming)?;
    let root = stable_id(&[&c.generation, channel, "root"]);
    hub.store.map_discord_thread(&incoming, &root)?;
    Ok(())
}

/// The bot's own mention is not part of what was said.
fn strip_mentions(content: &str, bot: &str) -> String {
    content.replace(&format!("<@{bot}>"), "").replace(&format!("<@!{bot}>"), "").trim().to_owned()
}

/// What the bot is given: the message, then the names of the files that came with it.
fn prompt(message: &Value, text: &str) -> String {
    let names: Vec<&str> = message["attachments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["filename"].as_str().filter(|n| !n.is_empty() && n.len() <= 255))
        .take(10)
        .collect();
    if names.is_empty() {
        text.to_owned()
    } else {
        format!("{text}\n\n[Attached, not downloaded: {}]", names.join(", "))
    }
}

fn valid_id(value: &str) -> bool {
    (1..=32).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bots_mention_leaves_the_text() {
        assert_eq!(strip_mentions("<@42> deploy please", "42"), "deploy please");
        assert_eq!(strip_mentions("<@!42> hi", "42"), "hi");
        assert_eq!(strip_mentions("  hi  ", "42"), "hi");
        assert_eq!(strip_mentions("<@9> hi", "42"), "<@9> hi");
    }

    #[test]
    fn attachments_are_named_in_the_prompt() {
        let message = json!({"attachments": [{"filename": "a.png"}, {"filename": ""}]});
        assert_eq!(prompt(&message, "look"), "look\n\n[Attached, not downloaded: a.png]");
        assert_eq!(prompt(&json!({}), "look"), "look");
    }
}
