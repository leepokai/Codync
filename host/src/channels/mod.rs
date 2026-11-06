//! What every messaging channel (Slack, Telegram) hands the host and takes back, so the host's own code never
//! learns a provider's wire format. A provider module turns its events into [`Incoming`]; the rest is the host's.

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

#[cfg(test)]
mod tests {
    use super::*;

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
