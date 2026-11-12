//! Read marks: reading the chat on screen, and `.`, the roster's "Mark as read" and "Mark as unread". The host
//! decides what a read does to a chat marked unread.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde_json::{Value, json};
use tokio::task::JoinHandle;

use super::bot::Bot;
use super::{After, App};

/// The first host with `markUnread`.
const MARK_UNREAD_SINCE: &str = "2.47.0";

/// The read state behind this screen's marks.
#[derive(Default)]
pub(super) struct ReadMarks {
    /// Chats marked read since their bot's unread count last changed.
    pub reading: HashSet<String>,
    /// The user picked the selected chat since the last tick: its read is theirs (`read_due`).
    pub chat_opened: bool,
    /// Each chat's last mark or message on its way, retries included: the next waits for it, so the host gets them
    /// in the order they were made.
    last: HashMap<String, JoinHandle<()>>,
    /// Each chat's last mark made here, numbered across all chats.
    marks: HashMap<String, u64>,
    made: u64,
    /// Chats whose last mark was an open that never reached the host.
    unsent_opens: HashSet<String>,
    /// Those the link has come back for since: read as opened once on screen.
    due_opens: HashSet<String>,
}

impl App {
    pub(super) fn read_what_is_on_screen(&mut self) {
        if let Some(read) = self.read_due() {
            let id = read["botId"].as_str().unwrap_or_default().to_owned();
            let after =
                if read["automatic"] == false { After::Opened(id, self.reads.made + 1) } else { After::Nothing };
            self.send_mark("markRead", read, after);
        }
    }

    /// Kept for `read_due` to send again, unless a newer mark for the chat was made since.
    pub(super) fn open_failed(&mut self, id: String, mark: u64) {
        if self.reads.marks.get(&id) == Some(&mark) {
            self.reads.unsent_opens.insert(id);
        }
    }

    pub(super) fn relinked(&mut self) {
        let unsent = std::mem::take(&mut self.reads.unsent_opens);
        self.reads.due_opens.extend(unsent);
    }

    fn send_mark(&mut self, method: &'static str, body: Value, after: After) {
        let id = body["botId"].as_str().unwrap_or_default().to_owned();
        self.reads.made += 1;
        self.reads.marks.insert(id.clone(), self.reads.made);
        self.reads.unsent_opens.remove(&id);
        self.reads.due_opens.remove(&id);
        let before = self.reads.last.remove(&id);
        let call = self.client.spawn_call_after(before, method, body, after, self.tx.clone());
        self.reads.last.insert(id, call);
    }

    /// A message sent from here reads the chat on the host, so it must not overtake the chat's marks on their way.
    pub(super) fn send_after_marks(&mut self, body: Value, files: Vec<PathBuf>, after: After) {
        let id = body["botId"].as_str().unwrap_or_default().to_owned();
        let before = self.reads.last.remove(&id);
        let tx = self.tx.clone();
        let call = if files.is_empty() {
            self.client.spawn_call_after(before, "send", body, after, tx)
        } else {
            self.client.spawn_send_files(before, body, files, after, tx)
        };
        self.reads.last.insert(id, call);
    }

    /// The `markRead` this tick owes. A chat the user just picked is the user reading it, whatever this screen knows
    /// of it. One that's merely on screen (picked for them at start-up, back from the trace, after a resize) is read
    /// automatically, once per change of its count, and the host leaves one marked unread alone.
    fn read_due(&mut self) -> Option<Value> {
        let picked = std::mem::take(&mut self.reads.chat_opened);
        if !self.chat_visible() || !self.term_focused {
            return None;
        }
        let unsent = self.online && self.selected.as_ref().is_some_and(|id| self.reads.due_opens.contains(id));
        let opened = picked || unsent;
        let bot = self.bot().filter(|b| opened || b.unread > 0)?;
        let id = bot.id.clone();
        (self.reads.reading.insert(id.clone()) || opened).then(|| json!({"botId": id, "automatic": !opened}))
    }

    /// `.`: the selected chat's "Mark as unread", or "Mark as read" when it has something unread.
    pub(super) fn toggle_unread(&mut self) {
        let Some(bot) = self.bot() else { return };
        let id = bot.id.clone();
        if !bot.offers_mark_unread() {
            self.send_mark("markRead", json!({"botId": id}), After::Nothing);
        } else if self.host_supports(
            MARK_UNREAD_SINCE,
            "Update Codync on {host}: marking a chat unread needs {version} or newer.",
        ) {
            self.send_mark("markUnread", json!({"botId": id}), After::MarkedUnread);
        }
    }

