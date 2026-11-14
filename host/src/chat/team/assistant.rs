//! The user's cloud assistant (ChatGPT, Claude or another one they connected to Codync) through
//! the Codync connector at `mcp.codync.dev` (docs/reference/remote-relay.md §7.9). The cloud
//! forwards its tool calls over this computer's relay socket; they're answered only while the
//! user has turned cloud assistants on here, and only for this computer's own account. It
//! messages this computer's bots the way a bot does. Nothing can wake it, so the replies wait here
//! until it reads them (in memory: a restart forgets them).
//!
//! Agents on this computer that aren't Codync bots (a Claude Code or Codex session) get the same
//! three tools from `codync-host mcp network` (`agentCall`, loopback only), each under its own name.

use super::{
    Back, Exchange, MessageLifetime, RequestStatus, STOPPED, check_message, directory, message_args, remote,
    request_prompt, visible_bot,
};
use crate::LockExt;
use crate::hub::Hub;
use crate::network::Recipient;
use crate::remote::cloud::{self, CloudStatus};
use crate::store::{BotConfig, Store, now_ms};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

/// How it shows in a bot's chat, and its id there (`sourceBotId`).
pub(super) const NAME: &str = "Cloud assistant";
const ID: &str = "assistant";
/// What it signs letters to the user's other computers with (`cloud-assistant@desk` there). No
/// bot ever gets this address name.
pub(crate) const HANDLE: &str = "cloud-assistant";
/// kv: `on` lets the cloud assistant in.
const KV: &str = "cloud_assistants";
/// Requests still running at a time.
const MAX_RUNNING: usize = 16;
/// Requests remembered, oldest finished ones forgotten first; `read_replies` shows the newest.
const KEEP: usize = 50;
const SHOW: usize = 20;
const OFF: &str = "Cloud assistants are off on this computer. To let ChatGPT or Claude message its bots, \
turn on Cloud assistants in Codync's settings on it (or run `codync-host cloud --assistants on`); \
it has to be in your Codync account.";
const NOT_IN_ACCOUNT: &str = "This computer isn't in your Codync account.";

pub fn enabled(store: &Store) -> bool {
    store.kv_get(KV).as_deref() == Some("on")
}

/// Whether the cloud can bring the user's assistant here at all: this computer is in their Codync
/// account (a private relay has none).
pub fn available(hub: &Hub) -> bool {
    hub.cloud.status().owner.is_some()
}

/// A letter the cloud assistant sent through another of the user's computers: this computer's own
/// switches decide whether it reaches `bot`, as they do for its calls here.
pub(super) fn admit_letter(hub: &Hub, bot: &BotConfig) -> Result<()> {
    if !enabled(&hub.store) {
        bail!(OFF);
    }
    if !bot.reachable_by_assistants {
        return Err(closed(bot));
    }
    Ok(())
}

fn closed(bot: &BotConfig) -> anyhow::Error {
    anyhow!("{} isn't open to cloud assistants: Reachable by cloud assistants is off in its settings", bot.name)
}

/// `setAssistants`: whether the cloud assistant may reach this computer's bots. Local only.
pub fn set_enabled(hub: &Hub, on: bool) -> Result<CloudStatus> {
    // Off always works; on needs the account the cloud routes the assistant's calls from.
    if on && !available(hub) {
        cloud::require_accounts(&hub.store)?;
        // Until a state pull on this connection works, no owner may only mean "not known yet".
        if !hub.cloud_synced.load(Ordering::Acquire) {
            bail!(
                "This computer isn't connected to the Codync cloud yet, so its account isn't known. \
                Turn on Reach from anywhere, then try again once it's connected."
            );
        }
        bail!("{NOT_IN_ACCOUNT} Sign in on the desktop app or a phone and add this computer to your account first.");
    }
    hub.store.kv_set(KV, if on { "on" } else { "off" })?;
    tracing::info!(on, "cloud assistants changed");
    cloud::update_status(hub, |s| s.assistants = on);
    // Phones see the switch through the network view.
    crate::network::emit(hub);
    Ok(hub.cloud.status())
}

