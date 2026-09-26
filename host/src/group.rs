//! Group chats (Grok Bot's design): several bots and the user in one conversation.
//!
//! A group is a roster row (`BotKind::Group`) with a member list and its own
//! transcript, but no agent. When the user writes, the group runs a *room turn*:
//! up to [`MAX_ROUNDS`] rounds in which the members answer one at a time, each in
//! its **own** session (a member's history is unified across its chats), told
//! what was said since it last spoke. Everyone answers unless the new messages
//! @-mention someone; a member with nothing to add says `(pass)`, and a round
//! where nobody speaks ends the turn. A newer message in the same lane (or Stop)
//! moves the lane's epoch, which ends a running room turn before its next speaker.
//!
//! Threads work the same way inside a group: the room turn runs in the thread's
//! lane and only sees the thread.

use crate::LockExt;
use crate::bot::Cmd;
use crate::hub::Hub;
use crate::push::{self, AlertKind};
use crate::store::{BotConfig, BotKind, Entry, EntryKind, Lane, Store};
use anyhow::{Result, anyhow, bail};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

const MAX_ROUNDS: usize = 3;
/// Replies per room turn, across all rounds.
const MAX_REPLIES: usize = 10;
/// Room lines a member is shown per turn.
const HISTORY_LINES: i64 = 24;
/// A bot's line in another member's prompt is clipped (the user's never is).
const LINE_CHARS: usize = 2000;
const PASS: &str = "(pass)";

/// One member's turn in a room, queued on its actor like any other turn.
pub struct GroupTurn {
    pub lane: Lane,
    pub prompt: String,
    /// The reply the room sees, or `None` for a pass.
    pub reply: oneshot::Sender<Result<Option<String>>>,
}

/// Room turn epochs per lane (`Lane::key`).
#[derive(Default)]
pub struct Rooms(Mutex<HashMap<String, u64>>);

impl Rooms {
    fn next(&self, lane: &Lane) -> u64 {
        let mut map = self.0.locked();
        let epoch = map.entry(lane.key()).or_default();
        *epoch += 1;
        *epoch
    }

    fn current(&self, lane: &Lane, epoch: u64) -> bool {
        self.0.locked().get(&lane.key()).copied() == Some(epoch)
    }

    /// Stop: ends every room turn in `group` (main chat and threads) and the member turns in it.
    pub fn stop(&self, hub: &Hub, group: &str) {
        let thread_prefix = format!("{group}/");
        let mut stopped = false;
        for (key, epoch) in self.0.locked().iter_mut() {
            if key == group || key.starts_with(&thread_prefix) {
                *epoch += 1;
                stopped = true;
            }
        }
        if !stopped {
            return;
        }
        let Ok(Some(row)) = hub.store.bot(group) else { return };
        for member in &row.config.members {
            let _ = hub.send_cmd(member, Cmd::CancelGroup { chat: group.to_owned() });
        }
    }
}

/// Existing, distinct agent bots (not groups, not the group itself).
pub fn valid_members(store: &Store, group_id: &str, ids: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for id in ids.iter().map(|i| i.trim()) {
        if id.is_empty() || id == group_id || out.iter().any(|o| o == id) {
            continue;
        }
        match store.bot(id)? {
            Some(row) if !row.deleted && row.config.kind == BotKind::Agent => out.push(id.to_owned()),
            Some(row) if row.config.is_group() => bail!("a group can't be a member of another group"),
            _ => {}
        }
    }
    if out.is_empty() {
        bail!("a group needs at least one bot");
    }
    Ok(out)
}

/// A group whose members are exactly `members`.
pub fn with_members(store: &Store, members: &[String]) -> Result<Option<String>> {
    let mut wanted = members.to_vec();
    wanted.sort();
    Ok(store.bots()?.into_iter().find_map(|row| {
        let mut have = row.config.members.clone();
        have.sort();
        (!row.deleted && row.config.is_group() && have == wanted).then_some(row.config.id)
    }))
}

