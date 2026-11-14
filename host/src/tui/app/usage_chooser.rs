//! The usage sheet and its chooser: their keys, and the host's answer to a move.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::view::{usage_scroll_max, usage_services};
use super::{After, App, Overlay};

impl App {
    pub(super) fn usage_key(&mut self, scroll: usize, k: KeyEvent) -> Option<Overlay> {
        let (_, height) = crossterm::terminal::size().unwrap_or((80, 24));
        let max = usage_scroll_max(self, height);
        match k.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'U' | 'P') => None,
            KeyCode::Down | KeyCode::Char('j') => Some(Overlay::Usage((scroll + 1).min(max))),
            KeyCode::Up | KeyCode::Char('k') => Some(Overlay::Usage(scroll.saturating_sub(1))),
            KeyCode::PageDown | KeyCode::Char(' ') => Some(Overlay::Usage((scroll + 10).min(max))),
            KeyCode::PageUp => Some(Overlay::Usage(scroll.saturating_sub(10))),
            KeyCode::Char('s') if !usage_services(self).is_empty() => {
                self.overlays.push(Overlay::UsageChooser { row: 0, moving: false });
                Some(Overlay::Usage(scroll))
            }
            _ => Some(Overlay::Usage(scroll)),
        }
    }

    pub(super) fn usage_chooser_key(&mut self, row: usize, moving: bool, k: KeyEvent) -> Option<Overlay> {
        let services = usage_services(self);
        let last = services.len().saturating_sub(1);
        // The list can shrink under the cursor: an agent turned off, a bot deleted on another device.
        let row = row.min(last);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q' | 's') => None,
            KeyCode::Down | KeyCode::Char('j') => Some(Overlay::UsageChooser { row: (row + 1).min(last), moving }),
            KeyCode::Up | KeyCode::Char('k') => Some(Overlay::UsageChooser { row: row.saturating_sub(1), moving }),
            KeyCode::Char(key @ ('J' | 'K')) => {
                let to = if key == 'J' { (row + 1).min(last) } else { row.saturating_sub(1) };
                let starts = !moving && to != row;
                if starts {
                    let mut ids: Vec<_> = services.iter().map(|service| &service["id"]).collect();
                    ids.swap(row, to);
                    self.call("setUsageOrder", json!({"ids": ids}), After::UsageOrder(to));
                }
                Some(Overlay::UsageChooser { row, moving: moving || starts })
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(service) = services.get(row) {
                    let shown = service["shown"].as_bool() != Some(true);
                    self.call("setUsageShown", json!({"id": service["id"], "shown": shown}), After::Nothing);
                }
                Some(Overlay::UsageChooser { row, moving })
            }
            _ => Some(Overlay::UsageChooser { row, moving }),
        }
    }

    /// The host's answer to a move. Only now do the list change and the cursor follow, so the next move starts from
    /// the order this one left, and a refused one leaves both where they were.
    pub(super) fn usage_order_answered(&mut self, to: usize, answer: Result<Value, String>) {
        let moved = match answer {
            Ok(usage) => {
                self.usage = usage;
                true
            }
            Err(error) => {
                self.flash(&error);
                false
            }
        };
        // Only the chooser that waits for a move: this one may have been closed and another opened since.
        // ponytail: a move started in that other one takes this answer as its own; give moves an id if that ever shows.
        if let Some(Overlay::UsageChooser { row, moving }) = self.overlays.last_mut()
            && *moving
        {
            *moving = false;
            if moved {
                *row = to;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::net::Client;
    use super::super::{After, App, Msg, Overlay, test_app};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::json;

    /// A host that hands each request it gets to `requests`, answering them all alike.
    async fn host() -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let base = format!("http://{}", listener.local_addr().expect("its address"));
        let (seen, requests) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = Vec::new();
                while !request.ends_with(b"}") {
                    let mut chunk = [0; 4096];
                    let read = socket.read(&mut chunk).await.expect("a request");
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..read]);
                }
                let _ = seen.send(String::from_utf8_lossy(&request).into_owned());
                let answer = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";
                let _ = socket.write_all(answer).await;
            }
        });
        (base, requests)
    }

    async fn next_request(requests: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(10), requests.recv())
            .await
            .expect("the host hears of it in time")
            .expect("the host is up")
    }

    #[tokio::test]
    async fn the_usage_sheet_opens_the_chooser_and_a_toggle_reaches_the_host() {
        let (base, mut requests) = host().await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new(&base, Some("token")), tx, String::new());
        app.usage = json!({"providers": [], "services": [
            {"id": "claude", "name": "Claude", "shown": true},
            {"id": "cursor", "name": "Cursor", "shown": false},
        ]});
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        app.overlays.push(Overlay::Usage(0));
        app.overlay_key(key(KeyCode::Char('s')));
        assert!(matches!(app.overlays.last(), Some(Overlay::UsageChooser { row: 0, .. })));
        app.overlay_key(key(KeyCode::Down));
        assert!(matches!(app.overlays.last(), Some(Overlay::UsageChooser { row: 1, .. })));
        app.overlay_key(key(KeyCode::Char(' ')));
        let request = next_request(&mut requests).await;
        assert!(request.starts_with("POST /api/setUsageShown"), "{request}");
        assert!(request.contains(r#""id":"cursor""#) && request.contains(r#""shown":true"#), "{request}");
        assert!(matches!(app.overlays.last(), Some(Overlay::UsageChooser { row: 1, .. })), "the chooser stays open");
        app.overlay_key(key(KeyCode::Esc));
        assert!(matches!(app.overlays.last(), Some(Overlay::Usage(0))), "back to the limits");
    }

    #[tokio::test]
    async fn j_and_k_move_the_selected_agent_as_the_host_answers() {
        let (base, mut requests) = host().await;
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(Client::new(&base, Some("token")), tx, String::new());
        let usage = |ids: &[&str]| {
            let services: Vec<_> = ids.iter().map(|id| json!({"id": id, "name": id, "shown": true})).collect();
            json!({"providers": [], "services": services})
        };
        let press = |app: &mut App, c| app.overlay_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        let row = |app: &App| match app.overlays.last() {
            Some(Overlay::UsageChooser { row, .. }) => *row,
            other => panic!("the chooser is closed: {:?}", other.is_some()),
        };
        app.usage = usage(&["a", "b", "c"]);
        app.overlays.push(Overlay::UsageChooser { row: 0, moving: false });

        press(&mut app, 'K');
        press(&mut app, 'J');
        press(&mut app, 'J');
        assert_eq!(row(&app), 0, "the cursor waits for the host");
        let sent = next_request(&mut requests).await;
        assert!(sent.starts_with("POST /api/setUsageOrder"), "{sent}");
        assert!(sent.contains(r#""ids":["b","a","c"]"#), "the top's K and the waiting J sent nothing: {sent}");
        app.on_msg(Msg::Reply(After::UsageOrder(1), Ok(usage(&["b", "a", "c"]))));
        assert_eq!(row(&app), 1, "the cursor follows the agent");

        press(&mut app, 'J');
        assert!(next_request(&mut requests).await.contains(r#""ids":["b","c","a"]"#));
        app.on_msg(Msg::Reply(After::UsageOrder(2), Ok(usage(&["b", "c", "a"]))));
        press(&mut app, 'J');
        assert_eq!(row(&app), 2, "the end of the list does nothing");

        press(&mut app, 'K');
        assert!(next_request(&mut requests).await.contains(r#""ids":["b","a","c"]"#), "the last J sent nothing");
        app.on_msg(Msg::Reply(After::UsageOrder(1), Err("unknown method".into())));
        assert_eq!(row(&app), 2, "a refused move keeps the cursor on its agent");

        // An agent turned off leaves the cursor past the end.
        app.usage = usage(&["b", "c"]);
        press(&mut app, 'K');
        assert!(next_request(&mut requests).await.contains(r#""ids":["c","b"]"#));
    }

    #[tokio::test]
    async fn a_chooser_opened_after_a_move_was_sent_does_not_take_its_answer() {
        let mut app = test_app();
        app.overlays.push(Overlay::UsageChooser { row: 0, moving: false });
        app.on_msg(Msg::Reply(After::UsageOrder(1), Ok(json!({"providers": []}))));
        assert!(matches!(app.overlays.last(), Some(Overlay::UsageChooser { row: 0, moving: false })));
    }

    #[tokio::test]
    async fn a_host_with_no_list_offers_no_chooser() {
        let mut app = test_app();
        app.usage = json!({"providers": []});
        app.overlays.push(Overlay::Usage(0));
        app.overlay_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert!(matches!(app.overlays.last(), Some(Overlay::Usage(0))));
    }
}
