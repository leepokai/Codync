//! The palette's "Reset saved credentials…": deletes the keys and tokens this computer's host saved, for one
//! whose key is gone (`resetCredentials`, the same as the desktop app's Settings, under Computers).

use super::super::l10n::{fill, tr};
use super::{App, Confirm, ConfirmAct, Overlay};

impl App {
    pub(super) fn ask_reset_credentials(&mut self) {
        let c = Confirm {
            title: fill(&tr("Reset saved credentials on {name}?"), &[("name", &self.host)]),
            detail: "Deletes agent and voice keys, connectors, apps and logins.".into(),
            note: "Agents signed in on their own stay signed in.".into(),
            button: "Reset",
            act: ConfirmAct::ResetCredentials,
        };
        self.overlays.push(Overlay::Confirm(c));
    }

    /// The second question, after Enter on the first.
    pub(super) fn ask_delete_credentials(&mut self) {
        let c = Confirm {
            title: fill(&tr("Delete saved credentials on {name} for good?"), &[("name", &self.host)]),
            detail: "This can't be undone.".into(),
            note: String::new(),
            button: "Delete saved credentials",
            act: ConfirmAct::DeleteCredentials,
        };
        self.overlays.push(Overlay::Confirm(c));
    }
}

#[cfg(test)]
mod tests {
    use crate::tui::app::{ConfirmAct, Overlay, test_app};
    use crossterm::event::{KeyCode, KeyEvent};

    #[tokio::test]
    async fn reset_asks_twice_and_esc_keeps_everything() {
        let mut app = test_app();
        let asks = |app: &crate::tui::app::App, act: fn(&ConfirmAct) -> bool| matches!(app.overlays.as_slice(), [Overlay::Confirm(c)] if act(&c.act));
        app.ask_reset_credentials();
        assert!(asks(&app, |a| matches!(a, ConfirmAct::ResetCredentials)));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(asks(&app, |a| matches!(a, ConfirmAct::DeleteCredentials)), "Enter asks a second time");
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.overlays.is_empty(), "nothing is sent without the second Enter");
    }

    #[tokio::test]
    async fn the_palette_offers_reset_only_where_the_host_does() {
        use crate::tui::app::{Action, Editor, Goto, GotoItem};
        let mut app = test_app();
        let offered = |app: &crate::tui::app::App| {
            let g = Goto { query: Editor::with("reset saved"), filter: 0, cursor: 0 };
            app.goto_items(&g).iter().any(|i| matches!(i, GotoItem::Action(Action::ResetCredentials)))
        };
        assert!(!offered(&app), "an older host or a cloud computer has none");
        app.credentials_reset = true;
        assert!(offered(&app));
    }
}
