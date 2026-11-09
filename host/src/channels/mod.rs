//! What every messaging channel (Slack, Telegram, Discord) hands the host and takes back, so the host's own code never
//! learns a provider's wire format. A provider module turns its events into [`Incoming`]; the rest is the host's.

use anyhow::Context;
use serde_json::Value;

/// A file a person attached to a message. Only its name travels for now: the host does not download it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    pub name: String,
}

/// One message from a person, normalized: the same shape whatever channel it came through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incoming {
    /// Where it was said (a Slack channel id, a Telegram chat id).
    pub channel: String,
    /// Who said it, as the provider names them.
    pub person: String,
    /// The conversation it belongs to; the message's own id when it starts one.
    pub thread: String,
    pub text: String,
    pub files: Vec<FileRef>,
}

impl Incoming {
    /// The text the bot is given: the message, then the names of the files that came with it.
    pub fn prompt(&self) -> String {
        if self.files.is_empty() {
            return self.text.clone();
        }
        let names: Vec<&str> = self.files.iter().map(|f| f.name.as_str()).collect();
        format!("{}\n\n[Attached, not downloaded: {}]", self.text, names.join(", "))
    }
}

/// The pictures a channel can wear, as SF Symbols both clients already draw.
pub const ICONS: &[&str] = &[
    "bubble.left",
    "book",
    "hammer",
    "globe",
    "person.2",
    "bolt",
    "gearshape",
    "photo",
    "key",
    "pin",
    "folder",
    "calendar",
    "clock",
    "bell.badge",
    "star",
    "terminal",
    "sparkles",
    "shippingbox",
    "brain",
    "safari",
];

/// The icon a channel is given, or an empty one for its default.
pub fn icon_arg(body: &Value) -> anyhow::Result<String> {
    let icon = body["icon"].as_str().unwrap_or_default().trim();
    anyhow::ensure!(icon.is_empty() || ICONS.contains(&icon), "Unknown channel picture.");
    Ok(icon.to_owned())
}

/// A memory note kept with a channel and given to the bot before each message from it.
pub fn memory_arg(body: &Value) -> anyhow::Result<String> {
    let memory = body["memory"].as_str().unwrap_or_default().trim();
    anyhow::ensure!(memory.chars().count() <= 4000, "Channel memory is at most 4,000 characters.");
    Ok(memory.to_owned())
}

/// `text` with the channel's memory in front, when it has one.
pub fn with_memory(memory: &str, text: &str) -> String {
    let memory = memory.trim();
    if memory.is_empty() { text.to_owned() } else { format!("[Channel memory]\n{memory}\n\n{text}") }
}

const NOTES_PER_CHANNEL: usize = 20;
const NOTE_CHARS: usize = 300;

fn notes_slot(provider: &str, channel: &str) -> String {
    format!("channel-notes:{provider}:{channel}")
}

/// What this channel taught the bot, in the order it was written.
pub fn notes(store: &crate::store::Store, provider: &str, channel: &str) -> Vec<String> {
    store
        .kv_read(&notes_slot(provider, channel))
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
}

fn save_notes(store: &crate::store::Store, provider: &str, channel: &str, notes: &[String]) -> anyhow::Result<()> {
    store.kv_set(&notes_slot(provider, channel), &serde_json::to_string(notes)?)
}

fn channel_arg(body: &Value) -> anyhow::Result<(String, String)> {
    let provider = body["provider"].as_str().unwrap_or_default();
    anyhow::ensure!(
        matches!(provider, "telegram" | "slack" | "discord"),
        "Channels with notes are Telegram, Slack and Discord."
    );
    let channel = body["channel"].as_str().unwrap_or_default().trim();
    anyhow::ensure!(!channel.is_empty() && channel.chars().count() <= 200, "Choose a channel first.");
    Ok((provider.to_owned(), channel.to_owned()))
}

/// `channelNotes`, `addChannelNote` and `removeChannelNote`: small facts a channel keeps, given to the bot with
/// its memory before every message.
pub fn call(hub: &std::sync::Arc<crate::hub::Hub>, method: &str, body: &Value) -> anyhow::Result<Value> {
    let (provider, channel) = channel_arg(body)?;
    let mut list = notes(&hub.store, &provider, &channel);
    match method {
        "channelNotes" => {}
        "addChannelNote" => {
            let text = body["text"].as_str().unwrap_or_default().trim();
            anyhow::ensure!(!text.is_empty(), "Write the note first.");
            anyhow::ensure!(text.chars().count() <= NOTE_CHARS, "A note is at most 300 characters.");
            anyhow::ensure!(list.len() < NOTES_PER_CHANNEL, "This channel has enough notes; remove one first.");
            anyhow::ensure!(!list.iter().any(|note| note == text), "That note is already here.");
            list.push(text.to_owned());
            save_notes(&hub.store, &provider, &channel, &list)?;
        }
        "removeChannelNote" => {
            let index =
                usize::try_from(body["index"].as_u64().context("index is required")?).context("Unknown note.")?;
            anyhow::ensure!(index < list.len(), "Unknown note.");
            list.remove(index);
            save_notes(&hub.store, &provider, &channel, &list)?;
        }
        other => anyhow::bail!("Unknown channel notes method `{other}`."),
    }
    Ok(serde_json::json!({"notes": list}))
}

/// `text` with the channel's memory and its notes in front, when it has either.
pub fn with_notes(memory: &str, notes: &[String], text: &str) -> String {
    if notes.is_empty() {
        return with_memory(memory, text);
    }
    let mut combined = memory.trim().to_owned();
    if !combined.is_empty() {
        combined.push_str("\n\n");
    }
    combined.push_str("[What this channel taught the bot]\n");
    for note in notes {
        combined.push_str("- ");
        combined.push_str(note);
        combined.push('\n');
    }
    with_memory(&combined, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_merged_before_the_message() {
        let notes = vec!["Ships on Fridays".to_owned()];
        let merged = with_notes("Keep replies short", &notes, "hola");
        assert!(merged.starts_with("[Channel memory]\nKeep replies short"));
        assert!(merged.contains("- Ships on Fridays"));
        assert!(merged.ends_with("hola"));
        assert_eq!(with_notes("", &[], "hola"), "hola");
    }

    #[test]
    fn the_memory_goes_in_front_of_the_message() {
        assert_eq!(with_memory("", "hi"), "hi");
        assert_eq!(with_memory("  ", "hi"), "hi");
        assert_eq!(with_memory("the shop project", "hi"), "[Channel memory]\nthe shop project\n\nhi");
    }

    #[test]
    fn attached_files_are_named_in_the_prompt() {
        let mut m = Incoming {
            channel: "C1".into(),
            person: "U1".into(),
            thread: "1.0".into(),
            text: "look".into(),
            files: vec![],
        };
        assert_eq!(m.prompt(), "look");
        m.files = vec![FileRef { name: "a.png".into() }, FileRef { name: "b.pdf".into() }];
        assert_eq!(m.prompt(), "look\n\n[Attached, not downloaded: a.png, b.pdf]");
    }
}