    /// The bot event shows a mark; none comes for a chat with nothing that could count as unread.
    pub(super) fn marked_unread(&mut self, answer: &Value) {
        if answer["bot"]["keptUnread"] != true {
            self.flash("Nothing to mark unread");
        }
    }
}

impl Bot {
    /// What the other clients' roster menus offer: "Mark as read" once there's something to clear.
    fn offers_mark_unread(&self) -> bool {
        self.unread == 0 && !self.kept_unread
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Click, Msg, TraceMode, Width, test_app};
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    fn app_with(bots: &[(&str, i64)]) -> App {
        let mut app = test_app();
        for (id, unread) in bots {
            app.upsert_bot(&json!({"id": id, "name": id, "unread": unread}));
        }
        app
    }

    fn automatic(id: &str) -> Value {
        json!({"botId": id, "automatic": true})
    }

    fn by_the_user(id: &str) -> Value {
        json!({"botId": id, "automatic": false})
    }

    #[tokio::test]
    async fn a_chat_on_screen_is_read_automatically_once_per_change_of_its_count() {
        let mut app = app_with(&[("a", 0)]);
        app.selected = Some("a".into());
        assert_eq!(app.read_due(), None, "nothing unread");

        app.upsert_bot(&json!({"id": "a", "name": "a", "unread": 2}));
        assert_eq!(app.read_due(), Some(automatic("a")));
        assert_eq!(app.read_due(), None, "still unread: a chat marked unread, which the host keeps so");

        app.upsert_bot(&json!({"id": "a", "name": "a", "unread": 3}));
        assert_eq!(app.read_due(), Some(automatic("a")));
    }

    #[tokio::test]
    async fn a_chat_that_merely_comes_on_screen_is_not_opened() {
        // Picked for the user at start-up.
        let mut app = app_with(&[("a", 2)]);
        app.select_first_bot();
        assert_eq!(app.selected.as_deref(), Some("a"));
        assert_eq!(app.read_due(), Some(automatic("a")));

        // The one picked next when the selected bot is deleted.
        let mut app = app_with(&[("a", 2), ("b", 2)]);
        app.select("b");
        app.read_due();
        app.upsert_bot(&json!({"id": "b", "deleted": true}));
        app.select_first_bot();
        assert_eq!(app.read_due(), Some(automatic("a")));

        // Back from the full-screen trace.
        let mut app = app_with(&[("a", 2)]);
        app.selected = Some("a".into());
        app.trace = TraceMode::Full;
        assert_eq!(app.read_due(), None);
        app.trace = TraceMode::Off;
        assert_eq!(app.read_due(), Some(automatic("a")));

        // A narrow terminal made wide.
        let mut app = app_with(&[("a", 2)]);
        app.selected = Some("a".into());
        app.width = Width::Narrow;
        assert_eq!(app.read_due(), None);
        app.width = Width::Wide;
        assert_eq!(app.read_due(), Some(automatic("a")));
    }

    #[tokio::test]
    async fn picking_a_chat_reads_it_as_the_user_whatever_this_screen_knows_of_it() {
        let mut app = app_with(&[("a", 2), ("b", 0)]);
        app.selected = Some("a".into());
        assert_eq!(app.read_due(), Some(automatic("a")), "watched first");

        app.select("b");
        assert_eq!(app.read_due(), Some(by_the_user("b")), "nothing unread here: the host may know better");
        app.select("a");
        assert_eq!(app.read_due(), Some(by_the_user("a")), "back to one this screen already read");
        assert_eq!(app.read_due(), None, "once");
    }

    #[tokio::test]
    async fn picking_the_chat_already_on_screen_reads_it_too() {
        let mut app = app_with(&[("a", 2)]);
        app.selected = Some("a".into());
        app.read_due();

        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.read_due(), Some(by_the_user("a")), "enter in the roster");

