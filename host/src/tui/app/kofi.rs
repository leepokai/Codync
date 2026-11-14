//! The Ko-fi invitation, as the desktop app's: once per terminal, a quiet line under the first finished reply to the
//! first message it sends (it keeps no record of older ones, so a terminal used before gets it on its next message),
//! kept for the rest of the run that showed it and never shown again. Not on the phones: the stores' rules on
//! external tip links.

use super::{App, Entry};
use crate::analytics::reported::Reported;

pub(in crate::tui) const URL: &str = "https://ko-fi.com/leepokai";
/// This terminal's saved state: missing until a message is sent, then `due <bot> <ms>`, then `done` once shown.
const FILE: &str = "tui-kofi-invite";

#[derive(Debug, PartialEq)]
pub(in crate::tui) enum Invite {
    /// Nothing sent from this terminal yet.
    Never,
    Due {
        bot: String,
        since: i64,
    },
    /// Under `reply` until the terminal closes; `seen` (and saved as done) once its chat was on screen.
    Showing {
        bot: String,
        reply: String,
        seen: bool,
    },
    Done,
}

impl Invite {
    pub(in crate::tui) fn load() -> Self {
        Self::parse(super::super::choice::read(FILE).as_deref())
    }

    fn parse(saved: Option<&str>) -> Self {
        let Some(saved) = saved else { return Self::Never };
        let mut parts = saved.split_whitespace();
        match (parts.next(), parts.next(), parts.next().and_then(|ms| ms.parse().ok())) {
            (Some("due"), Some(bot), Some(since)) => Self::Due { bot: bot.to_owned(), since },
            _ => Self::Done,
        }
    }
}

fn save(value: &str) {
    if let Err(error) = super::super::choice::write(FILE, value) {
        tracing::warn!(%error, "couldn't save the Ko-fi invitation's state");
    }
}

impl App {
    /// The first message sent from this terminal arrived: the invitation waits for its reply.
    pub(super) fn kofi_sent(&mut self, bot: &str, sent: &Entry) {
        if self.kofi == Invite::Never {
            save(&format!("due {bot} {}", sent.created_at));
            self.kofi = Invite::Due { bot: bot.to_owned(), since: sent.created_at };
        }
    }

    /// A reply came in or changed (a plain one is marked final later): the first finished one in that chat after the
    /// first message carries the invitation.
    pub(super) fn kofi_reply(&mut self, bot: &str, e: &Entry) {
        let Invite::Due { bot: due, since } = &self.kofi else { return };
        if due == bot && e.is_final() && e.created_at >= *since {
            self.kofi = Invite::Showing { bot: bot.to_owned(), reply: e.id.clone(), seen: false };
        }
    }

    /// Before each draw: once the invitation's chat is on screen, it counts as shown and never comes back.
    pub(in crate::tui) fn kofi_seen(&mut self) {
        let visible = self.chat_visible();
        let Invite::Showing { bot, seen, .. } = &mut self.kofi else { return };
        if *seen || !visible || self.selected.as_deref() != Some(bot.as_str()) {
            return;
        }
        *seen = true;
        save("done");
        self.track(Reported::KofiInviteShown);
    }

    /// The reply the invitation sits under in this run.
    pub(in crate::tui) fn kofi_at(&self) -> Option<&str> {
        match &self.kofi {
            Invite::Showing { reply, .. } => Some(reply),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Entry, Msg, test_app};
    use super::Invite;
    use serde_json::{Value, json};

    fn value(seq: i64, kind: &str, at: i64, finished: bool) -> Value {
        json!({"botId": "b", "id": format!("e{seq}"), "seq": seq, "turn": 1, "kind": kind, "createdAt": at,
            "data": {"text": "hi", "final": finished}})
    }

    fn entry(seq: i64, kind: &str, at: i64, finished: bool) -> Entry {
        Entry::parse(&value(seq, kind, at, finished)).expect("a full entry parses").1
    }

    #[tokio::test]
    async fn the_first_finished_reply_after_the_first_message_carries_it_once() {
        let mut app = test_app();
        app.kofi = Invite::Never;
        app.kofi_sent("b", &entry(1, "user", 100, false));
        app.kofi_sent("b", &entry(3, "user", 300, false));
        assert_eq!(app.kofi, Invite::Due { bot: "b".into(), since: 100 }, "only the first message counts");
        app.kofi_reply("other", &entry(2, "agent", 150, true));
        app.kofi_reply("b", &entry(2, "agent", 150, false));
        assert_eq!(app.kofi_at(), None, "another chat, or a reply still being written");
        app.kofi_reply("b", &entry(4, "agent", 160, true));
        app.kofi_reply("b", &entry(5, "agent", 170, true));
        assert_eq!(app.kofi_at(), Some("e4"));
    }

    #[tokio::test]
    async fn a_plain_reply_carries_it_once_final_and_it_counts_once_its_chat_is_on_screen() {
        let mut app = test_app();
        app.kofi = Invite::Due { bot: "b".into(), since: 100 };
        for finished in [false, true] {
            app.on_msg(Msg::Event(json!({"type": "entry", "entry": value(2, "agent", 150, finished)})));
        }
        assert_eq!(app.kofi_at(), Some("e2"), "marked final by an update under the same seq");
        app.kofi_seen();
        assert!(matches!(app.kofi, Invite::Showing { seen: false, .. }), "its chat isn't open");
        app.selected = Some("b".into());
        app.kofi_seen();
        assert!(matches!(app.kofi, Invite::Showing { seen: true, .. }));
        assert_eq!(app.kofi_at(), Some("e2"), "kept for the rest of the run");
    }

    #[test]
    fn the_saved_state_reads_back() {
        assert_eq!(Invite::parse(None), Invite::Never);
        assert_eq!(Invite::parse(Some("due b1 1700\n")), Invite::Due { bot: "b1".into(), since: 1700 });
        assert_eq!(Invite::parse(Some("done")), Invite::Done);
        assert_eq!(Invite::parse(Some("due b1")), Invite::Done, "damaged: never shown rather than again");
    }
}
