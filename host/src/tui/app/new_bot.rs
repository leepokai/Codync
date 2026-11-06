//! The new bot dialog: a name and looks. The agent and everything else take the host's defaults.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::{After, App, COLORS, Editor, NewBot, NewBotField, Overlay, SHAPES, cycle};

const ORDER: [NewBotField; 3] = [NewBotField::Name, NewBotField::Color, NewBotField::Shape];

impl App {
    pub(super) fn new_bot(&mut self) {
        // A random look, out of every color with every shape.
        let looks = COLORS.len() * SHAPES.len();
        let n = usize::try_from(crate::store::now_ms()).unwrap_or(0) % looks;
        self.overlays.push(Overlay::NewBot(NewBot {
            name: Editor::default(),
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
            (_, NewBotField::Name) => {
                n.name.key(k);
            }
            _ => {}
        }
        Some(Overlay::NewBot(n))
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
    /// A personal workspace, notifications on. The agent, approvals, connectors and skills are left unset: the
    /// host picks the agent, its Auto mode and every installed plugin.
    fn body(&self) -> Value {
        json!({
            "name": self.name.text.trim(),
            "cwd": "",
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
        for k in [KeyCode::Char('R'), KeyCode::Char('e'), KeyCode::Char('x'), KeyCode::Tab, KeyCode::Right] {
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
        assert_eq!(body["avatarShape"], SHAPES[n.shape]);
        assert!(body["backend"].is_null(), "the host picks the agent");
        assert!(body["connectors"].is_null() && body["skills"].is_null(), "the host turns every one on");
        let Some(Overlay::NewBot(n)) = app.new_bot_key(n, KeyEvent::from(KeyCode::Enter)) else { panic!("saving") };
        assert!(n.saving);
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
