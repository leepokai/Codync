//! The palette's "Computer name": the name every app shows for this computer (`setComputerName`); empty, its
//! hostname.

use super::super::manage::{Submit, name_fields};
use super::{App, Overlay};

impl App {
    pub(super) fn ask_computer_name(&mut self) {
        let note = "Leave it empty to use the hostname.".into();
        let fields = name_fields("Computer name", note, "Name", "Name", Some(self.host.as_str()), Submit::ComputerName);
        self.overlays.push(Overlay::Fields(Box::new(fields)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Editor, Overlay, next_request, recording_host};
    use crate::tui::net::Client;
    use crossterm::event::{KeyCode, KeyEvent};

    #[tokio::test]
    async fn the_palette_names_this_computer_and_empty_means_its_hostname() {
        let (base, mut requests) = recording_host().await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new(&base, Some("token")), tx, String::new());
        app.host = "mac-mini".into();
        app.ask_computer_name();
        let Some(Overlay::Fields(f)) = app.overlays.last_mut() else { panic!("the name sheet is open") };
        assert_eq!(f.inputs[0].ed.text, "mac-mini", "it opens on the name apps show now");
        f.inputs[0].ed = Editor::with(" Work Mac ");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let request = next_request(&mut requests).await;
        assert!(
            request.starts_with("POST /api/setComputerName") && request.contains(r#"{"name":"Work Mac"}"#),
            "{request}"
        );
        app.ask_computer_name();
        if let Some(Overlay::Fields(f)) = app.overlays.last_mut() {
            f.inputs[0].ed = Editor::default();
        }
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let request = next_request(&mut requests).await;
        assert!(request.contains(r#"{"name":null}"#), "empty goes back to the hostname: {request}");
    }
}