/// The user wrote in a group lane: start a room turn there (ending one still running).
pub fn start(hub: &Arc<Hub>, group: &BotConfig, lane: Lane) {
    let epoch = hub.groups.next(&lane);
    let (hub, group_id) = (hub.clone(), group.id.clone());
    tokio::spawn(async move {
        if let Err(error) = run(&hub, &group_id, &lane, epoch).await {
            tracing::warn!(group = group_id, error = format!("{error:#}"), "room turn failed");
        }
    });
}

async fn run(hub: &Arc<Hub>, group_id: &str, lane: &Lane, epoch: u64) -> Result<()> {
    let mut replies = 0;
    let mut last_reply: Option<(String, String)> = None;
    for round in 0..MAX_ROUNDS {
        let group = hub.store.bot(group_id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("group deleted"))?.config;
        let members = members(hub, &group);
        let since_user = since_last_user_message(hub, lane)?;
        let mut speakers = responders(&members, &since_user);
        // Each round starts one member later, so the same bot doesn't always speak first.
        if !speakers.is_empty() {
            let offset = round % speakers.len();
            speakers.rotate_left(offset);
        }
        let mut spoke = 0;
        for member in &speakers {
            if !hub.groups.current(lane, epoch) || replies >= MAX_REPLIES {
                return Ok(());
            }
            let prompt = member_prompt(hub, &group, &members, member, lane)?;
            let (reply, answer) = oneshot::channel();
            if hub.send_cmd(&member.id, Cmd::Group(GroupTurn { lane: lane.clone(), prompt, reply })).is_err() {
                continue;
            }
            // A failed, cancelled or overdue member turn counts as a pass; its error shows in the room.
            if let Ok(Ok(Ok(Some(text)))) = tokio::time::timeout(crate::team::ASK_TIMEOUT, answer).await {
                spoke += 1;
                replies += 1;
                last_reply = Some((member.name.clone(), text));
            }
        }
        if spoke == 0 {
            break;
        }
    }
    if let Some((name, text)) = last_reply
        && let Some(group) = hub.store.bot(group_id)?.map(|r| r.config)
    {
        push::notify(hub, &group, &format!("{name} in {}", group.name), &text, AlertKind::Done);
    }
    Ok(())
}

fn members(hub: &Hub, group: &BotConfig) -> Vec<BotConfig> {
    group
        .members
        .iter()
        .filter_map(|id| hub.store.bot(id).ok().flatten())
        .filter(|r| !r.deleted)
        .map(|r| r.config)
        .collect()
}

/// The lane's messages since the user last wrote there, that message included.
fn since_last_user_message(hub: &Hub, lane: &Lane) -> Result<Vec<Entry>> {
    let recent = hub.store.messages_after(lane, 0, HISTORY_LINES)?;
    let start = recent.iter().rposition(|e| e.kind == EntryKind::User.as_str()).unwrap_or(0);
    Ok(recent[start..].to_vec())
}

/// Who answers: the members @-mentioned since the user's last message, or everyone
/// when nobody (or `@all` / `@everyone`) is.
fn responders<'a>(members: &'a [BotConfig], messages: &[Entry]) -> Vec<&'a BotConfig> {
    let texts: Vec<&str> = messages.iter().filter_map(|e| e.data["text"].as_str()).collect();
    let everyone = texts.iter().any(|t| mentions(t, "all") || mentions(t, "everyone"));
    let named: Vec<&BotConfig> =
        members.iter().filter(|m| texts.iter().any(|t| handles(&m.name).iter().any(|h| mentions(t, h)))).collect();
    if everyone || named.is_empty() { members.iter().collect() } else { named }
}

/// Ways to @-mention a bot: its full name, without spaces, or its first word (lowercased).
fn handles(name: &str) -> Vec<String> {
    let lower = name.trim().to_lowercase();
    let mut out = vec![lower.clone(), lower.split_whitespace().collect::<String>()];
    if let Some(first) = lower.split_whitespace().next() {
        out.push(first.to_owned());
    }
    out.retain(|h| !h.is_empty());
    out.dedup();
    out
}

