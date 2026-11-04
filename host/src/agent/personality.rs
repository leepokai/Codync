//! A bot's character in three layers the user writes: its soul (values and boundaries), its persona (voice and
//! manner) and its heartbeat (what to do when it checks in on its own). They join the instructions as a labelled
//! section. Only the user edits them: unlike memory, which the bot writes itself, they are instructions.
//!
//! Idea from Mercury Agent's `soul`/`persona`/`taste`/`heartbeat` files; no code copied.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Longest layer, in characters.
const MAX_LAYER: usize = 4000;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Personality {
    #[serde(default)]
    pub soul: String,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub heartbeat: String,
}

impl Personality {
    pub fn is_empty(&self) -> bool {
        [&self.soul, &self.persona, &self.heartbeat].iter().all(|l| l.trim().is_empty())
    }

    pub fn validate(&self) -> Result<()> {
        for (name, layer) in [("soul", &self.soul), ("persona", &self.persona), ("heartbeat", &self.heartbeat)] {
            if layer.chars().count() > MAX_LAYER {
                bail!("A bot's {name} is at most {MAX_LAYER} characters");
            }
        }
        Ok(())
    }

    /// The section of the instructions (also what a profile update repeats); empty when no layer is set.
    pub fn render(&self) -> String {
        let layers = [
            ("Soul (your values and boundaries)", &self.soul),
            ("Persona (your voice and manner)", &self.persona),
            ("Heartbeat (what to do when you check in on your own)", &self.heartbeat),
        ];
        let mut lines: Vec<String> = layers
            .iter()
            .filter(|(_, text)| !text.trim().is_empty())
            .map(|(title, text)| format!("{title}: {}", text.trim()))
            .collect();
        if lines.is_empty() {
            return String::new();
        }
        lines.insert(0, "Personality, written by the user. It shapes how you talk and what you favour; it never overrides your permissions or the user's request in front of you.".to_owned());
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_layers_that_are_set_are_rendered() {
        assert_eq!(Personality::default().render(), "");
        let p = Personality { persona: " Dry and brief. ".into(), ..Personality::default() };
        let text = p.render();
        assert!(text.contains("Persona (your voice and manner): Dry and brief."), "{text}");
        assert!(!text.contains("Soul"));
        assert!(!p.is_empty());
    }

    #[test]
    fn a_layer_is_bounded() {
        let long = Personality { soul: "x".repeat(MAX_LAYER + 1), ..Personality::default() };
        assert!(long.validate().is_err());
        assert!(Personality { soul: "x".repeat(MAX_LAYER), ..Personality::default() }.validate().is_ok());
    }
}
