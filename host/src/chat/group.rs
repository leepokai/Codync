//! Group chats (Grok Bot's design): several bots and the user in one conversation.
//!
//! A group is a roster row (`BotKind::Group`) with a member list and its own
//! transcript, but no agent. When the user writes, the group runs a *room turn*:
//! up to [`MAX_ROUNDS`] rounds in which the members answer one at a time, each in
//! its **own** session (a member's history is unified across its chats), told
//! what was said since it last spoke. In the first round everyone answers unless
//! the user @-mentions someone; later rounds only ask the members another member
//! @-mentioned and who haven't spoken or been asked since. A member with nothing
//! to add says `(pass)`, and a round where nobody speaks ends the turn. A newer
//! message in the same lane (or Stop) moves the lane's epoch, which ends a running
//! room turn before its next speaker.

use crate::LockExt;
use crate::agent::bot::{Cmd, NoticeStyle};
use crate::hub::{BotStatus, Hub};
use crate::remote::push::{self, AlertKind};
use crate::store::{BotConfig, BotKind, Entry, EntryKind, Lane, Store};
use anyhow::{Result, anyhow, bail};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

const MAX_ROUNDS: usize = 3;
/// A member's turn that runs longer counts as a pass.
const MEMBER_TIMEOUT: Duration = Duration::from_secs(600);
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
    /// The newest message `prompt` shows (its `seq`): a reaction goes on what the member saw.
    pub shown: i64,
    /// The room turn that asked; a member skips it once a newer one started (see [`Rooms::current`]), unless `late`.
    pub epoch: u64,
    /// Queued while the member was busy elsewhere: it answers once free whatever the room did since, on a
    /// prompt built when it starts ([`refresh`]). Only the group's Stop, the room asking the member again, or the
    /// member leaving the group drops it (Stop in the member's own chat doesn't).
    pub late: bool,
    /// The reply the room sees, or `None` for a pass.
    pub reply: oneshot::Sender<Result<Option<String>>>,
}

/// Room turn epochs per group.
#[derive(Default)]
pub struct Rooms(Mutex<HashMap<String, u64>>);

impl Rooms {
    fn next(&self, lane: &Lane) -> u64 {
        let mut map = self.0.locked();
        let epoch = map.entry(lane.chat.clone()).or_default();
        *epoch += 1;
        *epoch
    }

    pub fn current(&self, lane: &Lane, epoch: u64) -> bool {
        self.0.locked().get(&lane.chat).copied() == Some(epoch)
    }

