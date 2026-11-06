//! The new bot dialog: a name, what it should help with, its agent and looks. Everything else takes the host's
//! defaults; ready-made bots are in Extensions (Bots tab).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::{After, App, COLORS, Editor, NewBot, NewBotField, Overlay, SHAPES, cycle};

const ORDER: [NewBotField; 6] = [
    NewBotField::Name,
    NewBotField::Purpose,
    NewBotField::Agent,
    NewBotField::Profile,
    NewBotField::Color,
    NewBotField::Shape,
];

impl App {
    pub(super) fn new_bot(&mut self) {
        // A random look, out of every color with every shape.
        let looks = COLORS.len() * SHAPES.len();
        let n = usize::try_from(crate::store::now_ms()).unwrap_or(0) % looks;
        if self.profiles.is_empty() {
            self.call("securityProfiles", json!({}), After::Profiles);
        }
        self.overlays.push(Overlay::NewBot(NewBot {
            name: Editor::default(),
            purpose: Editor::default(),
            backend: None,
            profile: None,
            color: n % COLORS.len(),
            shape: n / COLORS.len(),
            field: NewBotField::Name,
            error: None,
            saving: false,
        }));
    }

    pub(super) fn new_bot_key(&mut self, mut n: NewBot, k: KeyEvent) -> Option<Overlay> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let step =
            |f: NewBotField, d: isize| ORDER[cycle(ORDER.iter().position(|o| *o == f).unwrap_or(0), d, ORDER.len())];
        let d: isize = if k.code == KeyCode::Left { -1 } else { 1 };
        match (k.code, n.field) {
            (KeyCode::Esc, _) => return None,
            (KeyCode::Enter, _) => self.create_bot(&mut n),
            (KeyCode::Char('s'), _) if ctrl => self.create_bot(&mut n),
            (KeyCode::Tab | KeyCode::Down, f) => n.field = step(f, 1),
            (KeyCode::BackTab | KeyCode::Up, f) => n.field = step(f, -1),
            (KeyCode::Left | KeyCode::Right, NewBotField::Color) => n.color = cycle(n.color, d, COLORS.len()),
            (KeyCode::Left | KeyCode::Right, NewBotField::Shape) => n.shape = cycle(n.shape, d, SHAPES.len()),
            (KeyCode::Left | KeyCode::Right, NewBotField::Agent) => {
                n.backend = self.next_agent(n.backend.as_deref(), d);
            }
            (KeyCode::Left | KeyCode::Right, NewBotField::Profile) => {
                n.profile = self.next_profile(n.profile.as_deref(), d);
            }
            (_, NewBotField::Name) => {
                n.name.key(k);
            }
            (_, NewBotField::Purpose) => {
                n.purpose.key(k);
            }
            _ => {}
        }
        Some(Overlay::NewBot(n))
    }

    /// The agents a new bot can start on: the ones that run here and the ones Codync knows.
    pub(in crate::tui) fn new_bot_agents(&self) -> Vec<&Value> {
        self.backends.iter().filter(|b| b["available"] == true || b["curated"] == true).collect()
    }

    /// The profile `d` steps from `current`, through "the host's default" (`None`) and back.
    pub(super) fn next_profile(&self, current: Option<&str>, d: isize) -> Option<String> {
        let ids: Vec<Option<&str>> =
            std::iter::once(None).chain(self.profiles.iter().map(|p| p["id"].as_str())).collect();
        let at = ids.iter().position(|id| *id == current).unwrap_or(0);
        ids[cycle(at, d, ids.len())].map(str::to_owned)
    }

    /// A profile's title and one-sentence summary, from what the host sent.
    pub(in crate::tui) fn profile_text(&self, id: &str) -> Option<(String, String)> {
        let p = self.profiles.iter().find(|p| p["id"] == id)?;
        Some((p["title"].as_str()?.to_owned(), p["summary"].as_str()?.to_owned()))
    }

    /// The agent `d` steps from `current`, through "the host's pick" (`None`) and back.
    fn next_agent(&self, current: Option<&str>, d: isize) -> Option<String> {
        let ids: Vec<Option<&str>> =
            std::iter::once(None).chain(self.new_bot_agents().into_iter().map(|b| b["id"].as_str())).collect();
        let at = ids.iter().position(|id| *id == current).unwrap_or(0);
        ids[cycle(at, d, ids.len())].map(str::to_owned)
    }

    fn create_bot(&mut self, n: &mut NewBot) {
        if n.saving {
            return;
        }
        n.saving = true;
        n.error = None;
        self.call("createBot", n.body(), After::Saved);
    }
}

