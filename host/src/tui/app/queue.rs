//! The user's messages waiting behind a running turn: `S` sends the next one now (`steer`), `D` takes back the last
//! one (`unqueue`).

use super::{After, App, Entry};
use serde_json::json;

/// The first host that sends a queued message now and takes one back.
const STEER_SINCE: &str = "2.39.0";

impl App {
    /// `bot`'s queued messages, oldest (the next to go) first.
    pub fn queued(&self, bot: &str) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.lane(bot).into_iter().filter(|e| e.queued()).collect();
        v.sort_by_key(|e| e.seq);
        v
    }

    /// `bot`'s chat: its lane without the queued messages, which wait above the composer.
    pub fn chat(&self, bot: &str) -> Vec<&Entry> {
        self.lane(bot).into_iter().filter(|e| !e.queued()).collect()
    }

    /// Whether the host takes `S` and `D`: an older one only queues.
    pub fn queue_keys(&self) -> bool {
        crate::compat::at_least(&self.host_version, STEER_SINCE)
    }

    /// `S`: the next queued message goes out now, into the running turn when the agent can take it.
    pub(super) fn steer_next(&mut self) {
        let next = self.selected.as_deref().and_then(|bot| self.queued(bot).first().map(|e| e.id.clone()));
        self.on_queued("steer", next, "Sending now…");
    }

    /// `D`: the last queued message never goes out.
    pub(super) fn unqueue_last(&mut self) {
        let last = self.selected.as_deref().and_then(|bot| self.queued(bot).last().map(|e| e.id.clone()));
        self.on_queued("unqueue", last, "Removed");
    }

    /// `method` on the selected bot's queued message `id`, saying `done`.
    fn on_queued(&mut self, method: &'static str, id: Option<String>, done: &str) {
        let Some(bot) = self.selected.clone() else { return };
        let Some(id) = id else {
            self.flash("No message waiting");
            return;
        };
        if self
            .host_supports(STEER_SINCE, "Update Codync on {host}: sending now and taking back need {version} or newer.")
        {
            self.call(method, json!({"botId": bot, "entryId": id}), After::Nothing);
            self.flash(done);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Msg, test_app};
    use serde_json::json;

    fn entry(app: &mut super::App, id: &str, seq: i64, data: &serde_json::Value) {
        let e = json!({"id": id, "botId": "a", "seq": seq, "kind": "user", "turn": seq, "data": data});
        app.on_msg(Msg::Event(json!({"type": "entry", "entry": e})));
    }

    #[test]
    fn queued_lists_waiting_messages_oldest_first_and_never_a_removed_one() {
        let mut app = test_app();
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A"}})));
        entry(&mut app, "sent", 1, &json!({"text": "hi", "status": "sent"}));
        entry(&mut app, "second", 3, &json!({"text": "two", "status": "queued"}));
        entry(&mut app, "first", 2, &json!({"text": "one", "status": "queued"}));
        entry(&mut app, "gone", 4, &json!({"text": "x", "status": "cancelled", "removed": true}));
        let ids: Vec<&str> = app.queued("a").iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["first", "second"]);
        let chat: Vec<&str> = app.chat("a").iter().map(|e| e.id.as_str()).collect();
        assert_eq!(chat, ["sent"], "the chat has neither the waiting messages nor a removed one");
    }

    #[tokio::test]
    async fn s_and_d_act_on_a_waiting_message_on_a_host_that_takes_them() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = test_app();
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "a", "name": "A"}})));
        app.selected = Some("a".into());
        app.host = "test".into();
        let hint = |app: &mut super::App, c: char| {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT));
            app.hint.take().map(|(text, _)| text).unwrap_or_default()
        };
        assert_eq!(hint(&mut app, 'S'), "No message waiting");
        entry(&mut app, "q", 1, &json!({"text": "one", "status": "queued"}));
        app.host_version = "2.38.0".into();
        assert!(!app.queue_keys());
        assert_eq!(hint(&mut app, 'S'), "Update Codync on test: sending now and taking back need 2.39.0 or newer.");
        app.host_version = "2.39.0".into();
        assert!(app.queue_keys());
        assert_eq!(hint(&mut app, 'S'), "Sending now…");
        assert_eq!(hint(&mut app, 'D'), "Removed");
    }
}
