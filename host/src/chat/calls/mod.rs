//! Voice calls, owned by the host (`docs/plans/voice-architecture.md`, stage 1). A client starts a call
//! (`startCall`): the host hands it the provider's credential, the voice's instructions and its tools, runs the
//! voice's tool calls (`callTool`, [`tools`]), keeps the transcript as it is spoken (`callLines`, and whatever lines
//! the app still holds ride along with `callTool` and `endCall`) and writes the call's line when it ends (`endCall`).
//! What a call sends the bot carries its id (`call`), and so do the bot's replies while it lasts: the apps show those
//! only in the call's conversation, which its line opens. While it lasts, the bot's replies, approvals, failures
//! and progress reach the voice from here ([`announce`]).

mod announce;
pub mod context;
mod project;
mod speech;
mod sweep;
pub mod tools;

pub use announce::replay;
pub use sweep::sweep_loop;

use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, EntryKind, Lane, Store, now_ms};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use sweep::mark_open;

const MAX_LINES: usize = 500;
const MAX_LINE_CHARS: usize = 2_000;
/// What the title writer reads of a call, from its end.
const TITLE_SOURCE_CHARS: usize = 8_000;
const MAX_TITLE_CHARS: usize = 60;
/// The call line's text until a title is written.
pub const UNTITLED: &str = "Voice chat";
/// The most of a call's messages the host reads or hands an app (`callEntries`), newest first.
pub const CALL_ENTRIES: i64 = 500;

/// One read-modify-write of a call's state at a time (lines and hand-offs arrive together).
static STATE: Mutex<()> = Mutex::new(());

/// Who said a line of a call: the user, or the voice answering them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    User,
    Voice,
}

/// One line of what was said on a call, with when (ms, the app's clock) and the app's id for it, so a line sent
/// again (its first answer was lost) is kept once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallLine {
    pub from: Speaker,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// A call in progress, kept in `kv` so a host restart or a dropped client loses nothing: its bot, the transcript
/// so far and how much of it the bot has heard (`handoff`).
#[derive(Serialize, Deserialize)]
struct CallState {
    bot: String,
    lines: Vec<CallLine>,
    handoff: usize,
    started: i64,
    /// When the call last heard from its app.
    #[serde(default)]
    last: i64,
    /// The device that holds the call (`Caller::device_key`): only it hears what the voice should say.
    #[serde(default)]
    owner: String,
    /// What carries its voice now: how what it hears is worded ([`announce`]).
    #[serde(default)]
    mode: CallMode,
}

impl CallState {
    fn new(bot: &str) -> Self {
        let now = now_ms();
        Self {
            bot: bot.to_owned(),
            lines: Vec::new(),
            handoff: 0,
            started: now,
            last: now,
            owner: String::new(),
            mode: CallMode::default(),
        }
    }
}

/// What carries a call's voice: a realtime provider, speech to text and back, or the device's own speech.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CallMode {
    #[default]
    Realtime,
    Speech,
    Device,
}

/// A call id from an app: a UUID, or anything as plain.
pub fn checked(id: &str) -> Result<&str> {
    let ok = !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok { Ok(id) } else { bail!("`{id}` isn't a call id") }
}

fn ended_key(call: &str) -> String {
    format!("call.ended.{call}")
}

fn state_key(call: &str) -> String {
    format!("call.{call}")
}

fn state(store: &Store, call: &str) -> Option<CallState> {
    store.kv_get(&state_key(call)).and_then(|s| serde_json::from_str(&s).ok())
}

fn save(store: &Store, call: &str, s: &CallState) -> Result<()> {
    store.kv_set(&state_key(call), &serde_json::to_string(s)?)
}

/// A call's state for an update, made (and kept among the open calls) when a message of it arrives first (its
/// `startCall` lost); `None` once it ended.
fn begun(store: &Store, bot: &str, call: &str) -> Result<Option<CallState>> {
    if store.kv_get(&ended_key(call)).is_some() {
        return Ok(None);
    }
    if let Some(mut s) = state(store, call) {
        s.last = now_ms();
        return Ok(Some(s));
    }
    mark_open(store, call, true)?;
    Ok(Some(CallState::new(bot)))
}