impl NewBot {
    /// A personal workspace, automatic approvals, notifications on. Connectors and skills are left unset, and the
    /// agent too unless one was picked: the host picks the agent and turns on every installed plugin.
    fn body(&self) -> Value {
        json!({
            "name": self.name.text.trim(),
            "description": self.purpose.text.trim(),
            "backend": self.backend,
            "cwd": "",
            "permission": "auto",
            "securityProfile": self.profile,
            "notify": true,
            "avatarColor": COLORS[self.color],
            "avatarShape": SHAPES[self.shape],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Msg, test_app};
    use super::*;

    fn top(app: &mut App) -> NewBot {
        let Some(Overlay::NewBot(n)) = app.overlays.pop() else { panic!("the new bot dialog is on top") };
        n
    }

    #[tokio::test]
    async fn typing_names_the_bot_and_arrows_change_its_looks() {
        let mut app = test_app();
        app.new_bot();
        let mut n = top(&mut app);
        assert!(n.name.text.is_empty(), "the host names an unnamed bot");
        assert_eq!(n.field, NewBotField::Name);
        let (color, shape) = (n.color, n.shape);
        for k in [KeyCode::Char('R'), KeyCode::Char('e'), KeyCode::Char('x'), KeyCode::Tab, KeyCode::Char('P')] {
            let Some(Overlay::NewBot(next)) = app.new_bot_key(n, KeyEvent::from(k)) else { panic!("stays open") };
            n = next;
        }
        assert_eq!((n.field, n.purpose.text.as_str()), (NewBotField::Purpose, "P"));
        // Purpose, then Agent, Profile, and Color.
        for k in [KeyCode::Tab, KeyCode::Tab, KeyCode::Tab, KeyCode::Right] {
            let Some(Overlay::NewBot(next)) = app.new_bot_key(n, KeyEvent::from(k)) else { panic!("stays open") };
            n = next;
        }
        assert_eq!(n.name.text, "Rex");
        assert_eq!((n.field, n.color), (NewBotField::Color, cycle(color, 1, COLORS.len())));
        for k in [KeyCode::Down, KeyCode::Left] {
            let Some(Overlay::NewBot(next)) = app.new_bot_key(n, KeyEvent::from(k)) else { panic!("stays open") };
            n = next;
        }
        assert_eq!((n.field, n.shape), (NewBotField::Shape, cycle(shape, -1, SHAPES.len())));
        let body = n.body();
        assert_eq!(body["name"], "Rex");
        assert_eq!(body["description"], "P");
        assert_eq!(body["avatarShape"], SHAPES[n.shape]);
        assert!(body["backend"].is_null(), "the host picks the agent");
        assert!(body["connectors"].is_null() && body["skills"].is_null(), "the host turns every one on");
        let Some(Overlay::NewBot(n)) = app.new_bot_key(n, KeyEvent::from(KeyCode::Enter)) else { panic!("saving") };
        assert!(n.saving);
    }

    #[tokio::test]
    async fn arrows_on_the_profile_step_through_the_hosts_profiles_and_back_to_its_default() {
        let mut app = test_app();
        app.profiles = vec![
            json!({"id": "readOnly", "title": "Read-only", "summary": "x"}),
            json!({"id": "full", "title": "Full access", "summary": "y"}),
        ];
        assert_eq!(app.next_profile(None, 1).as_deref(), Some("readOnly"));
        assert_eq!(app.next_profile(Some("readOnly"), 1).as_deref(), Some("full"));
        assert_eq!(app.next_profile(Some("full"), 1), None, "back to the host's default");
        assert_eq!(app.next_profile(None, -1).as_deref(), Some("full"));
        assert_eq!(app.profile_text("full"), Some(("Full access".into(), "y".into())));
        app.new_bot();
        let mut n = top(&mut app);
        n.profile = Some("readOnly".into());
        assert_eq!(n.body()["securityProfile"], "readOnly", "the choice goes to the host");
    }

    #[tokio::test]
    async fn arrows_on_the_agent_step_through_the_agents_that_can_run() {
        let mut app = test_app();
        app.backends = vec![
            json!({"id": "claude", "available": true, "curated": true}),
            json!({"id": "goose", "available": false, "curated": false}),
            json!({"id": "codex", "available": false, "curated": true}),
        ];
        assert_eq!(app.next_agent(None, 1).as_deref(), Some("claude"));
        assert_eq!(
            app.next_agent(Some("claude"), 1).as_deref(),
            Some("codex"),
            "an unknown agent that can't run is skipped"
        );
        assert_eq!(app.next_agent(Some("codex"), 1), None, "back to the host's pick");
        assert_eq!(app.next_agent(None, -1).as_deref(), Some("codex"));
    }

    #[tokio::test]
    async fn a_created_bot_opens_and_a_refusal_stays_in_the_dialog() {
        let mut app = test_app();
        app.new_bot();
        if let Some(Overlay::NewBot(n)) = app.overlays.last_mut() {
            n.saving = true;
        }
        app.on_msg(Msg::Reply(After::Saved, Err("No agent".into())));
        let n = top(&mut app);
        assert_eq!((n.saving, n.error.as_deref()), (false, Some("No agent")));
        app.overlays.push(Overlay::NewBot(n));
        app.on_msg(Msg::Reply(After::Saved, Ok(json!({"bot": {"id": "b", "name": "Rex"}}))));
        assert!(app.overlays.is_empty());
        assert_eq!(app.selected.as_deref(), Some("b"));
        assert!(app.typing);
    }
}
