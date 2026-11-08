//! What every messaging channel (Slack, Telegram, Discord) hands the host and takes back, so the host's own code never
//! learns a provider's wire format. A provider module turns its events into [`Incoming`]; the rest is the host's.

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

#[cfg(test)]
mod tests {
    use super::*;

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
