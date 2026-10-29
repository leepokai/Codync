//! Voice calls, owned by the host (`docs/plans/voice-architecture.md`, stage 1). A client starts a call
//! (`startCall`): the host hands it the provider's credential, the voice's instructions and its tools, runs the
//! voice's tool calls (`callTool`, [`tools`]), keeps the transcript as it is spoken (`callLines`, and whatever lines
//! the app still holds ride along with `callTool` and `endCall`) and writes the call's line when it ends (`endCall`).
//! What a call sends the bot carries its id (`call`), and so do the bot's replies while it lasts: the apps show those
//! only in the call's conversation, which its line opens. While it lasts, the bot's replies, approvals, failures
//! and progress reach the voice from here ([`announce`]).

mod announce;
pub mod context;
pub mod live;
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

/// An original GPT-Live transcript delta, preserved when adjacent fragments form one displayed line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallFragment {
    pub delta: String,
    pub start_ms: i64,
    pub end_ms: i64,
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
    /// Provider timeline for a GPT-Live transcript fragment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fragments: Vec<CallFragment>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    live_session: Option<String>,
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
            live_session: None,
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
                start_ms: line.start_ms,
                end_ms: line.end_ms,
                fragments: line.fragments,
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
                let s = CallState { owner: owner.to_owned(), mode, live_session: None, ..s };
                save(&hub.store, call, &s)?;
                s.lines
            }
            None => bail!("that call has ended"),
        }
    };
    live::end(hub, call);
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
    let said = speech::user_words(unheard);
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
    live::end(hub, call);
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
mod tests;