/// Answers one `assistant.call` from the relay: `{"t":"assistant.result","id","ok","result"|"error"}`.
pub async fn answer(hub: &Arc<Hub>, call: &Value) -> Value {
    let id = &call["id"];
    let tool = call["tool"].as_str().unwrap_or_default();
    match cloud_call(hub, call["user"].as_str().unwrap_or_default(), tool, &call["args"]).await {
        Ok(result) => json!({"t": "assistant.result", "id": id, "ok": true, "result": result}),
        Err(e) => {
            tracing::info!(tool, error = format!("{e:#}"), "refused a cloud assistant call");
            json!({"t": "assistant.result", "id": id, "ok": false, "error": format!("{e:#}")})
        }
    }
}

async fn cloud_call(hub: &Arc<Hub>, user: &str, tool: &str, args: &Value) -> Result<Value> {
    if !enabled(&hub.store) {
        bail!(OFF);
    }
    // The cloud only routes the account's own calls here; this computer checks that too.
    if user.is_empty() || hub.cloud.status().owner.map(|o| o.user_id).as_deref() != Some(user) {
        bail!(NOT_IN_ACCOUNT);
    }
    run(hub, &Asker::cloud(), tool, args).await
}

/// `agentCall`: an agent on this computer that isn't a Codync bot, through `codync-host mcp
/// network`. It's on the computer, like the user, so neither the cloud switch nor the per-bot
/// cloud scope applies. Loopback only.
pub async fn local_call(hub: &Arc<Hub>, name: &str, tool: &str, args: &Value) -> Result<Value> {
    run(hub, &Asker::local(name)?, tool, args).await
}

/// Who asks through these tools. Neither kind can be woken, so its replies wait here.
struct Asker {
    /// Its id in a bot's chat (`sourceBotId`), and whose replies `read_replies` shows.
    id: String,
    /// How a bot's chat shows it.
    name: String,
    /// Who it is, in the bot's prompt.
    who: String,
    /// What it signs letters to the user's other computers with.
    handle: String,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The user's cloud assistant, through the relay.
    Cloud,
    /// An agent on this computer (`codync-host mcp network`).
    Local,
}

impl Asker {
    fn cloud() -> Self {
        Self {
            id: ID.into(),
            name: NAME.into(),
            who: "The user's cloud assistant (ChatGPT, Claude or another assistant they connected to Codync)".into(),
            handle: HANDLE.into(),
            kind: Kind::Cloud,
        }
    }

    /// `name` is how the agent introduced itself (`--name`).
    fn local(name: &str) -> Result<Self> {
        let name = name.trim();
        if name.is_empty() || name.len() > 40 {
            bail!("an agent's name is 1–40 characters");
        }
        let handle = crate::network::handle(name);
        Ok(Self {
            id: format!("agent:{handle}"),
            name: format!("{name} (this computer)"),
            who: format!("{name}, an agent on this computer that isn't a Codync bot,"),
            handle: format!("agent-{handle}"),
            kind: Kind::Local,
        })
    }

    /// Whether it may see and message `bot`: a cloud assistant only the bots open to it.
    fn reaches(&self, bot: &BotConfig) -> bool {
        self.kind == Kind::Local || bot.reachable_by_assistants
    }
}

async fn run(hub: &Arc<Hub>, asker: &Asker, tool: &str, args: &Value) -> Result<Value> {
    match tool {
        // Also the bots on the user's other computers in this one's network (what they offer).
        "list_bots" => directory::list(hub, &asker.id, args, &hub.network.directory().await, |bot| asker.reaches(bot)),
        "message_bot" => message(hub, asker, args),
        "read_replies" => Ok(hub.assistant.read(&asker.id, args["requestId"].as_str())),
        _ => bail!("unknown tool: {tool}"),
    }
}

