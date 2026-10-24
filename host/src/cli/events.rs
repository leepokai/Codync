//! The same bot snapshots clients receive; no separate bot-list API exists.

use super::api::Api;
use super::send::Failure;
use futures::StreamExt;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::watch;

#[derive(Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Loading,
    Ready,
    Disconnected,
}

#[derive(Default)]
struct State {
    phase: Phase,
    bots: BTreeMap<String, Value>,
    permissions: HashMap<String, Value>,
    tracking: Option<Tracking>,
}

struct Tracking {
    bot: String,
    started: Instant,
    entries: BTreeMap<i64, Value>,
    sent: HashSet<String>,
    starting: Vec<(String, i64)>,
    assigned: HashMap<String, i64>,
    active_turn: Option<i64>,
    settled: HashMap<i64, String>,
    paused: Duration,
    pause_started: Option<Instant>,
}

impl Tracking {
    fn new(bot: &str) -> Self {
        Self {
            bot: bot.to_owned(),
            started: Instant::now(),
            entries: BTreeMap::new(),
            sent: HashSet::new(),
            starting: Vec::new(),
            assigned: HashMap::new(),
            active_turn: None,
            settled: HashMap::new(),
            paused: Duration::ZERO,
            pause_started: None,
        }
    }

    fn timing(&self) -> (Duration, Duration) {
        let approval = self.paused + self.pause_started.map_or(Duration::ZERO, |start| start.elapsed());
        (self.started.elapsed(), approval)
    }
}

pub(super) struct Progress {
    pub bot: Value,
    pub entries: Vec<Value>,
    pub approvals: Vec<Value>,
    pub turn: Option<i64>,
    pub settled: Option<String>,
    pub elapsed: Duration,
    pub approval: Duration,
    pub pending: bool,
}

pub(super) struct BotFeed {
    sender: watch::Sender<State>,
    receiver: watch::Receiver<State>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for BotFeed {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl BotFeed {
    pub async fn connect(api: &Api) -> Result<Self, Failure> {
        let response = api.events().await?;
        let (sender, receiver) = watch::channel(State::default());
        let writer = sender.clone();
        let task = tokio::spawn(async move {
            read_events(response, &writer).await;
            writer.send_modify(|s| s.phase = Phase::Disconnected);
        });
        let mut feed = Self { sender, receiver, task };
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match feed.receiver.borrow().phase {
                    Phase::Ready => return Ok(()),
                    Phase::Disconnected => return Err(Failure::api("the host event stream disconnected")),
                    Phase::Loading => {}
                }
                feed.changed().await?;
            }
        })
        .await
        .map_err(|_| Failure::api("couldn't read the host's bot snapshot"))??;
        Ok(feed)
    }

    pub fn bots(&self) -> Vec<Value> {
        self.receiver.borrow().bots.values().filter(|b| b["deleted"] != true).cloned().collect()
    }

    pub fn track(&self, bot: &str) {
        self.sender.send_modify(|s| {
            s.tracking = Some(Tracking::new(bot));
            s.update_pause();
        });
    }

    pub fn merge(&self, entries: &[Value]) {
        self.sender.send_modify(|s| {
            for entry in entries {
                s.entry(entry, false);
            }
            s.update_pause();
        });
    }

    pub fn progress(&self, entry: &Value) -> Result<Progress, Failure> {
        let state = self.receiver.borrow();
        if state.phase == Phase::Disconnected {
            return Err(Failure::api("the host event stream disconnected"));
        }
        let tracked = state.tracking.as_ref().expect("track is called before waiting");
        let turn = entry["id"].as_str().and_then(|id| tracked.assigned.get(id)).copied();
        let (elapsed, approval) = tracked.timing();
        Ok(Progress {
            bot: state.bots.get(&tracked.bot).cloned().unwrap_or(Value::Null),
            entries: tracked.entries.values().cloned().collect(),
            approvals: state
                .permissions
                .values()
                .filter(|p| p["botId"] == tracked.bot && p["data"]["status"] == "pending")
                .cloned()
                .collect(),
            turn,
            settled: turn.and_then(|turn| tracked.settled.get(&turn).cloned()),
            elapsed,
            approval,
            pending: tracked.pause_started.is_some(),
        })
    }

    pub fn timing(&self) -> (Duration, Duration) {
        self.receiver.borrow().tracking.as_ref().expect("track is called before waiting").timing()
    }

    pub async fn changed(&mut self) -> Result<(), Failure> {
        self.receiver.changed().await.map_err(|_| Failure::api("the host event stream disconnected"))
    }
}

