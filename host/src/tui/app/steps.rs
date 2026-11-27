//! The working line's steps (what the running turn has done so far, unfolded for one bot and turn), and how any list of
//! steps shows subagents: their steps nested under their launch, the launch's quiet line.

use std::collections::{HashMap, HashSet};

use super::super::l10n::{fill, tr};
use super::{App, Bot, Entry};
use serde_json::Value;

/// The turn running now; a queued message already holds the next turn.
pub(in crate::tui) fn running_turn(entries: &[&Entry]) -> Option<i64> {
    entries.iter().filter(|e| e.data["status"] != "queued").map(|e| e.turn).max()
}

/// The running turn's thinking, narration, tool calls and plan, in order.
pub(in crate::tui) fn working_steps<'a>(entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let turn = running_turn(entries);
    entries.iter().copied().filter(|e| Some(e.turn) == turn && e.is_working_step()).collect()
}

/// Steps with each subagent's steps right under the call that launched it (`parent`, in the same turn), `depth`
/// deeper. A step whose launch isn't among them stays where it is. `entry` reads an item's entry.
pub(in crate::tui) fn nested<'a, T: Copy>(items: &[T], entry: impl Fn(T) -> &'a Entry) -> Vec<(T, usize)> {
    // A call is known by its turn and id: a launch is looked for in its own turn only.
    let call = |item: T, key: &str| {
        let e = entry(item);
        e.data[key].as_str().map(|id| (e.turn, id))
    };
    let ids: HashSet<(i64, &str)> = items.iter().filter_map(|i| call(*i, "toolCallId")).collect();
    let launch = |item: T| call(item, "parent").filter(|p| Some(*p) != call(item, "toolCallId") && ids.contains(p));
    let mut children: HashMap<(i64, &str), Vec<T>> = HashMap::new();
    for item in items {
        if let Some(parent) = launch(*item) {
            children.entry(parent).or_default().push(*item);
        }
    }
    let mut out = vec![];
    let mut stack: Vec<(T, usize)> = items.iter().rev().filter(|i| launch(**i).is_none()).map(|i| (*i, 0)).collect();
    while let Some((item, depth)) = stack.pop() {
        out.push((item, depth));
        let mine = call(item, "toolCallId").and_then(|id| children.get(&id)).into_iter().flatten();
        stack.extend(mine.rev().map(|c| (*c, depth + 1)));
    }
    out
}

/// A subagent's launch, quietly: its kind, then the tokens it used and how long it ran once it finished, or that it
/// runs in the background. `None` while a foreground one runs, or for any other call.
pub(in crate::tui) fn subagent_line(data: &Value) -> Option<String> {
    let s = &data["subagent"];
    let background = s["background"] == true;
    let tokens = s["tokens"].as_u64();
    if !background && tokens.is_none() {
        return None;
    }
    let mut parts: Vec<String> = s["kind"].as_str().map(str::to_owned).into_iter().collect();
    if background {
        parts.push(tr("in background").into_owned());
    }
    if let Some(tokens) = tokens {
        parts.push(fill(&tr("{tokens} tokens"), &[("tokens", &compact_count(tokens))]));
    }
    if let Some(ms) = s["durationMs"].as_u64() {
        let secs = ((ms + 500) / 1000).max(1);
        parts.push(if secs < 60 {
            fill(&tr("{count} s"), &[("count", &secs.to_string())])
        } else {
            fill(&tr("{count} min"), &[("count", &((secs + 30) / 60).to_string())])
        });
    }
    Some(parts.join(" · "))
}

/// 812, 42.3k, 1.2M.
fn compact_count(n: u64) -> String {
    let (div, unit) = match n {
        0..1000 => return n.to_string(),
        1000..1_000_000 => (1000, "k"),
        _ => (1_000_000, "M"),
    };
    let tenths = (n * 10 + div / 2) / div;
    if tenths >= 1000 {
        format!("{}{unit}", (n + div / 2) / div)
    } else if tenths.is_multiple_of(10) {
        format!("{}{unit}", tenths / 10)
    } else {
        format!("{}.{}{unit}", tenths / 10, tenths % 10)
    }
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

    fn entry(id: &str, turn: i64, kind: Kind, data: Value) -> Entry {
        Entry { id: id.into(), seq: 0, turn, kind, data, rev: None, created_at: 0 }
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

    #[test]
    fn a_subagents_steps_sit_under_its_launch() {
        let tool =
            |id: &str, parent: Option<&str>| entry(id, 1, Kind::Tool, json!({"toolCallId": id, "parent": parent}));
        let steps = [
            tool("a", None),
            tool("b", None),
            tool("a1", Some("a")),
            tool("b1", Some("b")),
            tool("a2", Some("a")),
            tool("lost", Some("gone")),
            tool("self", Some("self")),
            entry("later", 2, Kind::Tool, json!({"toolCallId": "later", "parent": "a"})),
        ];
        let refs: Vec<&Entry> = steps.iter().collect();
        let order: Vec<_> = nested(&refs, |e| e).iter().map(|(e, depth)| (e.id.as_str(), *depth)).collect();
        assert_eq!(
            order,
            [("a", 0), ("a1", 1), ("a2", 1), ("b", 0), ("b1", 1), ("lost", 0), ("self", 0), ("later", 0)]
        );
    }

    #[test]
    fn a_subagents_launch_reads_its_totals_or_that_it_runs_in_the_background() {
        let line = |s: Value| subagent_line(&json!({"subagent": s}));
        assert_eq!(line(json!({"kind": "general-purpose", "background": false})), None);
        assert_eq!(
            line(json!({"kind": "general-purpose", "tokens": 42310, "durationMs": 6287})).as_deref(),
            Some("general-purpose · 42.3k tokens · 6 s")
        );
        assert_eq!(
            line(json!({"kind": "Explore", "tokens": 812, "durationMs": 125_000})).as_deref(),
            Some("Explore · 812 tokens · 2 min")
        );
        assert_eq!(
            line(json!({"kind": "general-purpose", "background": true})).as_deref(),
            Some("general-purpose · in background")
        );
        assert_eq!(subagent_line(&json!({})), None);
        let tokens: Vec<_> =
            [1150, 2000, 123_456, 1_200_000].map(|n| line(json!({"tokens": n})).unwrap_or_default()).into();
        assert_eq!(tokens, ["1.2k tokens", "2k tokens", "123k tokens", "1.2M tokens"]);
    }
}