fn message(hub: &Arc<Hub>, asker: &Asker, args: &Value) -> Result<Value> {
    let (to, message) = message_args(args)?;
    check_message(message)?;
    if hub.assistant.running(&asker.id) >= MAX_RUNNING {
        bail!("{MAX_RUNNING} requests are still running: read_replies first, then try again");
    }
    let to = match hub.network.recipient(to)? {
        Recipient::Here(bot) => bot,
        Recipient::There(address) => return message_elsewhere(hub, asker, &address, message),
    };
    let target = visible_bot(hub, &to)?;
    if !asker.reaches(&target) {
        return Err(closed(&target));
    }
    let request = uuid::Uuid::new_v4().to_string();
    let back = Back::Assistant { request: request.clone() };
    let mut lifetime = MessageLifetime::new(hub, &request, message, back, &target.name);
    let (source, name) = (asker.id.as_str(), asker.name.as_str());
    let exchange = Exchange { request: &request, source, target: &target.id, text: message, peer_name: Some(name) };
    lifetime.entries.push(exchange.notice(hub, &target.id, &format!("Message from {name}: {message}"), true)?);
    hub.assistant.add(Reply::queued(&asker.id, &request, &target.name, message));
    lifetime.deliver(&target.id, 1, request_prompt(&asker.who, name, message))?;
    Ok(json!({
        "requestId": request, "botId": target.id, "name": target.name, "status": RequestStatus::Queued,
        "note": "Queued. Bots can take minutes: get the reply later with read_replies.",
    }))
}

/// A bot on another of the user's computers (`reviewer@laptop`): a letter from this computer,
/// whose answer comes back here. That computer's own rules and switches decide whether it takes it.
fn message_elsewhere(hub: &Arc<Hub>, asker: &Asker, to: &crate::network::Target, message: &str) -> Result<Value> {
    let (request, peer) = remote::ask_for_replies(hub, &asker.id, &asker.handle, to, message)?;
    Ok(json!({
        "requestId": request, "botId": hub.network.address_text(to), "name": peer, "status": RequestStatus::Queued,
        "note": "Queued on that computer. Bots can take minutes: get the reply later with read_replies.",
    }))
}

/// The tools an agent on this computer gets from `codync-host mcp network`: the cloud
/// assistant's, which the Worker describes for hosted assistants (`cloud/src/routes/assistant.ts`).
pub fn tools() -> Value {
    json!([
        {
            "name": "list_bots",
            "description": "List the user's Codync bots you can message, with their IDs, kinds, specialties and status: this computer's first, then those on the user's other computers (IDs like reviewer@laptop), where Claude Code sessions and Codex threads can be offered too. Narrow it with query or kind; it returns at most limit at a time, with next for the rest.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string", "description": "Words that must all appear in the bot's name or description."},
                "kind": directory::kind_property(),
                "limit": {"type": "integer", "minimum": 1, "maximum": 50, "description": "At most this many (default 20)."},
                "cursor": {"type": "string", "description": "next from the previous call, for the rest."},
            }},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "message_bot",
            "description": "Send a self-contained request to one of the user's Codync bots. It runs in that bot's own chat, with the bot's tools, folder and permissions, and this returns as soon as it is queued. Get the reply later with read_replies.",
            "inputSchema": {"type": "object", "properties": {
                "botId": {"type": "string", "description": "The bot's id from list_bots."},
                "message": {"type": "string", "description": "The task, relevant context, file paths and the expected result."},
            }, "required": ["botId", "message"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        },
        {
            "name": "read_replies",
            "description": "Your recent message_bot requests, newest first, with each bot's reply once it's done. status is queued, sent (the bot is working), completed, failed or cancelled.",
            "inputSchema": {"type": "object", "properties": {
                "requestId": {"type": "string", "description": "One request's id from message_bot."},
            }},
            "annotations": {"readOnlyHint": true},
        },
    ])
}