    /// Stop: ends the room turn in `group` and the member turns in it.
    pub fn stop(&self, hub: &Hub, group: &str) {
        match self.0.locked().get_mut(group) {
            Some(epoch) => *epoch += 1,
            None => return,
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
    let mut last_reply: Option<(String, String, String)> = None;
    // Members busy elsewhere: their turn waits in their queue, and they answer on their own once free.
    let mut later: Vec<String> = vec![];
    // The newest message each member was shown when last asked: what it passed on, it has seen.
    let mut asked: HashMap<String, i64> = HashMap::new();
    'rounds: for round in 0..MAX_ROUNDS {
        let group = group(hub, group_id)?;
        let members = members(hub, &group);
        let since_user = since_last_user_message(hub, lane)?;
        let mut speakers =
            if round == 0 { responders(&members, &since_user) } else { cited(&members, &since_user, &asked) };
        // Each round starts one member later, so the same bot doesn't always speak first.
        if !speakers.is_empty() {
            let offset = round % speakers.len();
            speakers.rotate_left(offset);
        }
        let mut spoke = 0;
        for member in &speakers {
            if !hub.groups.current(lane, epoch) {
                return Ok(());
            }
            // A full room turn still reports its last reply.
            if replies >= MAX_REPLIES {
                break 'rounds;
            }
            if later.contains(&member.id) {
                continue;
            }
            let (prompt, shown) = ask(hub, &group, &members, member, lane)?;
            asked.insert(member.id.clone(), shown);
            let (reply, answer) = oneshot::channel();
            let busy = busy_elsewhere(hub, &member.id, lane);
            let turn = GroupTurn { lane: lane.clone(), prompt, shown, epoch, late: busy, reply };
            if hub.send_cmd(&member.id, Cmd::Group(turn)).is_err() {
                continue;
            }
            // The room doesn't wait on a turn elsewhere (it can run for hours): it says so, and the member
            // answers here when it's done.
            if busy {
                let text = format!("{} is busy in another chat and will answer here when it's done.", member.name);
                let data = serde_json::json!({"text": text, "style": NoticeStyle::Info});
                hub.add_entry(lane, EntryKind::Notice, hub.store.max_turn(group_id), &data);
                later.push(member.id.clone());
                tokio::spawn(answer_later(hub.clone(), group_id.to_owned(), (*member).clone(), answer));
                continue;
            }
            // A failed, cancelled or overdue member turn counts as a pass; its error shows in the room.
            if let Ok(Ok(Ok(Some(text)))) = tokio::time::timeout(MEMBER_TIMEOUT, answer).await {
                spoke += 1;
                replies += 1;
                last_reply = Some((member.id.clone(), member.name.clone(), text));
            }
        }
        if spoke == 0 {
            break;
        }
    }
    if let Some((from, name, text)) = last_reply {
        alert(hub, group_id, &from, &name, &text)?;
    }
    Ok(())
}

/// A busy member's reply, after the room turn moved on: it alerts like the room's last reply. Dropping the member's
/// queued turn (see [`GroupTurn::late`]) ends this wait.
async fn answer_later(
    hub: Arc<Hub>,
    group_id: String,
    member: BotConfig,
    answer: oneshot::Receiver<Result<Option<String>>>,
) {
    if let Ok(Ok(Some(text))) = answer.await
        && let Err(error) = alert(&hub, &group_id, &member.id, &member.name, &text)
    {
        tracing::warn!(group = group_id, error = format!("{error:#}"), "late room reply not pushed");
    }
}

/// The "done" push for `name`'s reply in the group.
fn alert(hub: &Hub, group_id: &str, from: &str, name: &str, text: &str) -> Result<()> {
    if let Some(group) = hub.store.bot(group_id)?.map(|r| r.config) {
        push::notify(hub, &group, Some(from), &format!("{name} in {}", group.name), text, AlertKind::Done);
    }
    Ok(())
}

/// `member` runs a turn outside this room, so a room turn would queue behind it.
fn busy_elsewhere(hub: &Hub, member: &str, lane: &Lane) -> bool {
    let r = hub.runtime(member);
    matches!(r.status, BotStatus::Working | BotStatus::NeedsInput) && r.lane.as_ref() != Some(lane)
}

/// A late turn starts: it shows the room as it is now, not as it was when the turn was queued.
pub fn refresh(hub: &Hub, turn: &mut GroupTurn, me: &BotConfig) -> Result<()> {
    let group = group(hub, &turn.lane.chat)?;
    if !group.members.contains(&me.id) {
        bail!("no longer in the group");
    }
    (turn.prompt, turn.shown) = ask(hub, &group, &members(hub, &group), me, &turn.lane)?;
    Ok(())
}

fn group(hub: &Hub, id: &str) -> Result<BotConfig> {
    Ok(hub.store.bot(id)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("group deleted"))?.config)
}

/// `me`'s prompt for a turn in the room, and the newest message it shows (its `seq`).
fn ask(hub: &Hub, group: &BotConfig, members: &[BotConfig], me: &BotConfig, lane: &Lane) -> Result<(String, i64)> {
    let shown = hub.store.messages_after(lane, 0, 1)?.last().map_or(0, |e| e.seq);
    Ok((member_prompt(hub, group, members, me, lane)?, shown))
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

/// Who answers the user: the members @-mentioned or replied to since the user's last message, or
/// everyone when nobody (or `@all` / `@everyone`) is.
fn responders<'a>(members: &'a [BotConfig], messages: &[Entry]) -> Vec<&'a BotConfig> {
    let texts: Vec<&str> = messages.iter().filter_map(|e| e.data["text"].as_str()).collect();
    let everyone = texts.iter().any(|t| names_everyone(t));
    let replied = |m: &BotConfig| messages.iter().any(|e| e.data["replyTo"]["author"] == m.id.as_str());
    let named: Vec<&BotConfig> = members.iter().filter(|m| replied(m) || texts.iter().any(|t| names(t, m))).collect();
    if everyone || named.is_empty() { members.iter().collect() } else { named }
}

