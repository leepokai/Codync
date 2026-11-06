//! Reading the chat on screen. The host decides what a read does to a chat marked unread.

use serde_json::{Value, json};

use super::bot::Bot;
use super::{After, App};

impl App {
    pub(super) fn read_what_is_on_screen(&mut self) {
        let shown = self.chat_visible().then(|| self.selected.clone()).flatten();
        let opened = shown.is_some() && shown != self.shown_chat;
        self.shown_chat = shown;
        if self.chat_visible()
            && self.term_focused
            && let Some((id, read)) = self.bot().and_then(|b| Some((b.id.clone(), b.read_on_tick(opened)?)))
            && (self.reading.insert(id) || opened)
        {
            self.call("markRead", read, After::Nothing);
        }
    }
}

impl Bot {
    /// The `markRead` a tick sends for this chat on screen: opening it is the user reading it, whatever this screen
    /// knows of it. While it's watched the read is automatic, and the host leaves a chat marked unread alone.
    pub(super) fn read_on_tick(&self, opened: bool) -> Option<Value> {
        (opened || self.unread > 0).then(|| json!({"botId": self.id, "automatic": !opened}))
    }
}
