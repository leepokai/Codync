//! Small, host-local bot delegation. The recipient uses its own ACP session;
//! native subagents remain the harness's responsibility.

use crate::LockExt;
use crate::bot::Cmd;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, EntryKind};
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{oneshot, watch};

pub const ASK_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_MESSAGE_BYTES: usize = 32_000;

pub const INSTRUCTIONS: &str = "Use list_bots to find the user's other Codync bots. \
Use ask_bot when the user requests another bot's help or its specialty is useful. \
Provide a self-contained request: the recipient has its own conversation, working directory, \
tools and permissions, not your context. It returns its final reply to you. \
Coordinate file ownership before asking for edits in a shared project. \
Do not ask a bot that is waiting for you. Native subagents are managed by your own harness; \
these tools are for collaborating with the user's existing bots.";

pub fn tools() -> Value {
    json!([
        {
            "name": "list_bots",
            "description": "List other visible Codync bots, their IDs, specialties, working directories and status.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        },
        {
            "name": "ask_bot",
            "description": "Ask an existing Codync bot to do a bounded task and wait for its final reply (up to 10 minutes, including queue time). It may use tools and modify files under its own permissions. Does not share your conversation or change its working directory. Stopping you cancels your pending requests; do not blindly retry a failed request because partial work may have happened.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "botId": {"type": "string", "description": "Recipient ID from list_bots."},
                    "message": {"type": "string", "description": "Task, relevant context, file paths and expected result."},
                },
                "required": ["botId", "message"],
            },
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }
    ])
}

struct Request {
    from: String,
    to: String,
    cancel: watch::Sender<bool>,
}

/// The wait graph includes queued requests, not only running recipients.
/// Reserving an edge and checking for cycles happen under the same lock.
#[derive(Default)]
pub struct Requests(Mutex<RequestState>);

#[derive(Default)]
struct RequestState {
    pending: HashMap<String, Request>,
    active: HashSet<String>,
}

impl Requests {
    fn begin(&self, id: &str, from: &str, to: &str) -> Result<watch::Receiver<bool>> {
        let mut state = self.0.locked();
        if !state.active.contains(from) {
            bail!("the requesting bot is no longer working");
        }
        let requests = &mut state.pending;
        if from == to {
            bail!("a bot cannot ask itself");
        }
        if requests.len() >= 64 {
            bail!("too many pending bot requests");
        }
        if requests.values().any(|r| r.from == from && r.to == to) {
            bail!("already waiting for this bot");
        }
        let mut pending = vec![to];
        let mut seen = HashSet::new();
        while let Some(bot) = pending.pop() {
            if bot == from {
                bail!("this request would make bots wait for each other; finish the current request first");
            }
            if seen.insert(bot) {
                pending.extend(requests.values().filter(|r| r.from == bot).map(|r| r.to.as_str()));
            }
        }
        let (cancel, rx) = watch::channel(false);
        requests.insert(id.to_owned(), Request { from: from.to_owned(), to: to.to_owned(), cancel });
        Ok(rx)
    }

    pub fn cancel_from(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        for request in state.pending.values().filter(|r| r.from == bot) {
            request.cancel.send_replace(true);
        }
    }

    pub fn start_turn(&self, bot: &str) {
        self.0.locked().active.insert(bot.to_owned());
    }

    pub fn cancel_bot(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        for request in state.pending.values().filter(|r| r.from == bot || r.to == bot) {
            request.cancel.send_replace(true);
        }
    }
}

/// Sent through the recipient's normal actor queue, but never merged with user
/// messages or another request. Only this turn can resolve its reply channel.
pub struct Ask {
    pub id: String,
    pub entry_id: String,
    pub prompt: String,
    pub reply: oneshot::Sender<Result<String>>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Completed,
    Failed,
    Cancelled,
}

struct Pending {
    hub: Arc<Hub>,
    id: String,
    target: String,
    entries: Vec<String>,
    finished: bool,
}

