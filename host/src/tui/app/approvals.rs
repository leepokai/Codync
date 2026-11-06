//! The bot form's Approvals: the agent's modes, its Auto mode first (`agent::modes`), then Codync approving
//! everything. An agent without modes offers Ask me instead; one whose list hasn't come yet, Auto and Ask me. A bot
//! without a mode (or in Auto on an agent without one) shows as Ask me, and one in a mode the list lacks as that
//! mode's id: never as more careful than it is.

use super::{App, Form, cycle};
use crate::agent::modes::{AUTO, Mode};

const ASKS: &str = "Anything the agent still asks about comes to you as an approval card.";

/// One Approvals choice: the mode to work in and whether Codync approves what still asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub mode: Option<String>,
    pub auto: bool,
    pub label: String,
    /// The agent's own words for its mode, untranslated.
    pub note: Option<String>,
    /// What Codync does, to translate.
    pub detail: &'static str,
}

impl Approval {
    fn new(mode: Option<&str>, auto: bool, label: &str, detail: &'static str) -> Self {
        Self { mode: mode.map(str::to_owned), auto, label: label.to_owned(), note: None, detail }
    }

    fn ask() -> Self {
        Self::new(None, false, "Ask me", "You'll get an approval card and a notification whenever the agent asks.")
    }
}

/// The modes an agent's model list came with (`agentModels`), Auto first and listed as `auto`.
pub fn parse_modes(v: &serde_json::Value) -> Vec<Mode> {
    serde_json::from_value(v["modes"].clone()).unwrap_or_default()
}

impl App {
    /// What the form offers for its agent, and which one its setting shows as.
    pub fn approvals(&self, f: &Form) -> (Vec<Approval>, usize) {
        let mut choices = match self.modes.get(&f.backend) {
            None => vec![
                Approval::new(Some(AUTO), false, "Auto", "The agent decides what's safe and asks you about the rest."),
                Approval::ask(),
            ],
            Some(modes) if modes.is_empty() => vec![Approval::ask()],
            Some(modes) => modes
                .iter()
                .map(|m| Approval { note: m.description.clone(), ..Approval::new(Some(&m.id), false, &m.name, ASKS) })
                .collect(),
        };
        choices.push(Approval::new(
            None,
            true,
            "Approve automatically",
            "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply.",
        ));
        let position = |choices: &[Approval], mode: Option<&str>| {
            choices.iter().position(|c| !c.auto && c.mode.as_deref() == mode)
        };
        // Unlisted: Auto on an agent without it is its default, like no mode (Ask me); any other mode is itself.
        let unlisted = match f.mode.as_deref() {
            Some(id) if id != AUTO => Approval::new(Some(id), false, id, ASKS),
            _ => Approval::ask(),
        };
        let found = position(&choices, f.mode.as_deref()).or_else(|| position(&choices, unlisted.mode.as_deref()));
        let selected = match found {
            _ if f.auto => choices.len() - 1,
            Some(i) => i,
            None => {
                choices.insert(choices.len() - 1, unlisted);
                choices.len() - 2
            }
        };
        (choices, selected)
    }

    /// The choice the form's setting shows as.
    pub fn approval(&self, f: &Form) -> Approval {
        let (mut choices, selected) = self.approvals(f);
        choices.swap_remove(selected)
    }

    /// What a bot's mode is called on its agent, as far as the list that came says.
    pub fn mode_name<'a>(&'a self, backend: &str, id: &'a str) -> &'a str {
        let listed = self.modes.get(backend).and_then(|modes| modes.iter().find(|m| m.id == id));
        listed.map_or(id, |m| m.name.as_str())
    }

    pub(super) fn cycle_approval(&self, f: &mut Form, d: isize) {
        let (choices, selected) = self.approvals(f);
        let next = &choices[cycle(selected, d, choices.len())];
        (f.mode, f.auto) = (next.mode.clone(), next.auto);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Overlay, test_app};
    use super::*;
    use serde_json::{Value, json};

    fn form(app: &mut App, bot: &Value) -> Form {
        app.bots.insert("b".into(), super::super::Bot::parse(bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(f)) = app.overlays.pop() else { panic!("the editor opens") };
        *f
    }

    fn labels(app: &App, f: &Form) -> Vec<String> {
        app.approvals(f).0.into_iter().map(|c| c.label).collect()
    }

    #[tokio::test]
    async fn approvals_list_the_agents_modes_then_approve_automatically() {
        let mut app = test_app();
        let codex = json!({"modes": [{"id": "auto", "name": "Auto review", "description": "Only ask for unsafe actions"},
            {"id": "read-only", "name": "Read-only"}]});
        app.modes.insert("codex".into(), parse_modes(&codex));
        let mut f = form(&mut app, &json!({"id": "b", "backend": "codex", "permission": "ask", "mode": "auto"}));
        let auto = app.approval(&f);
        assert_eq!((auto.label.as_str(), auto.note.as_deref()), ("Auto review", Some("Only ask for unsafe actions")));
        assert_eq!(labels(&app, &f), ["Auto review", "Read-only", "Approve automatically"]);
        assert_eq!(app.mode_name("codex", "read-only"), "Read-only");
        app.cycle_approval(&mut f, 1);
        assert_eq!((f.mode.as_deref(), f.auto), (Some("read-only"), false));
        app.cycle_approval(&mut f, 1);
        assert_eq!((f.mode.as_deref(), f.auto), (None, true));
        assert_eq!(f.changes(), json!({"permission": "auto", "mode": null}));

        // A bot without a mode is the agent's default, and one in a mode the list lacks shows it.
        f.auto = false;
        assert_eq!(app.approval(&f).label, "Ask me");
        f.mode = Some("bypassPermissions".into());
        assert_eq!(labels(&app, &f), ["Auto review", "Read-only", "bypassPermissions", "Approve automatically"]);
        assert_eq!(app.approval(&f).label, "bypassPermissions");

        // An agent without modes asks about everything; one whose list hasn't come yet offers Auto too.
        app.modes.insert("gemini".into(), vec![]);
        let f = form(&mut app, &json!({"id": "b", "backend": "gemini", "permission": "ask", "mode": "auto"}));
        assert_eq!(labels(&app, &f), ["Ask me", "Approve automatically"]);
        assert_eq!(app.approval(&f).label, "Ask me");
        let f = form(&mut app, &json!({"id": "b", "backend": "other", "permission": "ask", "mode": "plan"}));
        assert_eq!(labels(&app, &f), ["Auto", "Ask me", "plan", "Approve automatically"]);
    }
}