/// Who answers in a later round: the members another member @-mentioned since the user's last
/// message (`@all` / `@everyone`: all the others) and who haven't spoken or been `asked` since.
/// Nobody, and the room turn ends: a member pulls a teammate in by naming it.
fn cited<'a>(members: &'a [BotConfig], messages: &[Entry], asked: &HashMap<String, i64>) -> Vec<&'a BotConfig> {
    let mut cited_at: HashMap<&str, i64> = HashMap::new();
    // The newest message each member answered or passed on.
    let mut seen: HashMap<&str, i64> = asked.iter().map(|(id, seq)| (id.as_str(), *seq)).collect();
    for e in messages.iter().filter(|e| e.kind == EntryKind::Agent.as_str()) {
        let Some(author) = e.data["author"].as_str() else { continue };
        let at = seen.entry(author).or_default();
        *at = (*at).max(e.seq);
        let text = e.data["text"].as_str().unwrap_or_default();
        let everyone = names_everyone(text);
        for member in members.iter().filter(|m| m.id != author) {
            if everyone || names(text, member) {
                cited_at.insert(&member.id, e.seq);
            }
        }
    }
    let pending = |m: &&BotConfig| {
        cited_at.get(m.id.as_str()).is_some_and(|cited| seen.get(m.id.as_str()).is_none_or(|seen| seen < cited))
    };
    members.iter().filter(pending).collect()
}

/// `@all` or `@everyone` in `text`.
fn names_everyone(text: &str) -> bool {
    mentions(text, "all") || mentions(text, "everyone")
}

/// `member` @-mentioned in `text`.
fn names(text: &str, member: &BotConfig) -> bool {
    handles(&member.name).iter().any(|h| mentions(text, h))
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
         the one message the room sees. Keep it short and conversational. After this round, only the \
         participants someone @-mentions answer again: @-mention another participant when you need their \
         answer or action, and only then. If you have nothing new worth adding, reply exactly \"{PASS}\". Never \
         reveal private one-on-one context. Reactions are rare: react(emoji, to) only where a person would, \
         never to a question or a request, `to` naming whose newest message (\"user\" or a participant). With \
         nothing to add, a reaction and \"{PASS}\" agrees without a message."
    ));
    out.push(String::new());

    let seen = hub.store.last_spoke(lane, &me.id);
    let mut lines: Vec<String> = vec![];
    let header = format!("[Group chat: \"{}\" - with {peer_names}]", group.name);
    for e in hub.store.messages_after(lane, seen, HISTORY_LINES)? {
        lines.push(line(hub, group, &e, me));
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

fn line(hub: &Hub, group: &BotConfig, e: &Entry, me: &BotConfig) -> String {
    let text = e.data["text"].as_str().unwrap_or_default();
    let reacted = crate::chat::reactions::describe(hub, e);
    if e.kind == EntryKind::User.as_str() {
        // As in the member's own chat (its skills), with the files the group keeps.
        let root = crate::chat::uploads::root(group);
        let attachments = e.data["attachments"].as_array().map_or(&[][..], Vec::as_slice);
        let reply = crate::chat::replies::note(hub, &me.id, &e.data["replyTo"]).map(|n| format!("{n} "));
        return format!(
            "User: {}{}{reacted}",
            reply.unwrap_or_default(),
            crate::chat::prompt::of_message(&hub.store, &me.skills, &root, text, attachments)
        );
    }
    let author = e.data["author"].as_str().unwrap_or_default();
    let text = crate::agent::acp::truncate(text, LINE_CHARS);
    if author == me.id {
        format!("{} (you): {text}{reacted}", me.name)
    } else {
        format!("{}: {text}{reacted}", hub.bot_name(author))
    }
}

/// A member's final text is a pass (the room doesn't see it).
pub fn is_pass(text: &str) -> bool {
    let t = text.trim().trim_matches(|c: char| c == '"' || c == '.' || c == '*').to_lowercase();
    t.is_empty() || t == PASS || t == "pass"
}

#[cfg(test)]
mod tests;
