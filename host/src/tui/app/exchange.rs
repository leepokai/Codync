//! Bot-to-bot exchanges: the notices `chat::team` writes, and the conversation a pair of bots had.
//! Pure functions over entries, so the chat row and the conversation sheet share one reading.

use std::collections::BTreeMap;

use super::{Entry, Kind};

/// Gap between exchanges that starts a new time separator.
const SEPARATOR_GAP_MS: i64 = 3_600_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Done,
    /// The detail the host recorded; empty when it gave none.
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub id: String,
    pub seq: i64,
    /// The bot whose chat holds this notice.
    pub chat: String,
    pub source: String,
    pub target: String,
    pub text: String,
    pub reply: Option<String>,
    pub created_at: i64,
    pub outcome: Outcome,
}

impl Exchange {
    /// The structured exchange a notice in `chat` carries, if it carries one.
    pub fn of(chat: &str, e: &Entry) -> Option<Self> {
        if !e.is_bot_message() {
            return None;
        }
        let message = &e.data["botMessage"];
        let (source, target, text) =
            (message["sourceBotId"].as_str()?, message["targetBotId"].as_str()?, message["text"].as_str()?);
        let reply = message["reply"].as_str();
        let outcome = match e.data["status"].as_str() {
            Some("completed") => Outcome::Done,
            Some("failed" | "cancelled") => Outcome::Failed(message["detail"].as_str().unwrap_or_default().to_owned()),
            _ => Outcome::Pending,
        };
        Some(Self {
            id: e.id.clone(),
            seq: e.seq,
            chat: chat.to_owned(),
            source: source.to_owned(),
            target: target.to_owned(),
            text: text.to_owned(),
            reply: reply.map(str::to_owned),
            created_at: e.created_at,
            outcome,
        })
    }

    /// The chat's bot sent it.
    pub fn outgoing(&self) -> bool {
        self.source == self.chat
    }

    /// The other bot.
    pub fn peer(&self) -> &str {
        if self.outgoing() { &self.target } else { &self.source }
    }

    pub fn verb(&self) -> &'static str {
        if self.outgoing() { "Messaged" } else { "Message from" }
    }
}

/// One chat row: an entry, or a run of two or more exchanges with the same peer.
#[derive(Debug)]
pub enum Item<'a> {
    Entry(&'a Entry),
    Group(Group<'a>),
}

#[derive(Debug)]
pub struct Group<'a> {
    /// The run's first exchange: its id keys the row and opens the conversation.
    pub first: &'a Entry,
    pub peer: String,
    pub count: usize,
    /// Any exchange in the run failed.
    pub failed: bool,
}

/// Whether the chat draws this entry (the rest is trace).
fn is_visible(e: &Entry) -> bool {
    matches!(e.kind, Kind::User | Kind::Permission | Kind::Notice) || e.is_final()
}

/// The main chat's rows: consecutive exchanges with one peer (either direction) become one group.
/// Anything else the chat shows ends the run; trace entries neither break nor appear in it.
/// A run of one stays a plain entry, and threads (`main` false) are never grouped.
pub fn group<'a>(chat: &str, entries: &[&'a Entry], main: bool) -> Vec<Item<'a>> {
    fn flush<'a>(out: &mut Vec<Item<'a>>, run: &mut Vec<(&'a Entry, Exchange)>) {
        match run.as_slice() {
            [] => {}
            [(e, _)] => out.push(Item::Entry(e)),
            [(first, x), ..] => out.push(Item::Group(Group {
                first,
                peer: x.peer().to_owned(),
                count: run.len(),
                failed: run.iter().any(|(_, x)| matches!(x.outcome, Outcome::Failed(_))),
            })),
        }
        run.clear();
    }
    let mut out = vec![];
    let mut run: Vec<(&Entry, Exchange)> = vec![];
    for &e in entries {
        if !main {
            out.push(Item::Entry(e));
        } else if let Some(x) = Exchange::of(chat, e) {
            if run.last().is_some_and(|(_, r)| r.peer() != x.peer()) {
                flush(&mut out, &mut run);
            }
            run.push((e, x));
        } else if is_visible(e) {
            flush(&mut out, &mut run);
            out.push(Item::Entry(e));
        }
    }
    flush(&mut out, &mut run);
    out
}

