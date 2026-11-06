//! The working line's steps: what the running turn has done so far, unfolded for one bot and turn.

use super::{App, Bot, Entry};

/// The turn running now; a queued message already holds the next turn.
pub(in crate::tui) fn running_turn(entries: &[&Entry]) -> Option<i64> {
    entries.iter().filter(|e| e.data["status"] != "queued").map(|e| e.turn).max()
}

/// The running turn's thinking, narration, tool calls and plan, in order.
pub(in crate::tui) fn working_steps<'a>(entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let turn = running_turn(entries);
    entries.iter().copied().filter(|e| Some(e.turn) == turn && e.is_working_step()).collect()
}

impl App {
    /// Whether `bot`'s chat shows the working line: a turn in flight in this chat, while connected.
    pub(in crate::tui) fn shows_activity(&self, bot: &Bot) -> bool {
        self.online && bot.busy() && bot.works_here()
    }

    /// `>` or a click on the working line: unfolds or folds the selected bot's steps for this turn, once thinking
    /// details are on.
    pub(super) fn toggle_steps(&mut self) {
        let Some(bot) = self.bot().filter(|b| self.shows_activity(b)) else { return };
        let Some(turn) = running_turn(&self.lane(&bot.id)) else { return };
        if !super::super::details::enabled() {
            self.flash("Turn on Show thinking details (^k) to unfold the steps");
            return;
        }
        let key = (bot.id.clone(), turn);
        self.steps_open = if self.steps_open.as_ref() == Some(&key) { None } else { Some(key) };
    }
}

#[cfg(test)]
mod tests {
    use super::super::Kind;
    use super::*;
    use serde_json::json;

    fn entry(id: &str, turn: i64, kind: Kind, data: serde_json::Value) -> Entry {
        Entry { id: id.into(), seq: 0, turn, kind, data, created_at: 0 }
    }

    #[test]
    fn steps_are_the_running_turns_work_without_what_the_chat_or_memory_show() {
        let entries = [
            entry("old", 1, Kind::Tool, json!({})),
            entry("u2", 2, Kind::User, json!({"text": "go"})),
            entry("thinking", 2, Kind::Thought, json!({"text": "Checking the build"})),
            entry("blank", 2, Kind::Thought, json!({"text": "  "})),
            entry("says", 2, Kind::Agent, json!({"text": "Running the tests"})),
            entry("terminal", 2, Kind::Tool, json!({"title": "npm test", "bookkeeping": false})),
            entry("memory", 2, Kind::Tool, json!({"title": "mcp__memory__edit_memory", "bookkeeping": true})),
            entry("unflagged", 2, Kind::Tool, json!({"title": "an older host's call"})),
            entry("plan", 2, Kind::Plan, json!({"entries": []})),
            entry("reply", 2, Kind::Agent, json!({"text": "Done", "final": true})),
            entry("card", 2, Kind::Permission, json!({})),
            entry("pass", 2, Kind::Agent, json!({"text": "(pass)"})),
            entry("q", 3, Kind::User, json!({"status": "queued"})),
        ];
        let ids: Vec<_> = working_steps(&entries.iter().collect::<Vec<_>>()).iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["thinking", "says", "terminal", "plan"]);
    }
}
