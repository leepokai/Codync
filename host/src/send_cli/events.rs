//! The host's event stream as the apps read it: bot snapshots, approval cards and the tracked bot's chat.

use super::{Failure, redact};
use futures::StreamExt;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::watch;

#[derive(Default, PartialEq, Eq)]
enum Phase {
    /// Reading the catch-up.
    #[default]
    Loading,
    Ready,
    /// The running host is older than this command: its `hello` doesn't say how long the catch-up is.
    Outdated,
    /// Why the event stream ended.
    Disconnected(String),
}

/// How a tracked turn ended, as the bot's status said.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Settled {
    Idle,
    Error,
}

#[derive(Default)]
struct State {
    phase: Phase,
    bots: BTreeMap<String, Value>,
    permissions: HashMap<String, Value>,
    tracking: Option<Tracking>,
}

/// The bot `--wait` follows: its chat since tracking began, and which turn took each sent message.
struct Tracking {
    bot: String,
    started: Instant,
    entries: BTreeMap<i64, Value>,
    /// Messages seen waiting in the queue: only one of these going out starts a turn (a reaction or edit to an older
    /// message re-sends it as `sent`).
    queued: HashSet<String>,
    starting: Vec<(String, i64)>,
    assigned: HashMap<String, i64>,
    active_turn: Option<i64>,
    settled: HashMap<i64, Settled>,
    paused: Duration,
    pause_started: Option<Instant>,
}

impl Tracking {
    fn new(bot: &str) -> Self {
        Self {
            bot: bot.to_owned(),
            started: Instant::now(),
            entries: BTreeMap::new(),
            queued: HashSet::new(),
            starting: Vec::new(),
            assigned: HashMap::new(),
            active_turn: None,
            settled: HashMap::new(),
            paused: Duration::ZERO,
            pause_started: None,
        }
    }

    /// Time since tracking began, and how much of it an approval held.
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
    pub settled: Option<Settled>,
    pub elapsed: Duration,
    pub approval: Duration,
    /// Waiting for an approval (the timeout's clock is stopped).
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
    /// Opens the event stream and reads its catch-up.
    pub async fn connect(base: &str, token: &str) -> Result<Self, Failure> {
        let request = crate::http().get(format!("{base}/events?since=0&client=cli-send")).bearer_auth(token).send();
        let response = tokio::time::timeout(Duration::from_secs(20), request)
            .await
            .ok()
            .and_then(Result::ok)
            .ok_or_else(|| Failure::host(crate::local_api::NOT_RUNNING))?;
        if !response.status().is_success() {
            let body: Value = response.json().await.unwrap_or_default();
            return Err(Failure::host(redact(body["error"].as_str().unwrap_or("the host refused"), token)));
        }
        let (sender, receiver) = watch::channel(State::default());
        let writer = sender.clone();
        let task = tokio::spawn(async move {
            let why = read_events(response, &writer).await;
            writer.send_modify(|s| s.phase = Phase::Disconnected(why));
        });
        let mut feed = Self { sender, receiver, task };
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match &feed.receiver.borrow().phase {
                    Phase::Ready => return Ok(()),
                    Phase::Outdated => {
                        return Err(Failure::host("the running host is older than this command; restart it"));
                    }
                    Phase::Disconnected(why) => return Err(disconnected(why)),
                    Phase::Loading => {}
                }
                feed.changed().await?;
            }
        })
        .await
        .map_err(|_| Failure::host("couldn't read the host's bot snapshot"))??;
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

    /// Where the turn answering sent message `entry` stands.
    pub fn progress(&self, entry: &str) -> Result<Progress, Failure> {
        let state = self.receiver.borrow();
        if let Phase::Disconnected(why) = &state.phase {
            return Err(disconnected(why));
        }
        let tracked = state.tracking.as_ref().expect("track is called before waiting");
        let turn = tracked.assigned.get(entry).copied();
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
            settled: turn.and_then(|turn| tracked.settled.get(&turn).copied()),
            elapsed,
            approval,
            pending: tracked.pause_started.is_some(),
        })
    }

    pub fn timing(&self) -> (Duration, Duration) {
        self.receiver.borrow().tracking.as_ref().expect("track is called before waiting").timing()
    }

    pub async fn changed(&mut self) -> Result<(), Failure> {
        self.receiver.changed().await.map_err(|_| Failure::host("the host event stream disconnected"))
    }
}