/// `peer`'s exchanges with `chat`, oldest first: the fetched history plus the live chat, the live copy winning.
pub fn conversation(chat: &str, fetched: &[Entry], live: Option<&BTreeMap<i64, Entry>>, peer: &str) -> Vec<Exchange> {
    let mut by_id: BTreeMap<String, Exchange> = BTreeMap::new();
    for e in fetched.iter().chain(live.into_iter().flat_map(BTreeMap::values)) {
        if let Some(x) = Exchange::of(chat, e) {
            by_id.insert(x.id.clone(), x);
        }
    }
    let mut all: Vec<Exchange> = by_id.into_values().filter(|x| x.peer() == peer).collect();
    all.sort_by_key(|x| x.seq);
    all
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationRow {
    Separator(i64),
    Message {
        author: String,
        text: String,
        shows_author: bool,
    },
    /// A pending or failed exchange; `awaiting` is the bot being waited for.
    Status {
        outcome: Outcome,
        awaiting: String,
    },
}

/// The sheet's rows. A separator or status row ends the previous author's run.
pub fn rows(exchanges: &[Exchange]) -> Vec<ConversationRow> {
    let mut out = vec![];
    let mut prev_at: Option<i64> = None;
    let mut prev_author: Option<&str> = None;
    for x in exchanges {
        if prev_at.is_none_or(|at| x.created_at - at > SEPARATOR_GAP_MS) {
            out.push(ConversationRow::Separator(x.created_at));
            prev_author = None;
        }
        prev_at = Some(x.created_at);
        let mut say = |author: &'_ str, text: &str, prev: &mut Option<&str>| {
            out.push(ConversationRow::Message {
                author: author.to_owned(),
                text: text.to_owned(),
                shows_author: *prev != Some(author),
            });
        };
        say(&x.source, &x.text, &mut prev_author);
        prev_author = Some(&x.source);
        if let Some(reply) = &x.reply {
            say(&x.target, reply, &mut prev_author);
            prev_author = Some(&x.target);
        }
        if x.outcome != Outcome::Done {
            out.push(ConversationRow::Status { outcome: x.outcome.clone(), awaiting: x.target.clone() });
            prev_author = None;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Kind;
    use super::*;
    use serde_json::json;

    fn notice(id: &str, seq: i64, at: i64, message: &serde_json::Value, status: &str) -> Entry {
        let data = json!({"text": "A rendered notice", "botMessage": message, "status": status});
        Entry { id: id.into(), seq, turn: 1, kind: Kind::Notice, created_at: at, thread_id: None, data }
    }

    fn msg(source: &str, target: &str, text: &str) -> serde_json::Value {
        json!({"sourceBotId": source, "targetBotId": target, "text": text})
    }

    #[test]
    fn direction_and_peer_follow_the_chat() {
        let e = notice("n", 1, 0, &msg("a", "b", "hi"), "queued");
        let out = Exchange::of("a", &e).unwrap();
        assert!(out.outgoing());
        assert_eq!((out.peer(), out.verb(), out.text.as_str()), ("b", "Messaged", "hi"));
        let inc = Exchange::of("b", &e).unwrap();
        assert_eq!((inc.peer(), inc.verb()), ("a", "Message from"));
        assert_eq!(inc.text, "hi", "the same structured request appears in either chat");
        let mut plain = e.clone();
        plain.data = json!({"text": "Messaged Owen"});
        assert!(Exchange::of("a", &plain).is_none());
        let mut old_notice = e.clone();
        old_notice.data =
            json!({"text": "Asked b: hi", "heading": "Asked b: hi", "sourceBotId": "a", "targetBotId": "b"});
        assert!(Exchange::of("a", &old_notice).is_none());
        let mut other = e;
        other.kind = Kind::Agent;
        assert!(Exchange::of("a", &other).is_none());
    }

    fn shape(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                Item::Entry(e) => e.id.clone(),
                Item::Group(g) => format!("{}x{}{}@{}", g.count, g.peer, if g.failed { "!" } else { "" }, g.first.id),
            })
            .collect()
    }

    fn grouped(chat: &str, entries: &[Entry], main: bool) -> Vec<String> {
        let refs: Vec<&Entry> = entries.iter().collect();
        shape(&group(chat, &refs, main))
    }

    #[test]
    fn same_peer_runs_group_in_either_direction() {
        let xs = [
            notice("1", 1, 0, &msg("a", "b", "x"), "completed"),
            notice("2", 2, 0, &msg("b", "a", "y"), "completed"),
            notice("3", 3, 0, &msg("a", "b", "z"), "failed"),
        ];
        assert_eq!(grouped("a", &xs, true), ["3xb!@1"]);
        assert_eq!(grouped("a", &xs[..2], true), ["2xb@1"]);
        assert_eq!(grouped("a", &xs[..1], true), ["1"]);
        assert_eq!(grouped("a", &xs, false), ["1", "2", "3"], "threads never group");
    }

    #[test]
    fn other_peers_and_visible_entries_split_but_trace_does_not() {
        let mut user = notice("u", 3, 0, &msg("a", "b", ""), "queued");
        user.kind = Kind::User;
        user.data = json!({"text": "hi"});
        let mut tool = user.clone();
        tool.id = "t".into();
        tool.kind = Kind::Tool;
        let mut thought = tool.clone();
        thought.id = "th".into();
        thought.kind = Kind::Agent;
        let ex = |id: &str, seq, peer: &str| notice(id, seq, 0, &msg("a", peer, "x"), "completed");
        let xs = [ex("1", 1, "b"), ex("2", 2, "b"), tool.clone(), thought, ex("3", 5, "b")];
        assert_eq!(grouped("a", &xs, true), ["3xb@1"], "trace entries do not split");
        let xs = [ex("1", 1, "b"), ex("2", 2, "b"), user.clone(), ex("3", 4, "b"), ex("4", 5, "b")];
        assert_eq!(grouped("a", &xs, true), ["2xb@1", "u", "2xb@3"]);
        let xs = [ex("1", 1, "b"), ex("2", 2, "b"), ex("3", 3, "c"), ex("4", 4, "b")];
        assert_eq!(grouped("a", &xs, true), ["2xb@1", "3", "4"]);
        let mut other = ex("n", 3, "b");
        other.data = json!({"text": "Routine ran"});
        let xs = [ex("1", 1, "b"), other, ex("2", 3, "b")];
        assert_eq!(grouped("a", &xs, true), ["1", "n", "2"]);
    }

    #[test]
    fn reply_comes_from_structured_data_without_parsing_display_text() {
        let mut m = msg("a", "b", "hi");
        m["reply"] = "line one\nline two".into();
        let reply = |m: &serde_json::Value| Exchange::of("a", &notice("n", 1, 0, m, "completed")).unwrap().reply;
        assert_eq!(reply(&m).as_deref(), Some("line one\nline two"));
        m.as_object_mut().unwrap().remove("reply");
        m["detail"] = "Reply from b:\nThis is display text, not a reply".into();
        assert_eq!(reply(&m), None);
    }

    #[test]
    fn bot_replies_stay_in_the_popup_and_user_reports_stay_in_main_chat() {
        let mut message = msg("a", "b", "question");
        message["reply"] = "answer for a".into();
        let ask = notice("ask", 1, 0, &message, "completed");
        let mut trace = ask.clone();
        trace.id = "trace".into();
        trace.kind = Kind::Agent;
        trace.data = json!({"text": "answer for a"});
        let mut report = trace.clone();
        report.id = "report".into();
        report.data = json!({"text": "report for the user", "final": true});
        let entries = [ask, trace, report];
        assert_eq!(grouped("b", &entries, true), ["ask", "report"]);
        let popup = conversation("b", &entries, None, "a");
        assert_eq!(popup.len(), 1);
        assert_eq!(popup[0].reply.as_deref(), Some("answer for a"));
    }

    #[test]
    fn status_maps_to_an_outcome() {
        let mut m = msg("a", "b", "hi");
        m["detail"] = "Recipient stopped.".into();
        let outcome = |status: &str| Exchange::of("a", &notice("n", 1, 0, &m, status)).unwrap().outcome;
        assert_eq!(outcome("queued"), Outcome::Pending);
        assert_eq!(outcome("sent"), Outcome::Pending);
        assert_eq!(outcome("completed"), Outcome::Done);
        assert_eq!(outcome("failed"), Outcome::Failed("Recipient stopped.".into()));
        assert_eq!(outcome("cancelled"), Outcome::Failed("Recipient stopped.".into()));
    }

    #[test]
    fn merge_prefers_live_copy_and_filters_the_peer() {
        let fetched = [
            notice("old", 1, 0, &msg("a", "b", "first"), "queued"),
            notice("c", 2, 0, &msg("a", "c", "elsewhere"), "queued"),
        ];
        let live: BTreeMap<i64, Entry> = [
            (1, notice("old", 1, 0, &msg("a", "b", "first"), "completed")),
            (3, notice("new", 3, 0, &msg("b", "a", "second"), "queued")),
        ]
        .into();
        let xs = conversation("a", &fetched, Some(&live), "b");
        assert_eq!(xs.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["old", "new"]);
        assert_eq!(xs[0].outcome, Outcome::Done);
        assert_eq!(conversation("a", &fetched, None, "b").len(), 1);
    }

    #[test]
    fn rows_group_authors_and_separate_by_hour() {
        let mut ask = msg("a", "b", "question");
        ask["reply"] = "answer".into();
        let hour = SEPARATOR_GAP_MS;
        let xs = [
            Exchange::of("a", &notice("1", 1, 0, &ask, "completed")).unwrap(),
            Exchange::of("a", &notice("2", 2, 1000, &msg("a", "b", "again"), "queued")).unwrap(),
            Exchange::of("a", &notice("3", 3, 1000 + hour + 1, &msg("a", "b", "later"), "failed")).unwrap(),
        ];
        let message = |author: &str, text: &str, shows_author| ConversationRow::Message {
            author: author.into(),
            text: text.into(),
            shows_author,
        };
        assert_eq!(
            rows(&xs),
            [
                ConversationRow::Separator(0),
                message("a", "question", true),
                message("b", "answer", true),
                message("a", "again", true),
                ConversationRow::Status { outcome: Outcome::Pending, awaiting: "b".into() },
                ConversationRow::Separator(1000 + hour + 1),
                message("a", "later", true),
                ConversationRow::Status { outcome: Outcome::Failed(String::new()), awaiting: "b".into() },
            ]
        );
        let same_author = [
            Exchange::of("a", &notice("1", 1, 0, &msg("a", "b", "x"), "completed")).unwrap(),
            Exchange::of("a", &notice("2", 2, 1, &msg("a", "b", "y"), "completed")).unwrap(),
        ];
        assert!(matches!(&rows(&same_author)[2], ConversationRow::Message { shows_author: false, .. }));
    }
}
