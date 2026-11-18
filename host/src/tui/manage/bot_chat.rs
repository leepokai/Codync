//! A bot-to-bot conversation sheet: read-only, opened from a bot-message row.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::json;

use super::super::app::{App, Exchange, Overlay};
use super::scroll::up_from_end;
use super::{BotChatSheet, Reply};

impl App {
    /// Opens the conversation a bot-message row in the selected chat stands for.
    pub(in crate::tui) fn open_bot_chat(&mut self, entry: &str) {
        let Some(chat) = self.selected.clone() else { return };
        let Some(x) = self.lane(&chat).into_iter().find(|e| e.id == entry).and_then(|e| Exchange::of(&chat, e)) else {
            return;
        };
        let peer = x.peer().to_owned();
        self.sheet(
            "botConversation",
            json!({"botId": chat, "peerId": peer}),
            Reply::BotChat { bot: chat.clone(), peer: peer.clone() },
        );
        self.overlays.push(Overlay::BotChat(BotChatSheet {
            bot: chat,
            peer,
            fetched: None,
            scroll: 0,
            max_scroll: Cell::default(),
            page: Cell::default(),
        }));
    }

    pub(in crate::tui) fn bot_chat_key(mut c: BotChatSheet, k: KeyEvent) -> Option<Overlay> {
        if matches!(k.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
            return None;
        }
        up_from_end(&mut c.scroll, c.max_scroll.get(), c.page.get(), k);
        Some(Overlay::BotChat(c))
    }
}