pub const INSTRUCTIONS: &str = "Codync connects you to the user's Codync bots, on this computer and the user's \
other computers. Use list_bots to find one, message_bot to give it a self-contained task, and read_replies to collect \
its answer. Bots work in their own folders with their own tools and can take minutes: after messaging one, go on with \
your work and check read_replies later instead of waiting in a loop.";

/// The cloud assistant's requests and their replies, oldest first.
#[derive(Default)]
pub struct Replies(Mutex<Vec<Reply>>);

pub(super) struct Reply {
    /// Whose request (`Asker::id`).
    asker: String,
    request: String,
    bot: String,
    quote: String,
    status: RequestStatus,
    /// The reply, or why there is none.
    text: Option<String>,
    at: i64,
}

impl Reply {
    /// A request just made, before anything came back.
    pub(super) fn queued(asker: &str, request: &str, bot: &str, message: &str) -> Self {
        Self {
            asker: asker.into(),
            request: request.into(),
            bot: bot.into(),
            quote: crate::agent::acp::truncate(message, super::REQUEST_QUOTE_BYTES),
            status: RequestStatus::Queued,
            text: None,
            at: now_ms(),
        }
    }

    fn running(&self) -> bool {
        matches!(self.status, RequestStatus::Queued | RequestStatus::Sent)
    }

    fn json(&self) -> Value {
        let mut v = json!({"requestId": self.request, "bot": self.bot, "request": self.quote, "status": self.status, "at": self.at});
        if let Some(text) = &self.text {
            v[if self.status == RequestStatus::Completed { "reply" } else { "detail" }] = text.clone().into();
        }
        v
    }
}

impl Replies {
    pub(super) fn add(&self, reply: Reply) {
        let mut all = self.0.locked();
        all.push(reply);
        while all.len() > KEEP {
            let Some(i) = all.iter().position(|r| !r.running()) else { break };
            all.remove(i);
        }
    }

    fn running(&self, asker: &str) -> usize {
        self.0.locked().iter().filter(|r| r.asker == asker && r.running()).count()
    }

    fn set(&self, request: &str, status: RequestStatus, text: Option<String>) {
        if let Some(r) = self.0.locked().iter_mut().find(|r| r.request == request) {
            r.status = status;
            r.text = text;
        }
    }

    /// The bot took the request, unless its outcome already came (a quick one can beat this).
    pub(super) fn started(&self, request: &str) {
        if let Some(r) = self.0.locked().iter_mut().find(|r| r.request == request && r.status == RequestStatus::Queued)
        {
            r.status = RequestStatus::Sent;
        }
    }

    pub(super) fn answered(&self, request: &str, outcome: std::result::Result<String, String>) {
        match outcome {
            Ok(reply) => self.set(request, RequestStatus::Completed, Some(reply)),
            Err(detail) => self.set(request, RequestStatus::Failed, Some(detail)),
        }
    }

    pub(super) fn cancelled(&self, request: &str) {
        self.set(request, RequestStatus::Cancelled, Some(STOPPED.to_owned()));
    }

    /// One of `asker`'s requests, or its newest ones (running and finished).
    fn read(&self, asker: &str, request: Option<&str>) -> Value {
        let all = self.0.locked();
        let mine = || all.iter().filter(|r| r.asker == asker);
        let shown: Vec<Value> = match request {
            Some(id) => mine().filter(|r| r.request == id).map(Reply::json).collect(),
            None => mine().rev().take(SHOW).map(Reply::json).collect(),
        };
        if request.is_some() && shown.is_empty() {
            return json!({"replies": [], "note": "No such request here: this computer forgets them when Codync restarts."});
        }
        json!({"replies": shown, "running": mine().filter(|r| r.running()).count()})
    }
}

#[cfg(test)]
mod tests;
