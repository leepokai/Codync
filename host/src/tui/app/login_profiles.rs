//! The bot form's Login profile field: which of the agent's logins the bot runs on (`agent::login_profiles`), the one the
//! computer already has or one of its other login profiles. ←/→ steps through them; ↵ opens the agent's sheet, where a
//! login profile is added, signed in (or given its keys) and removed. A host without login profiles has no such field.

use serde_json::Value;

use super::{App, FIELDS, Field, Form, cycle};
use crate::tui::l10n::tr;

impl App {
    /// Agent `backend`'s other login profiles; `None` for an agent that can't have any.
    pub(in crate::tui) fn login_profiles(&self, backend: &str) -> Option<&[Value]> {
        self.backends.iter().find(|b| b["id"] == backend)?["loginProfiles"].as_array().map(Vec::as_slice)
    }

    /// What the user calls agent `backend`'s default login profile, the login the computer already has.
    pub(in crate::tui) fn default_login_profile_name(&self, backend: &str) -> String {
        let own =
            self.backends.iter().find(|b| b["id"] == backend).and_then(|b| b["defaultLoginProfile"]["name"].as_str());
        own.map_or_else(|| tr("Default login profile").into_owned(), str::to_owned)
    }

    /// The name of agent `backend`'s login profile `id`.
    pub(in crate::tui) fn login_profile_name(&self, backend: &str, id: &str) -> String {
        let login_profile = self.login_profiles(backend).unwrap_or_default().iter().find(|a| a["id"] == id);
        login_profile
            .and_then(|a| a["name"].as_str())
            .map_or_else(|| tr("New login profile").into_owned(), str::to_owned)
    }

    /// The field `d` steps from the focused one, past the Login profile field of an agent without login profiles.
    pub(super) fn step_field(&self, f: &Form, d: isize) -> usize {
        let next = cycle(f.field, d, FIELDS.len());
        if FIELDS[next] == Field::LoginProfile && self.login_profiles(&f.backend).is_none() {
            cycle(next, d, FIELDS.len())
        } else {
            next
        }
    }

    /// What the field steps through: the default login profile (`None`), then the login profiles a bot can run on. One
    /// that isn't signed in can't run a bot; the bot's own stays listed so the field shows it.
    fn login_profile_choices(&self, f: &Form) -> Vec<Option<String>> {
        let login_profiles = self.login_profiles(&f.backend).unwrap_or_default().iter();
        let usable = login_profiles.filter(|a| a["signedIn"] == true || a["id"].as_str() == f.login_profile.as_deref());
        std::iter::once(None).chain(usable.map(|a| a["id"].as_str().map(str::to_owned))).collect()
    }

    pub(super) fn cycle_login_profile(&self, f: &mut Form, d: isize) {
        let choices = self.login_profile_choices(f);
        let i = choices.iter().position(|c| *c == f.login_profile).unwrap_or(0);
        f.login_profile.clone_from(&choices[cycle(i, d, choices.len())]);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Bot, Overlay, test_app};
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent};
    use serde_json::json;

    /// The editor of bot `bot`, on `field`.
    fn editor(app: &mut App, bot: &Value, field: Field) -> Box<Form> {
        app.bots.insert("b".into(), Bot::parse(bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        f.field = FIELDS.iter().position(|x| *x == field).expect("the field");
        f
    }

    fn press(app: &mut App, f: Box<Form>, key: KeyCode) -> Box<Form> {
        let Some(Overlay::Form(f)) = app.form_key(f, KeyEvent::from(key)) else { panic!("the editor stays") };
        f
    }

    fn agents() -> Vec<Value> {
        let login_profiles = json!([
            {"id": "a1", "name": "ada@example.com", "signedIn": true},
            {"id": "a2", "name": null, "signedIn": false},
            {"id": "a3", "name": "old@example.com", "signedIn": false},
        ]);
        vec![
            json!({"id": "claude", "name": "Claude Code", "available": true, "installed": true, "loginProfiles": login_profiles}),
            json!({"id": "gemini", "name": "Gemini", "available": true, "installed": true}),
            json!({"id": "codex", "name": "Codex", "available": true, "installed": true, "loginProfiles": [], "defaultLoginProfile": {"name": "Work"}}),
        ]
    }

    #[tokio::test]
    async fn the_login_profile_field_steps_through_the_ones_a_bot_can_run_on() {
        let mut app = test_app();
        app.backends = agents();
        let bot = json!({"id": "b", "name": "B", "backend": "claude", "loginProfile": "a3"});
        let f = editor(&mut app, &bot, Field::LoginProfile);
        assert_eq!(f.changes(), json!({}));
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!(
            f.changes(),
            json!({"loginProfile": null}),
            "back to the computer's own sign-in, as an explicit null"
        );
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!(f.changes(), json!({"loginProfile": "a1"}), "then the signed-in ones");
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!(f.login_profile, None, "a signed-out one shows only while it is the bot's");
        let f = press(&mut app, f, KeyCode::Left);
        assert_eq!(f.login_profile.as_deref(), Some("a1"));
    }

    #[tokio::test]
    async fn changing_the_agent_clears_the_login_profile() {
        let mut app = test_app();
        app.backends = agents();
        let bot = json!({"id": "b", "name": "B", "backend": "claude", "loginProfile": "a1",
            "fallbacks": [{"backend": "gemini"}, {"backend": "gemini", "loginProfile": "g1"}]});
        let f = editor(&mut app, &bot, Field::Agent);
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!((f.backend.as_str(), f.login_profile.as_deref()), ("gemini", None));
        let kept: Vec<_> = f.fallbacks.iter().map(|x| x.login_profile.as_deref()).collect();
        assert_eq!(kept, [Some("g1")], "only the step on the new agent's default login goes");
        assert_eq!(f.changes()["loginProfile"], Value::Null);
        // Gemini has no login profiles: the field is skipped both ways.
        let f = press(&mut app, f, KeyCode::Tab);
        assert_eq!(f.current(), Field::Model);
        let f = press(&mut app, f, KeyCode::BackTab);
        assert_eq!(f.current(), Field::Agent);
        let f = press(&mut app, f, KeyCode::Left);
        let f = press(&mut app, f, KeyCode::Tab);
        assert_eq!((f.backend.as_str(), f.current()), ("claude", Field::LoginProfile));
    }

    #[tokio::test]
    async fn the_login_profile_field_shows_only_for_an_agent_with_login_profiles() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut app = test_app();
        app.backends = agents();
        let mut drawn = |bot: Value| {
            let f = editor(&mut app, &bot, Field::Name);
            app.overlays.push(Overlay::Form(f));
            let mut terminal = Terminal::new(TestBackend::new(100, 60)).expect("terminal");
            terminal.draw(|frame| crate::tui::view::draw(frame, &mut app)).expect("draw the form");
            app.overlays.clear();
            terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>()
        };
        assert!(drawn(json!({"id": "b", "name": "B", "backend": "claude"})).contains("Default login profile"));
        let signed_out = drawn(json!({"id": "b", "name": "B", "backend": "claude", "loginProfile": "a2"}));
        assert!(signed_out.contains("New login profile") && signed_out.contains("Not signed in"), "{signed_out}");
        assert!(!drawn(json!({"id": "b", "name": "B", "backend": "gemini"})).contains("Default login profile"));
        assert!(drawn(json!({"id": "b", "name": "B", "backend": "codex"})).contains("‹ Work ›"), "named by the user");
    }
}
