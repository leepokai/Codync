//! The belief header of a note: what kind of thing it says, how sure Codync is of it, and when the person last
//! confirmed it. Three optional lines in the note's frontmatter, so the header is plain text a person can edit and
//! every older note, which has none, reads as a medium-confidence fact about the user:
//!
//! ```text
//! ---
//! description: Where the user lives and works
//! type: user
//! confidence: high
//! confirmed: 2026-10-30
//! ---
//! ```

use serde::Serialize;

/// What a note is about, in the vocabulary of a bot's memory: the user, how they want the bot to work, a project, or
/// where to look something up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    User,
    Feedback,
    Project,
    Reference,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "user" => Some(Self::User),
            "feedback" => Some(Self::Feedback),
            "project" => Some(Self::Project),
            "reference" => Some(Self::Reference),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Feedback => "feedback",
            Self::Project => "project",
            Self::Reference => "reference",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    #[default]
    Medium,
    Low,
}

impl Confidence {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "high" => Some(Self::High),
            "medium" => Some(Self::Medium),
            "low" => Some(Self::Low),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Belief {
    pub kind: Kind,
    pub confidence: Confidence,
    /// `YYYY-MM-DD` of the last time the person said it still holds.
    pub confirmed: Option<String>,
}

const KEYS: [&str; 3] = ["type:", "confidence:", "confirmed:"];

/// The frontmatter lines of `content` and where they end, when it starts with one.
fn frontmatter(content: &str) -> Option<(Vec<&str>, usize)> {
    let body = content.trim_start_matches('\u{feff}');
    let skipped = content.len() - body.len();
    let rest = body.strip_prefix("---")?;
    let rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'))?;
    let start = skipped + body.len() - rest.len();
    let mut lines = Vec::new();
    let mut at = start;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']).trim() == "---" {
            return Some((lines, at));
        }
        lines.push(line);
        at += line.len();
    }
    None
}

/// The belief a note's header states; lines that are missing or make no sense read as the defaults.
pub fn read(content: &str) -> Belief {
    let mut belief = Belief::default();
    let Some((lines, _)) = frontmatter(content) else { return belief };
    for line in lines {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("type:") {
            belief.kind = Kind::parse(value.trim_matches(['"', '\'', ' '])).unwrap_or_default();
        } else if let Some(value) = line.strip_prefix("confidence:") {
            belief.confidence = Confidence::parse(value.trim_matches(['"', '\'', ' '])).unwrap_or_default();
        } else if let Some(value) = line.strip_prefix("confirmed:") {
            let value = value.trim().trim_matches(['"', '\'']);
            belief.confirmed = valid_date(value).then(|| value.to_owned());
        }
    }
    belief
}

fn valid_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}

/// `content` with the header lines set to `belief`, everything else in it untouched. A note without frontmatter has
/// no place for a header and comes back as it is.
pub fn stamp(content: &str, belief: &Belief) -> String {
    let Some((lines, end)) = frontmatter(content) else { return content.to_owned() };
    let eol = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let start = content.len() - content.trim_start_matches('\u{feff}').len();
    let opening = content[start..].find('\n').map_or(start, |i| start + i + 1);
    let mut out = String::with_capacity(content.len() + 64);
    out.push_str(&content[..opening]);
    for line in lines {
        if !KEYS.iter().any(|key| line.trim_start().starts_with(key)) {
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push_str(eol);
            }
        }
    }
    out.push_str(&format!("type: {}{eol}confidence: {}{eol}", belief.kind.as_str(), belief.confidence.as_str()));
    if let Some(date) = &belief.confirmed {
        out.push_str(&format!("confirmed: {date}{eol}"));
    }
    out.push_str(&content[end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ndescription: Where the user lives\n---\nLives in Lisbon.\n";

    #[test]
    fn a_note_without_a_header_reads_as_a_medium_fact_about_the_user() {
        assert_eq!(read(NOTE), Belief { kind: Kind::User, confidence: Confidence::Medium, confirmed: None });
        assert_eq!(read("no frontmatter at all"), Belief::default());
    }

    #[test]
    fn the_header_is_read_as_written() {
        let note = "---\ndescription: x\ntype: Project\nconfidence: \"low\"\nconfirmed: 2026-10-30\n---\nbody\n";
        let belief = read(note);
        assert_eq!((belief.kind, belief.confidence, belief.confirmed.as_deref()), (Kind::Project, Confidence::Low, Some("2026-10-30")));
    }

    #[test]
    fn nonsense_in_the_header_falls_back_to_the_defaults() {
        let note = "---\ndescription: x\ntype: banana\nconfidence: certain\nconfirmed: yesterday\n---\nbody\n";
        assert_eq!(read(note), Belief::default());
    }

    #[test]
    fn stamping_sets_the_header_and_keeps_the_rest_of_the_note() {
        let belief = Belief { kind: Kind::Feedback, confidence: Confidence::High, confirmed: Some("2026-11-01".into()) };
        let stamped = stamp(NOTE, &belief);
        assert_eq!(
            stamped,
            "---\ndescription: Where the user lives\ntype: feedback\nconfidence: high\nconfirmed: 2026-11-01\n---\nLives in Lisbon.\n"
        );
        assert_eq!(read(&stamped), belief);
        // Stamping again replaces the lines instead of repeating them.
        let again = stamp(&stamped, &Belief { confidence: Confidence::Low, ..belief.clone() });
        assert_eq!(again.matches("confidence:").count(), 1);
        assert_eq!(read(&again).confidence, Confidence::Low);
    }

    #[test]
    fn stamping_leaves_a_note_with_no_frontmatter_and_windows_line_ends_alone() {
        assert_eq!(stamp("just text", &Belief::default()), "just text");
        let crlf = "---\r\ndescription: x\r\n---\r\nbody\r\n";
        let stamped = stamp(crlf, &Belief::default());
        assert!(stamped.contains("type: user\r\nconfidence: medium\r\n---\r\nbody"), "{stamped:?}");
    }
}