impl State {
    fn entry(&mut self, entry: &Value, live: bool) {
        let kind = entry["kind"].as_str().unwrap_or_default();
        if kind == "permission"
            && let Some(id) = entry["id"].as_str()
            && self.permissions.get(id).is_none_or(|old| old["rev"].as_i64() <= entry["rev"].as_i64())
        {
            self.permissions.insert(id.to_owned(), entry.clone());
        }
        let Some(tracked) = self.tracking.as_mut().filter(|t| entry["botId"] == t.bot) else { return };
        if !matches!(kind, "user" | "agent" | "permission" | "notice") {
            return;
        }
        if live
            && kind == "user"
            && entry["data"]["status"] == "sent"
            && let (Some(id), Some(turn)) = (entry["id"].as_str(), entry["turn"].as_i64())
            && tracked.sent.insert(id.to_owned())
        {
            tracked.starting.push((id.to_owned(), turn));
        }
        if let Some(seq) = entry["seq"].as_i64()
            && tracked.entries.get(&seq).is_none_or(|old| old["rev"].as_i64() <= entry["rev"].as_i64())
        {
            tracked.entries.insert(seq, entry.clone());
        }
    }

    fn event(&mut self, event: &Value) {
        match event["type"].as_str() {
            Some("bot") => {
                let bot = &event["bot"];
                if let Some(id) = bot["id"].as_str() {
                    if self.bots.get(id).is_some_and(|old| old["rev"].as_i64() > bot["rev"].as_i64()) {
                        return;
                    }
                    self.bots.insert(id.to_owned(), bot.clone());
                    if let Some(t) = self.tracking.as_mut().filter(|t| t.bot == id) {
                        if bot["status"] == "working" && !t.starting.is_empty() {
                            // Consecutive queued messages in one lane become one host turn.
                            let turn = t.starting.iter().map(|(_, turn)| *turn).max().expect("nonempty batch");
                            for (id, _) in t.starting.drain(..) {
                                t.assigned.insert(id, turn);
                            }
                            t.active_turn = Some(turn);
                        } else if matches!(bot["status"].as_str(), Some("idle" | "error"))
                            && let Some(turn) = t.active_turn.take()
                        {
                            t.settled.insert(turn, bot["status"].as_str().unwrap_or("error").to_owned());
                        }
                    }
                }
            }
            Some("entry") => self.entry(&event["entry"], true),
            _ => {}
        }
        self.update_pause();
    }

    fn update_pause(&mut self) {
        let Some(t) = self.tracking.as_mut() else { return };
        let pending = self.bots.get(&t.bot).is_some_and(|b| b["status"] == "needsInput")
            && self.permissions.values().any(|p| p["botId"] == t.bot && p["data"]["status"] == "pending");
        if pending {
            t.pause_started.get_or_insert_with(Instant::now);
        } else if let Some(start) = t.pause_started.take() {
            t.paused += start.elapsed();
        }
    }
}

async fn read_events(response: reqwest::Response, sender: &watch::Sender<State>) {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut target = None;
    while let Some(Ok(chunk)) = stream.next().await {
        buffer.extend_from_slice(&chunk);
        while let Some(end) = buffer.windows(2).position(|bytes| bytes == b"\n\n") {
            let frame: Vec<_> = buffer.drain(..end + 2).collect();
            let Some(data) = frame.split(|b| *b == b'\n').find_map(|line| line.strip_prefix(b"data:")) else {
                continue;
            };
            let Ok(event) = serde_json::from_slice::<Value>(data) else { return };
            if event["type"] == "resync" {
                // Missing turn transitions cannot be recovered from a current bot snapshot.
                return;
            }
            if event["type"] == "hello" {
                target = event["rev"].as_i64();
                if target == Some(0) {
                    sender.send_modify(|s| s.phase = Phase::Ready);
                }
            } else {
                sender.send_modify(|s| {
                    s.event(&event);
                    if target.is_some_and(|rev| event["rev"].as_i64().is_some_and(|seen| seen >= rev)) {
                        s.phase = Phase::Ready;
                    }
                });
            }
        }
    }
}