/// A message said on call `call` reached `bot`'s chat (`send`) from device `owner`: the call is alive, and the host
/// knows of it even if its `startCall` never came (then it's an on-device or speech call: only those send
/// messages), and its voice keeps hearing the bot's work.
pub fn heard(hub: &Arc<Hub>, bot: &str, call: &str, owner: &str) -> Result<()> {
    {
        let _one = STATE.locked();
        let known = state(&hub.store, call).is_some();
        match begun(&hub.store, bot, call)? {
            Some(mut s) if s.bot == bot => {
                if !known {
                    s.mode = CallMode::Device;
                }
                if s.owner.is_empty() {
                    owner.clone_into(&mut s.owner);
                }
                save(&hub.store, call, &s)?;
            }
            _ => return Ok(()),
        }
    }
    announce::watch(hub, call, bot);
    Ok(())
}

/// As the host starts: the calls still on hear the bot's work again (the sweep ends abandoned ones).
pub fn rewatch(hub: &Arc<Hub>) {
    for call in sweep::open_calls(&hub.store) {
        if let Some(s) = state(&hub.store, &call) {
            announce::watch(hub, &call, &s.bot);
        }
    }
}

/// The call's bot, if the call is in progress.
fn bot_of(store: &Store, call: &str) -> Option<String> {
    state(store, call).map(|s| s.bot)
}

/// Whether `call` is in progress (started, not ended).
pub fn is_live(store: &Store, call: &str) -> bool {
    store.kv_get(&ended_key(call)).is_none() && state(store, call).is_some()
}

/// The call a turn answers: the one every message it answers came from, while that call lasts. A typed message
/// in the batch makes its reply visible.
pub fn of_turn(store: &Store, entry_ids: &[String]) -> Option<String> {
    let mut calls = entry_ids.iter().map(|id| store.entry(id).and_then(|e| e.data["call"].as_str().map(str::to_owned)));
    let first = calls.next()??;
    (calls.all(|c| c.as_deref() == Some(first.as_str())) && store.kv_get(&ended_key(&first)).is_none()).then_some(first)
}

/// Who wrote a chat message, as the voice and the title writer name them; `None` for a bot.
fn writer(e: &Entry) -> Option<&'static str> {
    match (e.kind.as_str(), e.data["from"].as_str()) {
        ("user", Some("voice")) => Some("Voice (relayed)"),
        ("user", _) => Some("User"),
        _ => None,
    }
}

/// Lines of what was said, from an app: `from` `user` (what the user said) or `voice`, with `at` (ms).
fn transcript(raw: &Value) -> Vec<CallLine> {
    raw.as_array()
        .into_iter()
        .flatten()
        .filter_map(|line| {
            let line = CallLine::deserialize(line).ok()?;
            let text = line.text.trim();
            (!text.is_empty()).then(|| CallLine {
                from: line.from,
                text: crate::agent::acp::truncate(text, MAX_LINE_CHARS),
                at: line.at,
                id: line.id.filter(|id| checked(id).is_ok()),
            })
        })
        .take(MAX_LINES)
        .collect()
}

/// Adds what was just said to the call, keeping its last [`MAX_LINES`]. A line it already has (an app sending it
/// again) is skipped.
fn append(s: &mut CallState, lines: Vec<CallLine>) {
    for line in lines {
        if line.id.is_some() && s.lines.iter().any(|l| l.id == line.id) {
            continue;
        }
        s.lines.push(line);
    }
    let over = s.lines.len().saturating_sub(MAX_LINES);
    s.lines.drain(..over);
    // What the bot heard moves with the lines dropped from the front.
    s.handoff = s.handoff.saturating_sub(over);
}