        app.hits.clicks.push((Rect::new(0, 0, 1, 1), Click::Bot("a".into())));
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.read_due(), Some(by_the_user("a")), "a click on its row");
    }

    #[tokio::test]
    async fn the_cursor_only_opens_a_chat_it_moves_to() {
        let mut app = app_with(&[("a", 0), ("b", 0)]);
        app.selected = Some("b".into());
        app.move_bot(-1);
        assert_eq!(app.read_due(), Some(by_the_user("a")));
        app.move_bot(-1);
        assert_eq!(app.selected.as_deref(), Some("a"), "the top of the list");
        assert_eq!(app.read_due(), None);

        // Narrow, the cursor moves over a roster with no chat on screen; enter opens one.
        let mut app = app_with(&[("a", 0), ("b", 0)]);
        app.width = Width::Narrow;
        app.selected = Some("b".into());
        app.move_bot(-1);
        assert_eq!(app.read_due(), None);
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.read_due(), Some(by_the_user("a")));
    }

    #[tokio::test]
    async fn an_open_that_never_reached_the_host_is_sent_again_when_the_link_comes_back() {
        let mut app = app_with(&[("a", 0), ("b", 0)]);
        app.online = true;
        app.select("a");
        app.read_what_is_on_screen();
        let mark = app.reads.marks["a"];
        app.on_msg(Msg::Reply(After::Opened("a".into(), mark), Err("Can't reach codync-host".into())));
        assert_eq!(app.read_due(), None, "not before the link comes back");

        app.on_msg(Msg::Online(false));
        app.on_msg(Msg::Online(true));
        assert_eq!(app.read_due(), Some(by_the_user("a")));
        app.read_what_is_on_screen();
        app.relinked();
        assert_eq!(app.read_due(), None, "sent again once");
    }

    #[tokio::test]
    async fn a_failed_open_is_not_sent_again_after_a_newer_mark() {
        let mut app = app_with(&[("a", 0)]);
        app.online = true;
        app.select("a");
        app.read_what_is_on_screen();
        let mark = app.reads.marks["a"];
        app.host_version = MARK_UNREAD_SINCE.into();
        app.on_key(KeyEvent::from(KeyCode::Char('.')));
        assert_ne!(app.reads.marks["a"], mark, "marked unread");
        app.on_msg(Msg::Reply(After::Opened("a".into(), mark), Err("Can't reach codync-host".into())));
        app.relinked();
        assert_eq!(app.read_due(), None);
    }

    #[test]
    fn a_chat_with_something_to_clear_offers_mark_as_read() {
        let offers = |v: Value| Bot::parse(&v).expect("a bot with an id parses").offers_mark_unread();
        assert!(offers(json!({"id": "a"})));
        assert!(!offers(json!({"id": "a", "unread": 1})));
        assert!(!offers(json!({"id": "a", "keptUnread": true})));
    }

    #[tokio::test]
    async fn marking_a_chat_unread_says_when_there_was_nothing_to_mark() {
        let mut app = app_with(&[("a", 0)]);
        app.marked_unread(&json!({"bot": {"keptUnread": true}}));
        assert!(app.hint.is_none());
        app.marked_unread(&json!({"bot": {"keptUnread": false}}));
        assert!(app.hint.is_some());
    }

    #[tokio::test]
    async fn a_message_goes_out_after_the_chats_mark_unread_is_done() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (log, mut seen) = tokio::sync::mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let log = log.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0; 65536];
                    while let Ok(n) = socket.read(&mut buf).await {
                        if n == 0 {
                            return;
                        }
                        let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                        let Some(path) = request.split_whitespace().nth(1) else { return };
                        let method = path.trim_start_matches("/api/").to_owned();
                        log.send(format!("{method} arrived")).unwrap();
                        if method == "markUnread" {
                            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        }
                        log.send(format!("{method} answered")).unwrap();
                        let answer = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";
                        socket.write_all(answer).await.unwrap();
                    }
                });
            }
        });
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(super::super::Client::new(&base, Some("t")), tx, String::new());
        app.upsert_bot(&json!({"id": "a", "name": "a", "unread": 0}));
        app.selected = Some("a".into());
        app.host_version = MARK_UNREAD_SINCE.into();

        app.on_key(KeyEvent::from(KeyCode::Char('.')));
        app.send_after_marks(json!({"botId": "a", "text": "hi"}), Vec::new(), After::Nothing);
        app.reads.last.remove("a").unwrap().await.unwrap();

        let mut order = Vec::new();
        while let Ok(line) = seen.try_recv() {
            order.push(line);
        }
        assert_eq!(order, ["markUnread arrived", "markUnread answered", "send arrived", "send answered"]);
    }

    #[tokio::test]
    async fn an_older_host_is_told_to_update_instead_of_refusing() {
        let mut app = app_with(&[("a", 0)]);
        app.selected = Some("a".into());
        app.host_version = "2.46.0".into();
        app.on_key(KeyEvent::from(KeyCode::Char('.')));
        assert!(app.hint.as_ref().is_some_and(|(text, _)| text.contains("2.47.0")));
    }
}
