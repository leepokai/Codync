//! The bot form's Account field: which of the agent's logins the bot runs on (`agent::accounts`), the one the
//! computer already has or one of its other accounts. ←/→ steps through them; ↵ opens the agent's sheet, where an
//! account is added, signed in and removed. An agent that can't have accounts has no such field.

use serde_json::Value;

use super::{App, FIELDS, Field, Form, cycle};

impl App {
    /// Agent `backend`'s other accounts; `None` for an agent that can't have any.
    pub(in crate::tui) fn accounts(&self, backend: &str) -> Option<&[Value]> {
        self.backends.iter().find(|b| b["id"] == backend)?["accounts"].as_array().map(Vec::as_slice)
    }

    /// The field `d` steps from the focused one, past the Account field of an agent without accounts.
    pub(super) fn step_field(&self, f: &Form, d: isize) -> usize {
        let next = cycle(f.field, d, FIELDS.len());
        if FIELDS[next] == Field::Account && self.accounts(&f.backend).is_none() {
            cycle(next, d, FIELDS.len())
        } else {
            next
        }
    }

    /// What the field steps through: the computer's own sign-in (`None`), then the accounts a bot can run on. One
    /// that isn't signed in can't run a bot; the bot's own stays listed so the field shows it.
    fn account_choices(&self, f: &Form) -> Vec<Option<String>> {
        let accounts = self.accounts(&f.backend).unwrap_or_default().iter();
        let usable = accounts.filter(|a| a["signedIn"] == true || a["id"].as_str() == f.account.as_deref());
        std::iter::once(None).chain(usable.map(|a| a["id"].as_str().map(str::to_owned))).collect()
    }

    pub(super) fn cycle_account(&self, f: &mut Form, d: isize) {
        let choices = self.account_choices(f);
        let i = choices.iter().position(|c| *c == f.account).unwrap_or(0);
        f.account.clone_from(&choices[cycle(i, d, choices.len())]);
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
        let accounts = json!([
            {"id": "a1", "name": "ada@example.com", "signedIn": true},
            {"id": "a2", "name": null, "signedIn": false},
            {"id": "a3", "name": "old@example.com", "signedIn": false},
        ]);
        vec![
            json!({"id": "claude", "name": "Claude Code", "available": true, "installed": true, "accounts": accounts}),
            json!({"id": "gemini", "name": "Gemini", "available": true, "installed": true}),
        ]
    }

    #[tokio::test]
    async fn the_account_field_steps_through_the_accounts_a_bot_can_run_on() {
        let mut app = test_app();
        app.backends = agents();
        let bot = json!({"id": "b", "name": "B", "backend": "claude", "account": "a3"});
        let f = editor(&mut app, &bot, Field::Account);
        assert_eq!(f.changes(), json!({}));
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!(f.changes(), json!({"account": null}), "back to the computer's own sign-in, as an explicit null");
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!(f.changes(), json!({"account": "a1"}), "then the signed-in ones");
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!(f.account, None, "a signed-out one shows only while it is the bot's");
        let f = press(&mut app, f, KeyCode::Left);
        assert_eq!(f.account.as_deref(), Some("a1"));
    }

    #[tokio::test]
    async fn changing_the_agent_clears_the_account() {
        let mut app = test_app();
        app.backends = agents();
        let bot = json!({"id": "b", "name": "B", "backend": "claude", "account": "a1"});
        let f = editor(&mut app, &bot, Field::Agent);
        let f = press(&mut app, f, KeyCode::Right);
        assert_eq!((f.backend.as_str(), f.account.as_deref()), ("gemini", None));
        assert_eq!(f.changes()["account"], Value::Null);
        // Gemini has no accounts: the field is skipped both ways.
        let f = press(&mut app, f, KeyCode::Tab);
        assert_eq!(f.current(), Field::Model);
        let f = press(&mut app, f, KeyCode::BackTab);
        assert_eq!(f.current(), Field::Agent);
        let f = press(&mut app, f, KeyCode::Left);
        let f = press(&mut app, f, KeyCode::Tab);
        assert_eq!((f.backend.as_str(), f.current()), ("claude", Field::Account));
    }

    #[tokio::test]
    async fn the_account_field_shows_only_for_an_agent_with_accounts() {
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
        assert!(drawn(json!({"id": "b", "name": "B", "backend": "claude"})).contains("This computer's sign-in"));
        let signed_out = drawn(json!({"id": "b", "name": "B", "backend": "claude", "account": "a2"}));
        assert!(signed_out.contains("New account") && signed_out.contains("Not signed in"), "{signed_out}");
        assert!(!drawn(json!({"id": "b", "name": "B", "backend": "gemini"})).contains("This computer's sign-in"));
    }
}