/// `startCall` from device `owner`: the call is on (again, for a session the provider replaced: its transcript
/// stays). Hands back what the client needs: the voice's instructions and tools, and a realtime provider's credential.
pub async fn start(hub: &Arc<Hub>, b: &Value, owner: &str) -> Result<Value> {
    let bot = b["botId"].as_str().ok_or_else(|| anyhow!("`botId` is required"))?;
    let call = checked(b["callId"].as_str().unwrap_or_default())?;
    let mode = if b["mode"].is_null() { CallMode::Realtime } else { CallMode::deserialize(&b["mode"])? };
    let cfg = hub.store.bot(bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    let so_far = {
        let _one = STATE.locked();
        match begun(&hub.store, bot, call)? {
            Some(s) if s.bot != bot => bail!("that call belongs to another bot"),
            // A session the provider replaced, maybe on another device or in another mode now.
            Some(s) => {
                let s = CallState { owner: owner.to_owned(), mode, ..s };
                save(&hub.store, call, &s)?;
                s.lines
            }
            None => bail!("that call has ended"),
        }
    };
    announce::watch(hub, call, bot);
    let provider = b["provider"].as_str().map(crate::voice::Provider::parse).transpose()?;
    let mut out = json!({
        "instructions": context::instructions(hub, &cfg, provider, b["language"].as_str(), &so_far).await,
        "tools": tools::declarations(),
    });
    if let (Some(p), CallMode::Realtime) = (provider, mode) {
        // The provider's key is in the vault.
        crate::market::vault::unlock(hub.clone()).await?;
        let (model, voice) = (b["model"].as_str().unwrap_or_default(), b["voice"].as_str().unwrap_or_default());
        out["credential"] = crate::voice::session(&hub.store, p, model, voice).await?["credential"].clone();
    }
    Ok(out)
}

/// `callLines`: what was just said on the call (`raw`, see [`transcript`]). Its voice keeps hearing the bot's work
/// (again, after a host restart).
pub fn add_lines(hub: &Arc<Hub>, call: &str, raw: &Value) -> Result<()> {
    let lines = transcript(raw);
    let _one = STATE.locked();
    let Some(mut s) = state(&hub.store, call) else {
        return if lines.is_empty() { Ok(()) } else { Err(anyhow!("no such call")) };
    };
    announce::watch(hub, call, &s.bot);
    if lines.is_empty() {
        return Ok(());
    }
    s.last = now_ms();
    append(&mut s, lines);
    save(&hub.store, call, &s)
}

/// What the user said since the bot last heard from the call, word for word; the bot hears it now.
fn take_said(store: &Store, call: &str) -> Result<String> {
    let _one = STATE.locked();
    let mut s = state(store, call).ok_or_else(|| anyhow!("no such call"))?;
    s.last = now_ms();
    let unheard = s.lines.get(s.handoff..).unwrap_or_default();
    let said =
        unheard.iter().filter(|l| l.from == Speaker::User).map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
    s.handoff = s.lines.len();
    save(store, call, &s)?;
    Ok(said)
}

/// `endCall`: the rest of the transcript (`raw`), the call's line in the chat, then a title written for it. A call
/// ends once: an app trying again changes nothing.
pub fn end(hub: &Arc<Hub>, bot: &str, call: &str, seconds: u64, raw: &Value) -> Result<Option<Entry>> {
    end_when(hub, bot, call, raw, |_| Some(seconds))
}

/// Ends `call` if `length`, asked under the lock that marks it ended, gives its length in seconds; `None` keeps it
/// going. The sweep decides there, so a call its app resumed meanwhile isn't ended.
fn end_when(
    hub: &Arc<Hub>,
    bot: &str,
    call: &str,
    raw: &Value,
    length: impl FnOnce(Option<&CallState>) -> Option<u64>,
) -> Result<Option<Entry>> {
    let (s, seconds) = {
        let _one = STATE.locked();
        if hub.store.kv_get(&ended_key(call)).is_some() {
            return Ok(None);
        }
        let mut s = state(&hub.store, call);
        let Some(seconds) = length(s.as_ref()) else { return Ok(None) };
        if let Some(s) = s.as_mut() {
            append(s, transcript(raw));
        }
        hub.store.kv_set(&ended_key(call), "1")?;
        hub.store.kv_delete(&state_key(call))?;
        mark_open(&hub.store, call, false)?;
        (s, seconds)
    };
    let bot = s.as_ref().map_or(bot, |s| s.bot.as_str()).to_owned();
    let cfg = hub.store.bot(&bot)?.filter(|r| !r.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    let mut data = json!({"text": UNTITLED, "callSeconds": seconds, "call": call});
    if let Some(s) = s.filter(|s| !s.lines.is_empty()) {
        data["transcript"] = serde_json::to_value(&s.lines)?;
    }
    let entry = hub.add_entry(&Lane::main(&bot), EntryKind::Notice, hub.store.max_turn(&bot), &data);
    // Replies from here on show in the chat again, and dreaming reads the call (its line is in the store by now)
    // soon. A group's members answer it.
    let actors = if cfg.is_group() { cfg.members.clone() } else { vec![cfg.id.clone()] };
    for actor in actors {
        if let Err(e) = hub.send_cmd(&actor, Cmd::CallEnded { call: call.to_owned() }) {
            tracing::debug!(bot = actor, error = format!("{e:#}"), "no bot actor to tell the call ended");
        }
    }
    let Some(entry) = entry else { return Ok(None) };
    let (hub, line) = (hub.clone(), entry.clone());
    tokio::spawn(async move {
        if let Err(e) = title(&hub, &cfg, &line).await {
            tracing::info!(bot = %cfg.id, error = format!("{e:#}"), "couldn't title the call");
        }
    });
    Ok(Some(entry))
}

/// The call as a conversation, oldest first: the voice's transcript and the messages the call carried.
fn conversation(store: &Store, bot: &str, line: &Entry) -> Vec<(String, String)> {
    let Some(call) = line.data["call"].as_str() else { return Vec::new() };
    let mut said: Vec<(i64, String, String)> = transcript(&line.data["transcript"])
        .into_iter()
        .map(|l| {
            let who = match l.from {
                Speaker::User => "User",
                Speaker::Voice => "Voice",
            };
            (l.at.unwrap_or(line.created_at), who.to_owned(), l.text)
        })
        .collect();
    for e in store.call_entries(bot, call, CALL_ENTRIES).unwrap_or_default() {
        let who = writer(&e).unwrap_or("Bot");
        said.push((e.created_at, who.to_owned(), e.data["text"].as_str().unwrap_or_default().to_owned()));
    }
    // ponytail: phone and computer clocks merged as they are; a skewed phone clock reorders a line or two.
    said.sort_by_key(|(at, ..)| *at);
    said.into_iter().map(|(_, who, text)| (who, text)).collect()
}

async fn title(hub: &Arc<Hub>, cfg: &BotConfig, line: &Entry) -> Result<()> {
    let said = conversation(&hub.store, &cfg.id, line);
    if !said.iter().any(|(who, _)| who == "User") {
        return Ok(());
    }
    // The call's end, whole lines, within the budget.
    let mut lines: Vec<String> = said
        .iter()
        .rev()
        .map(|(who, text)| format!("{who}: {}", text.trim()))
        .scan(0, |len, line| {
            *len += line.len() + 1;
            (*len <= TITLE_SOURCE_CHARS).then_some(line)
        })
        .collect();
    lines.reverse();
    let body = lines.join("\n");
    let raw =
        crate::chat::oneshot::one_shot(hub, cfg, TITLE_PROMPT, &format!("The call, oldest first:\n\n{body}")).await?;
    let Some(name) = parse_title(&raw) else { return Ok(()) };
    let Some(mut e) = hub.store.entry(&line.id) else { return Ok(()) };
    e.data["text"] = name.into();
    hub.set_entry(&line.id, &e.data);
    Ok(())
}

const TITLE_PROMPT: &str = "\
You title a voice call between the user and their assistant, for the call's line in the chat.
Output exactly one line:
title: <what the call was about, 2-6 words>
Write it in the language and script the user speaks (Traditional and Simplified Chinese are different). No quotes, no emoji, no trailing punctuation.
Output exactly NONE if the call has nothing to sum up (only greetings or tests).";

fn parse_title(raw: &str) -> Option<String> {
    let title = raw.lines().find_map(|line| {
        let (tag, rest) = line.trim().trim_start_matches(['-', '*', ' ']).split_once(':')?;
        tag.trim().eq_ignore_ascii_case("title").then(|| rest.trim().trim_matches(['"', '“', '”', '「', '」']))
    })?;
    let title = title.trim_end_matches(['.', '。']).trim();
    (!title.is_empty() && !title.eq_ignore_ascii_case("none") && title.chars().count() <= MAX_TITLE_CHARS)
        .then(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_ids_are_plain() {
        assert!(checked("6f1c2e0a-1b2c-4d5e-8f90-123456789abc").is_ok());
        assert!(checked("").is_err());
        assert!(checked("a/b").is_err());
        assert!(checked(&"a".repeat(65)).is_err());
    }

    #[test]
    fn transcripts_keep_only_lines_of_the_user_or_the_voice() {
        let raw = json!([
            {"from": "user", "text": "  幫我看 git status ", "at": 5},
            {"from": "voice", "text": "Checking."},
            {"from": "bot", "text": "spoofed"},
            {"from": "user", "text": "   "},
            {"text": "no speaker"},
        ]);
        let lines = serde_json::to_value(transcript(&raw)).unwrap();
        assert_eq!(
            lines,
            json!([{"from": "user", "text": "幫我看 git status", "at": 5}, {"from": "voice", "text": "Checking."}])
        );
        let long: Vec<Value> = (0..MAX_LINES + 5).map(|_| json!({"from": "user", "text": "x"})).collect();
        assert_eq!(transcript(&Value::Array(long)).len(), MAX_LINES);
    }

    #[test]
    fn titles_are_parsed() {
        assert_eq!(parse_title("title: 「規劃 2.26 發佈」").as_deref(), Some("規劃 2.26 發佈"));
        assert_eq!(parse_title("Sure.\n- Title: Fix the login bug.").as_deref(), Some("Fix the login bug"));
        assert_eq!(parse_title("NONE"), None);
        assert_eq!(parse_title(&format!("title: {}", "x".repeat(61))), None);
    }

    #[test]
    fn a_call_reads_as_one_conversation_in_order() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let chat = Lane::main("b");
        store
            .insert_entry(&chat, EntryKind::User, 1, &json!({"text": "check git", "call": "c", "from": "voice"}))
            .unwrap();
        store.insert_entry(&chat, EntryKind::Agent, 1, &json!({"text": "Clean.", "final": true, "call": "c"})).unwrap();
        store.insert_entry(&chat, EntryKind::Agent, 1, &json!({"text": "narration", "call": "c"})).unwrap();
        store.insert_entry(&chat, EntryKind::User, 2, &json!({"text": "typed, not the call's"})).unwrap();
        let data = json!({"text": UNTITLED, "call": "c", "transcript": [
            {"from": "user", "text": "看一下 git", "at": 0},
            {"from": "voice", "text": "It's clean.", "at": i64::MAX},
        ]});
        let line = store.insert_entry(&chat, EntryKind::Notice, 2, &data).unwrap();
        let said: Vec<String> =
            conversation(&store, "b", &line).into_iter().map(|(who, text)| format!("{who}: {text}")).collect();
        assert_eq!(said, ["User: 看一下 git", "Voice (relayed): check git", "Bot: Clean.", "Voice: It's clean."]);
    }

    #[tokio::test]
    async fn a_call_keeps_what_was_said_and_hands_the_users_words_to_the_bot() {
        let hub = context::tests::hub();
        let id = uuid::Uuid::new_v4().to_string();
        // No agent to run: the title writer gives up at once.
        let cfg = BotConfig { backend: "none".into(), ..context::tests::bot(&id, "Mika", "/tmp") };
        hub.store.save_bot(&cfg).unwrap();
        let started = start(&hub, &json!({"botId": id, "callId": "c1", "mode": "device"}), "phone").await.unwrap();
        assert!(started["instructions"].as_str().unwrap().contains("voice relay for Mika"));
        assert!(started["tools"].as_array().unwrap().iter().any(|t| t["name"] == "search_memory"));
        assert!(started.get("credential").is_none(), "an on-device call has no provider");
        add_lines(&hub, "c1", &json!([{"from": "user", "text": "首頁", "at": 1}])).unwrap();
        // A provider session that replaces a dropped one picks up from what was said.
        let again = start(&hub, &json!({"botId": id, "callId": "c1", "mode": "device"}), "phone").await.unwrap();
        let instructions = again["instructions"].as_str().unwrap();
        assert!(instructions.contains("This call so far") && instructions.contains("User: 首頁"), "{instructions}");

        // The rest of what the user said comes with the tool call that hands it on.
        let unsent = json!([{"from": "user", "text": "改成藍色", "at": 2}, {"from": "voice", "text": "好"}]);
        let sent = tools::call(&hub, "c1", "send_to_bot", &json!({"text": "Make the home page blue"}), &unsent).await;
        // No bot actor runs in this test, so delivery fails; the message is in the chat all the same.
        assert!(sent["error"].is_string() || sent["sent"] == true);
        let relayed = hub.store.call_entries(&id, "c1", CALL_ENTRIES).unwrap();
        assert_eq!(relayed.len(), 1);
        assert_eq!(relayed[0].data["from"], "voice");
        let text = relayed[0].data["text"].as_str().unwrap();
        assert!(text.starts_with("Make the home page blue") && text.contains("«首頁 改成藍色»"), "{text}");
        assert_eq!(take_said(&hub.store, "c1").unwrap(), "", "the bot already heard it");

        let last = json!([{"from": "voice", "text": "Done."}]);
        let line = end(&hub, &id, "c1", 30, &last).unwrap().unwrap();
        assert_eq!((line.data["callSeconds"].as_u64(), line.data["call"].as_str()), (Some(30), Some("c1")));
        assert_eq!(line.data["transcript"].as_array().map(Vec::len), Some(4), "with the lines the app still held");
        assert!(end(&hub, &id, "c1", 31, &last).unwrap().is_none(), "an app trying again ends it once");
        assert!(bot_of(&hub.store, "c1").is_none(), "an ended call takes no more lines");
        assert!(start(&hub, &json!({"botId": id, "callId": "c1", "mode": "device"}), "phone").await.is_err());
        assert!(of_turn(&hub.store, &[relayed[0].id.clone()]).is_none(), "its replies show again");
        assert!(tools::call(&hub, "c1", "bot_status", &json!({}), &Value::Null).await["error"].is_string());
    }

    #[tokio::test]
    async fn a_line_sent_again_is_kept_once() {
        let hub = context::tests::hub();
        heard(&hub, "b", "c1", "phone").unwrap();
        let batch = json!([{"from": "user", "text": "hi", "at": 1, "id": "l1"}, {"from": "voice", "text": "Hello.", "id": "l2"}]);
        add_lines(&hub, "c1", &batch).unwrap();
        // The answer to the first send was lost, so the app sends the batch again, and one more line.
        add_lines(&hub, "c1", &batch).unwrap();
        add_lines(&hub, "c1", &json!([{"from": "user", "text": "hi", "id": "l3"}])).unwrap();
        let s = state(&hub.store, "c1").unwrap();
        assert_eq!(
            (s.owner.as_str(), s.mode),
            ("phone", CallMode::Device),
            "a call first heard by a message is on-device"
        );
        let lines: Vec<String> = s.lines.iter().map(|l| l.text.clone()).collect();
        assert_eq!(lines, ["hi", "Hello.", "hi"], "repeats skipped, a new line with the same words kept");
    }

    #[test]
    fn a_turn_answers_a_call_only_when_all_its_messages_came_from_it() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let chat = Lane::main("b");
        let id = |data: Value| store.insert_entry(&chat, EntryKind::User, 1, &data).unwrap().id;
        let a = id(json!({"text": "one", "call": "c1"}));
        let b = id(json!({"text": "two", "call": "c1"}));
        let typed = id(json!({"text": "typed"}));
        assert_eq!(of_turn(&store, &[a.clone(), b.clone()]).as_deref(), Some("c1"));
        assert_eq!(of_turn(&store, &[a.clone(), typed]), None);
        store.kv_set(&ended_key("c1"), "1").unwrap();
        assert_eq!(of_turn(&store, &[a, b]), None);
    }
}