impl State {
    fn entry(&mut self, entry: &Value) {
        let kind = entry["kind"].as_str().unwrap_or_default();
        if kind == "permission"
            && let Some(id) = entry["id"].as_str()
            && self.permissions.get(id).is_none_or(|old| old["rev"].as_i64() <= entry["rev"].as_i64())
        {
            self.permissions.insert(id.to_owned(), entry.clone());
        }
        let Some(tracked) = self.tracking.as_mut().filter(|t| entry["botId"] == t.bot) else { return };
        if !matches!(kind, "user" | "agent" | "notice") {
            return;
        }
        if kind == "user"
            && let Some(id) = entry["id"].as_str()
        {
            match entry["data"]["status"].as_str() {
                Some("queued") => {
                    tracked.queued.insert(id.to_owned());
                }
                Some("sent") if tracked.queued.remove(id) => {
                    if let Some(turn) = entry["turn"].as_i64() {
                        tracked.starting.push((id.to_owned(), turn));
                    }
                }
                _ => {}
            }
        }
        if let Some(seq) = entry["seq"].as_i64()
            && tracked.entries.get(&seq).is_none_or(|old| old["rev"].as_i64() <= entry["rev"].as_i64())
        {
            tracked.entries.insert(seq, entry.clone());
        }
    }

    fn event(&mut self, event: &Value) {
        match event["type"].as_str() {
            Some("bot") => self.bot(&event["bot"]),
            Some("entry") => self.entry(&event["entry"]),
            _ => {}
        }
        self.update_pause();
    }

    fn bot(&mut self, bot: &Value) {
        let Some(id) = bot["id"].as_str() else { return };
        if self.bots.get(id).is_some_and(|old| old["rev"].as_i64() > bot["rev"].as_i64()) {
            return;
        }
        self.bots.insert(id.to_owned(), bot.clone());
        let Some(t) = self.tracking.as_mut().filter(|t| t.bot == id) else { return };
        let settled = match bot["status"].as_str() {
            Some("idle") => Some(Settled::Idle),
            Some("error") => Some(Settled::Error),
            _ => None,
        };
        if bot["status"] == "working" && !t.starting.is_empty() {
            // Consecutive queued messages in one lane become one host turn: the last one's (`start_turn`), whose `sent`
            // comes last. Not the highest: two sends at once can queue in the other order than their turns.
            let turn = t.starting.last().map(|(_, turn)| *turn).expect("nonempty batch");
            for (id, _) in t.starting.drain(..) {
                t.assigned.insert(id, turn);
            }
            t.active_turn = Some(turn);
        } else if let Some(settled) = settled
            && let Some(turn) = t.active_turn.take()
        {
            t.settled.insert(turn, settled);
        }
    }

    fn update_pause(&mut self) {
        let Some(t) = self.tracking.as_mut() else { return };
        // The host sets `needsInput` only while an approval card waits, and that card can be older than the
        // catch-up's newest 200 entries: the status alone says the bot is waiting for the user.
        let pending = self.bots.get(&t.bot).is_some_and(|b| b["status"] == "needsInput");
        if pending {
            t.pause_started.get_or_insert_with(Instant::now);
        } else if let Some(start) = t.pause_started.take() {
            t.paused += start.elapsed();
        }
    }
}

fn disconnected(why: &str) -> Failure {
    Failure::host(format!("the host event stream disconnected: {why}"))
}

/// Reads events into `sender` until the stream ends, and says why it did.
async fn read_events(response: reqwest::Response, sender: &watch::Sender<State>) -> String {
    let mut events = std::pin::pin!(crate::tui::net::sse_values(response));
    // Catch-up events still to come, once `hello` has said how many.
    let mut left = None;
    while let Some(event) = events.next().await {
        let event = match event {
            Ok(event) => event,
            Err(e) => return e.to_string(),
        };
        // A lagged stream lost turn transitions that a current bot snapshot can't bring back.
        if event["type"] == "resync" {
            return "it fell behind".to_owned();
        }
        sender.send_modify(|s| {
            if event["type"] == "hello" {
                left = event["catchUp"].as_u64();
                if left.is_none() {
                    s.phase = Phase::Outdated;
                }
            } else {
                s.event(&event);
                left = left.map(|n| n.saturating_sub(1));
            }
            if left == Some(0) {
                s.phase = Phase::Ready;
            }
        });
    }
    "the host closed it".to_owned()
}