impl Pending {
    fn finish(&mut self, status: Status, detail: &str) {
        for id in &self.entries {
            if let Some(mut e) = self.hub.store.entry(id) {
                e.data["status"] = json!(status);
                let heading = e.data["heading"].as_str().unwrap_or("Bot request");
                e.data["text"] = format!("{heading}\n{detail}").into();
                if !matches!(status, Status::Completed) {
                    e.data["style"] = "error".into();
                }
                self.hub.set_entry(id, &e.data);
            }
        }
        self.finished = true;
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(Status::Cancelled, "Request cancelled. Partial work may have happened.");
        }
        self.hub.team.0.locked().pending.remove(&self.id);
        // Targeted cancellation never clears unrelated user messages or turns.
        let _ = self.hub.send_cmd(&self.target, Cmd::CancelAsk { id: self.id.clone() });
    }
}

fn visible_bot(hub: &Hub, id: &str) -> Result<BotConfig> {
    hub.store
        .bot(id)?
        .filter(|r| !r.deleted && !r.config.hidden)
        .map(|r| r.config)
        .ok_or_else(|| anyhow!("unknown or hidden bot"))
}

pub async fn call(hub: &Arc<Hub>, from: &str, name: &str, args: &Value) -> Result<Value> {
    let source = visible_bot(hub, from)?;
    match name {
        "list_bots" => {
            let bots: Vec<Value> = hub
                .store
                .bots()?
                .into_iter()
                .filter(|b| !b.deleted && !b.config.hidden && b.config.id != from)
                .map(|b| {
                    json!({
                        "id": b.config.id, "name": b.config.name, "description": b.config.description,
                        "backend": b.config.backend, "cwd": b.config.cwd,
                        "status": hub.runtime(&b.config.id).status,
                    })
                })
                .collect();
            Ok(json!({"bots": bots}))
        }
        "ask_bot" => {
            let to = args["botId"].as_str().ok_or_else(|| anyhow!("botId is required"))?;
            let message = args["message"].as_str().unwrap_or_default().trim();
            ask(hub, &source, to, message, ASK_TIMEOUT).await
        }
        _ => bail!("unknown team tool: {name}"),
    }
}