/// `@handle` in `text`, not followed by more of a Latin word (`@al` isn't `@alice`).
/// CJK names are often followed straight by the message, so only ASCII letters end a match early.
fn mentions(text: &str, handle: &str) -> bool {
    let lower = text.to_lowercase();
    let needle = format!("@{handle}");
    lower
        .match_indices(&needle)
        .any(|(i, _)| lower[i + needle.len()..].chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_')))
}

fn member_prompt(hub: &Hub, group: &BotConfig, members: &[BotConfig], me: &BotConfig, lane: &Lane) -> Result<String> {
    let peers: Vec<&BotConfig> = members.iter().filter(|m| m.id != me.id).collect();
    let peer_names = if peers.is_empty() {
        "just the user".to_owned()
    } else {
        peers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
    };
    let about = group.description.trim();
    let room = if about.is_empty() { format!("\"{}\"", group.name) } else { format!("\"{}\" — {about}", group.name) };
    let mut out =
        vec![format!("You are {}, one participant in a group chat ({room}) with the user and {peer_names}.", me.name)];
    if !peers.is_empty() {
        out.push("Other participants in the room:".into());
        for p in &peers {
            let about =
                if p.description.trim().is_empty() { String::new() } else { format!(" ({})", p.description.trim()) };
            out.push(format!("- {}{about}", p.name));
        }
    }
    out.push(format!(
        "You have your full toolkit here. Do the work first, then answer: your final reply of this turn is \
         the one message the room sees. Keep it short and conversational. @-mention another participant only \
         when you need them to act. If you have nothing new worth adding, reply exactly \"{PASS}\". Never \
         reveal private one-on-one context."
    ));
    out.push(String::new());

    let seen = hub.store.last_spoke(lane, &me.id);
    let mut lines: Vec<String> = vec![];
    let mut header = format!("[Group chat: \"{}\" - with {peer_names}]", group.name);
    if let Some(root) = lane.thread.as_deref().and_then(|r| hub.store.entry(r)) {
        header = format!("[Group chat: \"{}\", in a thread - with {peer_names}]", group.name);
        if seen == 0 {
            lines.push(format!("(thread started on) {}", line(hub, &root, me)));
        }
    }
    for e in hub.store.messages_after(lane, seen, HISTORY_LINES)? {
        lines.push(line(hub, &e, me));
    }
    out.push(header);
    if lines.is_empty() {
        out.push("No new messages in the room since your last turn.".into());
    } else {
        out.push("New messages in the room (oldest first):".into());
        out.extend(lines);
    }
    out.push(String::new());
    out.push(format!(
        "It's your turn, {}. Reply to the room if you have something worth adding, or reply exactly \"{PASS}\" if you don't.",
        me.name
    ));
    Ok(out.join("\n"))
}

fn line(hub: &Hub, e: &Entry, me: &BotConfig) -> String {
    let text = e.data["text"].as_str().unwrap_or_default();
    if e.kind == EntryKind::User.as_str() {
        return format!("User: {text}");
    }
    let author = e.data["author"].as_str().unwrap_or_default();
    let text = crate::acp::truncate(text, LINE_CHARS);
    if author == me.id { format!("{} (you): {text}", me.name) } else { format!("{}: {text}", hub.bot_name(author)) }
}

/// A member's final text is a pass (the room doesn't see it).
pub fn is_pass(text: &str) -> bool {
    let t = text.trim().trim_matches(|c: char| c == '"' || c == '.' || c == '*').to_lowercase();
    t.is_empty() || t == PASS || t == "pass"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bot(id: &str, name: &str) -> BotConfig {
        serde_json::from_value(json!({"id": id, "name": name, "backend": "claude", "cwd": "/tmp"})).expect("valid bot")
    }

    fn said(text: &str) -> Entry {
        Entry {
            id: "e".into(),
            seq: 1,
            bot_id: "g".into(),
            thread_id: None,
            rev: 1,
            kind: "user".into(),
            turn: 1,
            data: json!({"text": text}),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn mentions_pick_responders_and_no_mention_means_everyone() {
        let members = [bot("a", "Alice Chen"), bot("b", "Bob"), bot("c", "尼采課")];
        let ids = |m: &[BotConfig], text: &str| -> Vec<String> {
            responders(m, &[said(text)]).into_iter().map(|b| b.id.clone()).collect()
        };
        assert_eq!(ids(&members, "hi all"), ["a", "b", "c"]);
        assert_eq!(ids(&members, "@bob can you look"), ["b"]);
        assert_eq!(ids(&members, "@alicechen and @Alice"), ["a"]);
        assert_eq!(ids(&members, "@尼采課幫我整理"), ["c"], "CJK names can run into the message");
        assert_eq!(ids(&members, "@bobby?"), ["a", "b", "c"], "a longer word isn't a mention");
        assert_eq!(ids(&members, "@all go"), ["a", "b", "c"]);
    }

    use crate::devices::Caller;
    use crate::hub::BotStatus;
    use std::path::PathBuf;
    use std::time::Duration;

    /// A host with agent bots `a` and `b` (the scripted fixture agent) and a group of both.
    struct Room {
        hub: Arc<Hub>,
        dir: PathBuf,
        group: String,
    }

    impl Room {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("codync-group-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Store::open(&dir.join("test.db")).unwrap();
            let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
            for id in ["a", "b"] {
                let cwd = dir.join(id);
                std::fs::create_dir_all(&cwd).unwrap();
                let cfg: BotConfig = serde_json::from_value(json!({
                    "id": id, "name": id, "backend": "fixture", "cwd": cwd,
                    "command": format!("python3 -u '{}'", agent.display()), "notify": false,
                }))
                .unwrap();
                store.save_bot(&cfg).unwrap();
            }
            let hub = Hub::new(
                store,
                "test-host".into(),
                crate::identity::Identity::load_or_create(&dir).unwrap(),
                "test-token".into(),
                19222,
            );
            hub.start().unwrap();
            let group = hub
                .create_bot(
                    serde_json::from_value(
                        json!({"id": "", "kind": "group", "name": "Crew", "description": "Ship the release", "members": ["a", "b", "a"]}),
                    )
                    .unwrap(),
                )
                .unwrap();
            let group = group["id"].as_str().unwrap().to_owned();
            Self { hub, dir, group }
        }

        async fn send(&self, text: &str, thread: Option<&str>) -> Entry {
            let sent = crate::api::dispatch(
                &self.hub,
                &Caller::Local,
                "send",
                json!({"botId": self.group, "text": text, "threadId": thread}),
            )
            .await
            .unwrap();
            serde_json::from_value::<serde_json::Value>(sent["entry"].clone())
                .ok()
                .and_then(|e| self.hub.store.entry(e["id"].as_str()?))
                .unwrap()
        }

        fn replies(&self, lane: &Lane) -> Vec<(String, String)> {
            self.hub
                .store
                .messages_after(lane, 0, 100)
                .unwrap()
                .into_iter()
                .filter(|e| e.kind == "agent")
                .map(|e| (e.data["author"].as_str().unwrap().to_owned(), e.data["text"].as_str().unwrap().to_owned()))
                .collect()
        }

        async fn settle(&self) {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    let idle = ["a", "b"].iter().all(|b| self.hub.runtime(b).status == BotStatus::Idle);
                    if idle && self.hub.bots_json(0).unwrap().iter().all(|b| b["status"] == "idle") {
                        break;
                    }
                }
            })
            .await
            .expect("room settles");
        }

        async fn shutdown(self) {
            self.hub.shutdown().await;
            std::fs::remove_dir_all(self.dir).unwrap();
        }
    }

    #[tokio::test]
    async fn room_turns_follow_mentions_and_end_when_everyone_passes() {
        let room = Room::new();
        let row = room.hub.store.bot(&room.group).unwrap().unwrap();
        assert_eq!(row.config.members, ["a", "b"], "members are deduplicated");
        let again = room
            .hub
            .create_bot(
                serde_json::from_value(json!({"id": "", "kind": "group", "name": "Dup", "members": ["b", "a"]}))
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(again["id"], room.group, "the same bots reopen their group");

        let main = Lane::main(&room.group);
        room.send("hello everyone", None).await;
        room.settle().await;
        // `a` answers the user; `b` then sees a bot spoke last and passes; round two is all passes.
        assert_eq!(room.replies(&main), [("a".to_owned(), "reply: group".to_owned())]);

        room.send("@b your turn", None).await;
        room.settle().await;
        assert_eq!(room.replies(&main).last().unwrap().0, "b", "only the mentioned bot answers");
        assert_eq!(room.replies(&main).len(), 2);
        let listed = room.hub.bot_value_for_test(&room.group);
        assert_eq!(listed["lastMessage"], "b: reply: group");
        let prompts = std::fs::read_to_string(room.dir.join("a/prompts.jsonl")).unwrap();
        assert!(
            prompts.contains("Ship the release) with the user"),
            "members are told what the room is for: {prompts}"
        );
        // The members' own chats stay private: the room's entries live in the group.
        assert!(room.hub.store.messages_after(&Lane::main("a"), 0, 100).unwrap().is_empty());
        room.shutdown().await;
    }

    #[tokio::test]
    async fn threads_stay_out_of_the_main_chat_and_summarize_on_their_root() {
        let room = Room::new();
        let first = room.send("@a start here", None).await;
        room.settle().await;
        room.send("@a in the thread", Some(&first.id)).await;
        room.settle().await;
        let thread = Lane::in_thread(&room.group, &first.id);
        assert_eq!(room.replies(&thread).len(), 1);
        assert_eq!(room.replies(&Lane::main(&room.group)).len(), 1, "the thread's reply isn't in the main chat");
        let summary = room.hub.store.entry(&first.id).unwrap().data["thread"].clone();
        assert_eq!(summary["count"], 2);
        assert_eq!(summary["authors"], json!(["user", "a"]));

        // A thread in a bot's own chat gets a session of its own, told where it branched off.
        let dm = crate::api::dispatch(&room.hub, &Caller::Local, "send", json!({"botId": "b", "text": "main"}))
            .await
            .unwrap();
        room.settle().await;
        let dm_root = dm["entry"]["id"].as_str().unwrap().to_owned();
        crate::api::dispatch(
            &room.hub,
            &Caller::Local,
            "send",
            json!({"botId": "b", "text": "aside", "threadId": dm_root}),
        )
        .await
        .unwrap();
        room.settle().await;
        let prompts = std::fs::read_to_string(room.dir.join("b/prompts.jsonl")).unwrap();
        assert!(prompts.lines().last().unwrap().contains("[Thread]"), "{prompts}");
        let replies = room.hub.store.thread("b", &dm_root, 10).unwrap();
        assert_eq!(replies.iter().filter(|e| e.data["final"] == true).count(), 1);
        assert!(
            crate::api::dispatch(
                &room.hub,
                &Caller::Local,
                "send",
                json!({"botId": "b", "text": "x", "threadId": replies[0].id})
            )
            .await
            .is_err(),
            "threads are flat"
        );
        room.shutdown().await;
    }

    #[test]
    fn pass_is_recognized_loosely() {
        for t in ["(pass)", " (pass) ", "\"(pass)\"", "pass.", ""] {
            assert!(is_pass(t), "{t:?}");
        }
        assert!(!is_pass("I'll pass this to Bob"));
    }
}