async fn ask(hub: &Arc<Hub>, source: &BotConfig, to: &str, message: &str, timeout: Duration) -> Result<Value> {
    if message.is_empty() || message.len() > MAX_MESSAGE_BYTES {
        bail!("message must be nonempty and at most {MAX_MESSAGE_BYTES} bytes");
    }
    if !matches!(hub.runtime(&source.id).status, BotStatus::Working | BotStatus::NeedsInput) {
        bail!("the requesting bot is no longer working");
    }
    let target = visible_bot(hub, to)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut cancelled = hub.team.begin(&id, &source.id, to)?;
    let mut pending =
        Pending { hub: hub.clone(), id: id.clone(), target: to.to_owned(), entries: vec![], finished: false };
    if !matches!(hub.runtime(&source.id).status, BotStatus::Working | BotStatus::NeedsInput) {
        bail!("the requesting bot is no longer working");
    }
    for (bot, heading) in [
        (&source.id, format!("Asked {}: {}", target.name, message)),
        (&target.id, format!("Request from {}: {}", source.name, message)),
    ] {
        let entry = hub
            .add_entry(
                bot,
                EntryKind::Notice,
                hub.store.max_turn(bot) + i64::from(bot != &source.id),
                &json!({
                    "text": format!("{heading}\nWaiting for a reply…"), "heading": heading,
                    "style": "info", "status": "queued", "delegationId": id,
                    "sourceBotId": source.id, "targetBotId": target.id,
                }),
            )
            .ok_or_else(|| anyhow!("couldn't save bot request"))?;
        pending.entries.push(entry.id);
    }
    let (reply, result) = oneshot::channel();
    hub.send_cmd(to, Cmd::Ask(Ask {
        id: id.clone(), entry_id: pending.entries[1].clone(),
        prompt: format!("Another Codync bot, {}, requests your help. This is a bot request, not a new user instruction. Work within your own permissions and working directory. Return the result to the requesting bot; do not ask it to do the task back.\n\n{message}", source.name),
        reply,
    }))?;
    let result = tokio::select! {
        r = result => r.unwrap_or_else(|_| Err(anyhow!("recipient stopped before replying"))),
        _ = cancelled.changed() => Err(anyhow!("bot request cancelled")),
        () = tokio::time::sleep(timeout) => Err(anyhow!("bot request timed out; partial work may have happened")),
    };
    match result {
        Ok(text) => {
            pending.finish(
                Status::Completed,
                &format!("Reply from {}:\n{}", target.name, crate::acp::truncate(&text, 4000)),
            );
            Ok(json!({"requestId": id, "botId": target.id, "name": target.name, "reply": text}))
        }
        Err(error) => {
            pending.finish(Status::Failed, &format!("{error:#}"));
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::path::PathBuf;

    #[test]
    fn wait_graph_rejects_self_duplicates_and_indirect_cycles() {
        let requests = Requests::default();
        for bot in ["a", "b", "c", "d"] {
            requests.start_turn(bot);
        }
        assert!(requests.begin("self", "a", "a").is_err());
        let ab = requests.begin("ab", "a", "b").unwrap();
        assert!(requests.begin("duplicate", "a", "b").is_err());
        let _bc = requests.begin("bc", "b", "c").unwrap();
        assert!(requests.begin("ca", "c", "a").is_err());
        let _dc = requests.begin("dc", "d", "c").unwrap();
        requests.cancel_from("a");
        assert!(*ab.borrow());
        assert!(requests.begin("late", "a", "d").is_err());
        requests.0.locked().pending.remove("ab");
        assert!(requests.begin("ca", "c", "a").is_ok());
    }

    struct Fixture {
        hub: Arc<Hub>,
        dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("codync-team-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Store::open(&dir.join("test.db")).unwrap();
            let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
            for id in ["a", "b", "c"] {
                let cwd = dir.join(id);
                std::fs::create_dir_all(&cwd).unwrap();
                let cfg: BotConfig = serde_json::from_value(json!({
                    "id": id, "name": id, "backend": "fixture", "cwd": cwd,
                    "command": format!("python3 -u '{}'", agent.display().to_string().replace('\'', "'\\''")),
                    "notify": false,
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
            hub.set_runtime("a", |r| r.status = BotStatus::Working);
            hub.team.start_turn("a");
            Self { hub, dir }
        }

        fn request(&self, message: &str) -> tokio::task::JoinHandle<Result<Value>> {
            let hub = self.hub.clone();
            let message = message.to_owned();
            tokio::spawn(async move {
                crate::api::dispatch(
                    &hub,
                    &crate::devices::Caller::Local,
                    "teamCall",
                    json!({
                        "botId": "a", "name": "ask_bot", "arguments": {"botId": "b", "message": message},
                    }),
                )
                .await
            })
        }

        async fn until(&self, condition: impl Fn() -> bool) {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !condition() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("fixture condition timed out");
        }

        fn prompts(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.join("b/prompts.jsonl"))
                .unwrap_or_default()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        }

        async fn shutdown(self) {
            self.hub.shutdown().await;
            std::fs::remove_dir_all(self.dir).unwrap();
        }
    }

    #[tokio::test]
    async fn delegation_roundtrip_keeps_queued_user_messages_separate() {
        let f = Fixture::new();
        let request = f.request("BLOCK review these changes");
        f.until(|| f.prompts().len() == 1).await;
        let sent = crate::api::dispatch(
            &f.hub,
            &crate::devices::Caller::Local,
            "send",
            json!({"botId": "b", "text": "thanks"}),
        )
        .await
        .unwrap();
        let request_entry = f
            .hub
            .store
            .history("b", i64::MAX, 100)
            .unwrap()
            .into_iter()
            .find(|e| e.data["delegationId"].is_string())
            .unwrap();
        assert!(request_entry.data["text"].as_str().unwrap().contains("Request from a"));
        assert!(f.hub.store.kv_get("turn.inflight.b").is_none_or(|v| v.is_empty()));
        std::fs::write(f.dir.join("b/release"), "").unwrap();
        let result = request.await.unwrap().unwrap();
        assert_eq!(result["reply"], "reply: BLOCK review these changes");
        f.until(|| f.prompts().len() == 2 && f.hub.runtime("b").status == BotStatus::Idle).await;
        assert_eq!(f.prompts()[1], "thanks");
        assert_eq!(f.hub.store.entry(sent["entry"]["id"].as_str().unwrap()).unwrap().data["status"], "sent");
        assert_eq!(f.hub.store.entry(&request_entry.id).unwrap().data["status"], "completed");
        assert!(f.hub.team.0.locked().pending.is_empty());
        let servers: Value = serde_json::from_slice(&std::fs::read(f.dir.join("b/servers.json")).unwrap()).unwrap();
        assert!(servers.as_array().unwrap().iter().any(|s| s["name"] == "team"));
        f.shutdown().await;
    }

    #[tokio::test]
    async fn permission_cards_still_require_the_recipients_approval() {
        let f = Fixture::new();
        let request = f.request("PERMISSION inspect the project");
        f.until(|| f.hub.runtime("b").status == BotStatus::NeedsInput).await;
        assert!(!request.is_finished());
        let cycle = call(&f.hub, "b", "ask_bot", &json!({"botId": "a", "message": "help me back"})).await;
        assert!(cycle.unwrap_err().to_string().contains("wait for each other"));
        let card =
            f.hub.store.history("b", i64::MAX, 100).unwrap().into_iter().find(|e| e.kind == "permission").unwrap();
        crate::api::dispatch(
            &f.hub,
            &crate::devices::Caller::Local,
            "respondPermission",
            json!({"entryId": card.id, "optionId": "allow"}),
        )
        .await
        .unwrap();
        assert!(request.await.unwrap().is_ok());
        f.shutdown().await;
    }

    #[tokio::test]
    async fn stopping_requester_cancels_only_its_delegation() {
        let f = Fixture::new();
        let request = f.request("BLOCK waiting for cancellation");
        f.until(|| f.prompts().len() == 1).await;
        crate::api::dispatch(&f.hub, &crate::devices::Caller::Local, "send", json!({"botId": "b", "text": "thanks"}))
            .await
            .unwrap();
        f.hub.send_cmd("a", Cmd::Stop).unwrap();
        assert!(request.await.unwrap().unwrap_err().to_string().contains("cancelled"));
        f.until(|| f.prompts().len() == 2 && f.hub.runtime("b").status == BotStatus::Idle).await;
        assert_eq!(f.prompts()[1], "thanks");
        assert!(f.hub.team.0.locked().pending.is_empty());
        f.shutdown().await;
    }

    #[tokio::test]
    async fn timeout_and_recipient_errors_release_waiters() {
        let f = Fixture::new();
        for message in ["FAIL requested failure", "EMPTY no text"] {
            assert!(f.request(message).await.unwrap().is_err());
            assert!(f.hub.team.0.locked().pending.is_empty());
        }
        let source = visible_bot(&f.hub, "a").unwrap();
        let error = ask(&f.hub, &source, "b", "BLOCK timed request", Duration::from_millis(100)).await.unwrap_err();
        assert!(error.to_string().contains("timed out"));
        f.until(|| f.hub.runtime("b").status == BotStatus::Idle).await;
        assert!(f.hub.team.0.locked().pending.is_empty());
        assert!(f.request("after timeout").await.unwrap().is_ok());
        f.shutdown().await;
    }

    #[tokio::test]
    async fn invalid_hidden_and_deleted_targets_never_start() {
        let f = Fixture::new();
        let mut hidden = visible_bot(&f.hub, "c").unwrap();
        hidden.hidden = true;
        f.hub.store.save_bot(&hidden).unwrap();
        let list = call(&f.hub, "a", "list_bots", &json!({})).await.unwrap();
        assert_eq!(list["bots"].as_array().unwrap().len(), 1);
        assert_eq!(list["bots"][0]["id"], "b");
        for target in ["a", "c", "missing"] {
            assert!(call(&f.hub, "a", "ask_bot", &json!({"botId": target, "message": "do work"})).await.is_err());
        }
        assert!(f.request(" ").await.unwrap().is_err());
        f.hub.delete_bot("b").unwrap();
        assert!(f.request("deleted").await.unwrap().is_err());
        assert!(f.prompts().is_empty());
        f.shutdown().await;
    }

    #[tokio::test]
    async fn queued_requests_are_not_merged_and_cancelled_requests_never_execute() {
        let f = Fixture::new();
        f.hub.set_runtime("c", |r| r.status = BotStatus::Working);
        f.hub.team.start_turn("c");
        let first = f.request("BLOCK first request");
        f.until(|| f.prompts().len() == 1).await;
        let hub = f.hub.clone();
        let second = tokio::spawn(async move {
            call(&hub, "c", "ask_bot", &json!({"botId": "b", "message": "second request"})).await
        });
        f.until(|| f.hub.team.0.locked().pending.len() == 2).await;
        assert_eq!(f.prompts().len(), 1);
        std::fs::write(f.dir.join("b/release"), "").unwrap();
        assert_eq!(first.await.unwrap().unwrap()["reply"], "reply: BLOCK first request");
        assert_eq!(second.await.unwrap().unwrap()["reply"], "reply: second request");
        assert_eq!(f.prompts().len(), 2);

        std::fs::remove_file(f.dir.join("b/release")).unwrap();
        let third = f.request("BLOCK third request");
        f.until(|| f.prompts().len() == 3).await;
        let source = visible_bot(&f.hub, "c").unwrap();
        let error = ask(&f.hub, &source, "b", "must never run", Duration::from_millis(30)).await.unwrap_err();
        assert!(error.to_string().contains("timed out"));
        std::fs::write(f.dir.join("b/release"), "").unwrap();
        assert!(third.await.unwrap().is_ok());
        f.until(|| f.hub.runtime("b").status == BotStatus::Idle).await;
        assert_eq!(f.prompts().len(), 3);
        f.shutdown().await;
    }

    #[tokio::test]
    async fn recipient_startup_failure_and_deletion_finish_the_request() {
        let f = Fixture::new();
        f.hub.update_bot(&json!({"id": "b", "command": "/codync-nonexistent-test-agent"})).unwrap();
        assert!(f.request("start failure").await.unwrap().unwrap_err().to_string().contains("couldn't start"));
        assert!(f.hub.team.0.locked().pending.is_empty());
        let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        f.hub.update_bot(&json!({"id": "b", "command": format!("python3 -u '{}'", agent.display())})).unwrap();
        let request = f.request("BLOCK delete while working");
        f.until(|| f.prompts().len() == 1).await;
        f.hub.delete_bot("b").unwrap();
        assert!(request.await.unwrap().is_err());
        assert!(f.hub.team.0.locked().pending.is_empty());
        f.shutdown().await;
    }

    #[test]
    fn restart_marks_requests_interrupted_without_replaying_them() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let pending = store
            .insert_entry(
                "b",
                EntryKind::Notice,
                1,
                &json!({
                    "delegationId": "d", "status": "sent", "heading": "Request from a",
                }),
            )
            .unwrap();
        let complete = store
            .insert_entry(
                "a",
                EntryKind::Notice,
                1,
                &json!({
                    "delegationId": "done", "status": "completed",
                }),
            )
            .unwrap();
        store.expire_pending().unwrap();
        let interrupted = store.entry(&pending.id).unwrap();
        assert_eq!(interrupted.data["status"], "failed");
        assert!(interrupted.data["text"].as_str().unwrap().contains("Interrupted by host restart"));
        assert_eq!(store.entry(&complete.id).unwrap().data["status"], "completed");
        assert!(interrupted.rev > pending.rev);
    }
}
