//! What the call's voice hears from the computer while the call lasts (`docs/plans/voice-architecture.md`, stage
//! 2): the bot's final replies, approvals waiting, failures, and short progress while it works. Each goes out as a
//! `call` event that only the app holding the call receives (`api::events`), worded for the call's voice (a
//! realtime model relays it; on-device and speech-to-text calls read it aloud); the app hands it to its voice.

use super::speech::spoken;
use super::tools::{approval, runtime};
use super::{CallMode, state};
use crate::LockExt;
use crate::hub::{BotStatus, Hub, Runtime};
use crate::store::{BotConfig, Entry, EntryKind, Store, now_ms};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Duration, Instant};

/// The fewest seconds between two progress lines.
const PROGRESS_EVERY: Duration = Duration::from_secs(15);
/// A working bot the voice hasn't heard about for this long gets a "still working".
const SILENCE: Duration = Duration::from_secs(45);
const TICK: Duration = Duration::from_secs(5);
/// A call its app hasn't touched for this long (the app is gone, or the call silent) stops being watched; the app's
/// next request for it brings the watcher back.
// ponytail: an on-device call silent for this long misses cues until the user speaks again; track the owner's
// connection if that matters.
const IDLE: Duration = Duration::from_hours(2);
/// The chat's latest entries read as a watcher starts (replies still unfinished) and after missed events.
const RECENT: i64 = 50;
/// Replies and notices kept for an app that reconnects mid-call.
const KEPT: usize = 10;
const REPLY_CHARS: usize = 3_000;
const LINE_CHARS: usize = 300;

/// A call this host process watches.
struct Watch {
    /// Which watcher task: a stopped one never removes the one after it.
    task: u64,
    /// When the app last touched the call.
    touched: Instant,
    /// The last cue's `seq` (ms or above, so it keeps rising across host restarts): an app skips cues it heard.
    seq: i64,
    /// The latest replies and notices, for an app that missed them ([`replay`]).
    kept: VecDeque<Value>,
}

static WATCHED: LazyLock<Mutex<HashMap<String, Watch>>> = LazyLock::new(Mutex::default);
static TASKS: AtomicU64 = AtomicU64::new(0);

/// What a cue is on the wire: the bot's reply, a notice from the computer, or what the bot is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Reply,
    Notice,
    Progress,
}

/// Something for the voice, worded for the call's voice as it goes out ([`Cue::words`]).
#[derive(Debug, PartialEq, Eq)]
enum Cue {
    /// A final reply as written; in a group, with the member who wrote it.
    Reply { from: Option<String>, text: String },
    /// An approval card waiting: who asks, for what, and its options' names.
    Approval { from: String, request: String, options: Vec<String> },
    /// An error notice in the chat.
    Failure { from: String, text: String },
    /// What the bot is doing.
    Progress(String),
}

impl Cue {
    /// The cue as the call's voice takes it: a realtime model gets lines to relay, shortened for speech; a voice that
    /// reads aloud gets them worded for the listener (it shortens replies itself, and skips progress).
    fn words(&self, mode: CallMode) -> Option<(Kind, String)> {
        let relayed = mode == CallMode::Realtime;
        match self {
            Self::Reply { from, text } => {
                let text = if relayed { spoken(text, REPLY_CHARS) } else { text.clone() };
                let text = match from {
                    Some(name) => format!("{name}: {text}"),
                    None => text,
                };
                Some((Kind::Reply, text))
            }
            Self::Approval { from, request, options } => {
                let request = spoken(request, LINE_CHARS);
                let text = if relayed {
                    format!("{from} needs the user's approval: {request} (options: {})", options.join(", "))
                } else {
                    format!("{from} needs your approval in the chat: {request}")
                };
                Some((Kind::Notice, text))
            }
            Self::Failure { from, text } => {
                Some((Kind::Notice, format!("{from} hit a problem: {}", spoken(text, LINE_CHARS))))
            }
            Self::Progress(line) => relayed.then(|| (Kind::Progress, line.clone())),
        }
    }
}

/// What a call has told its voice so far, and what it tells it next.
struct Announcer {
    /// The call's bot or group: its chat is the one followed.
    cfg: BotConfig,
    /// Entries already said.
    said: HashSet<String>,
    /// Replies unfinished when the call began: said when they finish, though they began before it.
    open: HashSet<String>,
    /// When the call began (ms): what the chat held before it isn't said.
    since: i64,
    last_cue: Instant,
    /// The last progress line's activity and when it went out.
    progress: Option<(String, Instant)>,
}

impl Announcer {
    /// `recent`: the chat's latest entries as the call begins.
    fn new(cfg: BotConfig, recent: &[Entry], since: i64, now: Instant) -> Self {
        let open = recent
            .iter()
            .filter(|e| e.kind == EntryKind::Agent.as_str() && e.data["final"] != true)
            .map(|e| e.id.clone())
            .collect();
        Self { cfg, said: HashSet::new(), open, since, last_cue: now, progress: None }
    }

    /// An event of the hub's stream.
    fn event(&mut self, hub: &Hub, v: &Value, now: Instant) -> Option<Cue> {
        match v["type"].as_str()? {
            "entry" => self.entry(hub, &v["entry"], now),
            "bot" if v["bot"]["id"] == self.cfg.id.as_str() => self.progress(&runtime(hub, &self.cfg), now),
            _ => None,
        }
    }

    /// After missed events: the chat's latest entries stand in for them (each is still said once).
    fn rescan(&mut self, hub: &Hub, now: Instant) -> Vec<Cue> {
        let recent = hub.store.history(&self.cfg.id, i64::MAX, RECENT).unwrap_or_default();
        recent.iter().filter_map(|e| self.entry(hub, &serde_json::to_value(e).ok()?, now)).collect()
    }

    /// Every few seconds: progress that waited out the rate limit, else a "still working" after a long silence.
    fn tick(&mut self, rt: &Runtime, now: Instant) -> Option<Cue> {
        self.progress(rt, now)
            .or_else(|| if now.duration_since(self.last_cue) >= SILENCE { self.still_working(rt, now) } else { None })
    }

    fn entry(&mut self, hub: &Hub, e: &Value, now: Instant) -> Option<Cue> {
        let id = e["id"].as_str()?;
        if e["botId"] != self.cfg.id.as_str() || self.said.contains(id) {
            return None;
        }
        let new = e["createdAt"].as_i64().unwrap_or(0) >= self.since;
        let data = &e["data"];
        let cue = match e["kind"].as_str()? {
            k if k == EntryKind::Agent.as_str() => self.reply(hub, id, data, new)?,
            k if k == EntryKind::Permission.as_str() && new && data["status"] == "pending" => {
                let (request, options) = approval(data);
                let options = options.into_iter().map(str::to_owned).collect();
                Cue::Approval { from: self.author(hub, data), request: request.to_owned(), options }
            }
            k if k == EntryKind::Notice.as_str() && new && data["style"] == "error" => Cue::Failure {
                from: self.author(hub, data),
                text: data["text"].as_str().unwrap_or_default().to_owned(),
            },
            _ => return None,
        };
        self.said.insert(id.to_owned());
        self.last_cue = now;
        // Progress right after a cue is stale (`send_message` leaves "Working…" just before the turn ends): the next
        // line waits out the limit.
        let last = self.progress.take().map(|(last, _)| last).unwrap_or_default();
        self.progress = Some((last, now));
        Some(cue)
    }

    /// A final reply begun during the call, or one unfinished when it began.
    fn reply(&self, hub: &Hub, id: &str, data: &Value, new: bool) -> Option<Cue> {
        let text = data["text"].as_str().unwrap_or_default().trim();
        if data["final"] != true || text.is_empty() || !(new || self.open.contains(id)) {
            return None;
        }
        Some(Cue::Reply { from: self.cfg.is_group().then(|| self.author(hub, data)), text: text.to_owned() })
    }

    /// Who wrote an entry: in a group, the member.
    fn author(&self, hub: &Hub, data: &Value) -> String {
        data["author"].as_str().map_or_else(|| self.cfg.name.clone(), |id| hub.bot_name(id))
    }

    /// What the bot is doing, when that changed and the last line is old enough.
    fn progress(&mut self, rt: &Runtime, now: Instant) -> Option<Cue> {
        let doing = doing(rt)?;
        let changed =
            self.progress.as_ref().is_none_or(|(last, at)| *last != doing && now.duration_since(*at) >= PROGRESS_EVERY);
        if doing.is_empty() || !changed {
            return None;
        }
        let line = format!("{}: {doing}", self.cfg.name.trim());
        Some(self.progressed(doing, line, now))
    }

    /// Whatever the bot is doing, after a long silence.
    fn still_working(&mut self, rt: &Runtime, now: Instant) -> Option<Cue> {
        let doing = doing(rt)?;
        let name = self.cfg.name.trim();
        let line = if doing.is_empty() {
            format!("{name} is still working.")
        } else {
            format!("{name} is still working: {doing}")
        };
        Some(self.progressed(doing, line, now))
    }

    fn progressed(&mut self, doing: String, line: String, now: Instant) -> Cue {
        self.progress = Some((doing, now));
        self.last_cue = now;
        Cue::Progress(line)
    }
}

/// The bot's activity line while it works, shortened for speech.
fn doing(rt: &Runtime) -> Option<String> {
    (rt.status == BotStatus::Working).then(|| spoken(rt.activity.trim(), LINE_CHARS))
}

/// Tells the call's app what its voice should say until the call ends or the app leaves it idle; for a call watched
/// already, notes that the app touched it.
pub(super) fn watch(hub: &Arc<Hub>, call: &str, bot: &str) {
    let task = TASKS.fetch_add(1, Ordering::Relaxed);
    {
        let mut watched = WATCHED.locked();
        if let Some(w) = watched.get_mut(call) {
            w.touched = Instant::now();
            return;
        }
        watched.insert(call.to_owned(), Watch { task, touched: Instant::now(), seq: 0, kept: VecDeque::new() });
    }
    let Ok(Some(row)) = hub.store.bot(bot) else {
        unwatch(call, task);
        return;
    };
    let mut events = hub.events.subscribe();
    let recent = hub.store.history(bot, i64::MAX, RECENT).unwrap_or_default();
    let mut announcer = Announcer::new(row.config, &recent, now_ms(), Instant::now());
    let (hub, call) = (hub.clone(), call.to_owned());
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TICK);
        loop {
            let cues: Vec<Cue> = tokio::select! {
                event = events.recv() => match event {
                    Ok(v) => announcer.event(&hub, &v, Instant::now()).into_iter().collect(),
                    Err(RecvError::Lagged(_)) => announcer.rescan(&hub, Instant::now()),
                    Err(RecvError::Closed) => break,
                },
                _ = tick.tick() => {
                    if !watching(&hub.store, &call, task) {
                        break;
                    }
                    announcer.tick(&runtime(&hub, &announcer.cfg), Instant::now()).into_iter().collect()
                }
            };
            for cue in &cues {
                tell(&hub, &call, cue);
            }
        }
        unwatch(&call, task);
    });
}

/// The call is still on, this task watches it and its app touched it lately; else the watcher stops here.
fn watching(store: &Store, call: &str, task: u64) -> bool {
    let live = super::is_live(store, call);
    let mut watched = WATCHED.locked();
    let Some(w) = watched.get(call).filter(|w| w.task == task) else { return false };
    let keep = live && w.touched.elapsed() < IDLE;
    if !keep {
        watched.remove(call);
    }
    keep
}

fn unwatch(call: &str, task: u64) {
    let mut watched = WATCHED.locked();
    if watched.get(call).is_some_and(|w| w.task == task) {
        watched.remove(call);
    }
}

/// Sends the app holding the call a cue, worded for its voice now (a replaced session may be on another device, or
/// carry another mode).
fn tell(hub: &Hub, call: &str, cue: &Cue) {
    let Some(s) = state(&hub.store, call) else { return };
    let Some((kind, text)) = cue.words(s.mode) else { return };
    let event = {
        let mut watched = WATCHED.locked();
        let Some(w) = watched.get_mut(call) else { return };
        w.seq = now_ms().max(w.seq + 1);
        let event = json!({"type": "call", "call": call, "owner": s.owner, "kind": kind, "text": text, "seq": w.seq});
        if kind != Kind::Progress {
            if w.kept.len() == KEPT {
                w.kept.pop_front();
            }
            w.kept.push_back(event.clone());
        }
        event
    };
    let _ = hub.events.send(event);
}

/// The latest replies and notices for the calls `owner` holds, for its catch-up: an app that missed them while it
/// reconnected hears them now (it skips the ones it heard by `seq`).
pub fn replay(owner: &str) -> Vec<Value> {
    WATCHED.locked().values().flat_map(|w| &w.kept).filter(|v| v["owner"] == owner).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{BotKind, Lane};

    fn entry(hub: &Hub, bot: &str, kind: EntryKind, data: &Value) -> Value {
        let e = hub.store.insert_entry(&Lane::main(bot), kind, 1, data).unwrap();
        json!({"type": "entry", "entry": e})
    }

    fn setup(id: &str) -> (Arc<Hub>, BotConfig) {
        let hub = super::super::context::tests::hub();
        let cfg = super::super::context::tests::bot(id, "Mika", "/tmp");
        hub.store.save_bot(&cfg).unwrap();
        (hub, cfg)
    }

    fn working(activity: &str) -> Runtime {
        Runtime { status: BotStatus::Working, activity: activity.into(), ..Runtime::default() }
    }

    #[tokio::test]
    async fn the_voice_hears_new_replies_approvals_and_failures_once() {
        let (hub, cfg) = setup("b-cue");
        let now = Instant::now();
        let old = entry(&hub, &cfg.id, EntryKind::Agent, &json!({"text": "Before the call.", "final": true}));
        std::thread::sleep(Duration::from_millis(2));
        let mut a = Announcer::new(cfg.clone(), &[], now_ms(), now);
        assert_eq!(a.event(&hub, &old, now), None, "a reply from before the call isn't said");

        let reply =
            entry(&hub, &cfg.id, EntryKind::Agent, &json!({"text": "**Tests pass.** See `cargo test`", "final": true}));
        let said = a.event(&hub, &reply, now).unwrap();
        assert_eq!(said.words(CallMode::Realtime), Some((Kind::Reply, "Tests pass. See cargo test".into())));
        assert_eq!(a.event(&hub, &reply, now), None, "said once, though its entry changes again");

        let streaming = entry(&hub, &cfg.id, EntryKind::Agent, &json!({"text": "Work"}));
        assert_eq!(a.event(&hub, &streaming, now), None, "not until it's final");

        let card =
            json!({"title": "Run rm -rf build", "status": "pending", "options": [{"name": "Allow"}, {"name": "Deny"}]});
        let card = entry(&hub, &cfg.id, EntryKind::Permission, &card);
        let cue = a.event(&hub, &card, now).unwrap();
        assert_eq!(
            cue.words(CallMode::Realtime),
            Some((Kind::Notice, "Mika needs the user's approval: Run rm -rf build (options: Allow, Deny)".into()))
        );

        let failed = entry(&hub, &cfg.id, EntryKind::Notice, &json!({"text": "Rate limited.", "style": "error"}));
        let cue = a.event(&hub, &failed, now).unwrap();
        assert_eq!(cue.words(CallMode::Device), Some((Kind::Notice, "Mika hit a problem: Rate limited.".into())));

        let elsewhere = entry(&hub, "another-bot", EntryKind::Agent, &json!({"text": "Not this chat.", "final": true}));
        assert_eq!(a.event(&hub, &elsewhere, now), None);
    }

    #[tokio::test]
    async fn a_reply_unfinished_when_the_call_began_is_said_when_it_finishes() {
        let (hub, cfg) = setup("b-open");
        let begun = entry(&hub, &cfg.id, EntryKind::Agent, &json!({"text": "Let me run the full test suite."}));
        let id = begun["entry"]["id"].as_str().unwrap().to_owned();
        std::thread::sleep(Duration::from_millis(2));
        let recent = hub.store.history(&cfg.id, i64::MAX, RECENT).unwrap();
        let now = Instant::now();
        let mut a = Announcer::new(cfg.clone(), &recent, now_ms(), now);
        let finished = hub.set_entry(&id, &json!({"text": "All 412 tests pass.", "final": true})).unwrap();
        let cue = a.event(&hub, &json!({"type": "entry", "entry": finished}), now);
        assert_eq!(cue, Some(Cue::Reply { from: None, text: "All 412 tests pass.".into() }));
    }

    #[tokio::test]
    async fn in_a_group_the_voice_hears_which_member_replied_or_asks() {
        let (hub, ada) = setup("b-ada");
        let ada = BotConfig { name: "Ada".into(), ..ada };
        hub.store.save_bot(&ada).unwrap();
        let group = BotConfig {
            kind: BotKind::Group,
            members: vec![ada.id.clone()],
            ..super::super::context::tests::bot("g-team", "Team", "")
        };
        hub.store.save_bot(&group).unwrap();
        let now = Instant::now();
        let mut a = Announcer::new(group.clone(), &[], now_ms(), now);
        let reply = json!({"text": "Done.", "final": true, "author": ada.id});
        let cue = a.event(&hub, &entry(&hub, &group.id, EntryKind::Agent, &reply), now).unwrap();
        assert_eq!(cue.words(CallMode::Realtime), Some((Kind::Reply, "Ada: Done.".into())));
        let card =
            json!({"title": "Edit a.txt", "status": "pending", "options": [{"name": "Allow"}], "author": ada.id});
        let cue = a.event(&hub, &entry(&hub, &group.id, EntryKind::Permission, &card), now).unwrap();
        assert_eq!(
            cue.words(CallMode::Device),
            Some((Kind::Notice, "Ada needs your approval in the chat: Edit a.txt".into()))
        );
    }

    #[test]
    fn a_voice_that_reads_aloud_gets_cues_worded_for_the_listener() {
        let reply = Cue::Reply { from: None, text: "See `main.rs`:\n```\nfn main() {}\n```".into() };
        assert_eq!(
            reply.words(CallMode::Speech),
            Some((Kind::Reply, "See `main.rs`:\n```\nfn main() {}\n```".into())),
            "it shortens replies itself"
        );
        assert_eq!(reply.words(CallMode::Realtime), Some((Kind::Reply, "See main.rs: (code)".into())));
        let card = Cue::Approval { from: "Mika".into(), request: "Edit a.txt".into(), options: vec!["Allow".into()] };
        assert_eq!(
            card.words(CallMode::Device),
            Some((Kind::Notice, "Mika needs your approval in the chat: Edit a.txt".into()))
        );
        assert_eq!(Cue::Progress("Mika: Running tests".into()).words(CallMode::Device), None);
    }

    #[tokio::test]
    async fn progress_is_rate_limited_and_silence_gets_a_still_working() {
        let (hub, cfg) = setup("b-progress");
        let t0 = Instant::now();
        let mut a = Announcer::new(cfg.clone(), &[], now_ms(), t0);
        let bot = json!({"type": "bot", "bot": {"id": cfg.id}});
        assert_eq!(a.event(&hub, &bot, t0), None, "an idle bot has no progress");

        hub.set_runtime(&cfg.id, |r| *r = working("Running `cargo test`"));
        assert_eq!(a.event(&hub, &bot, t0), Some(Cue::Progress("Mika: Running cargo test".into())));
        let editing = working("Editing main.rs");
        let soon = t0 + Duration::from_secs(5);
        assert_eq!(a.tick(&editing, soon), None, "too soon after the last line");
        let later = t0 + PROGRESS_EVERY;
        assert_eq!(a.tick(&editing, later), Some(Cue::Progress("Mika: Editing main.rs".into())), "the tick catches up");
        assert_eq!(a.tick(&editing, later + Duration::from_secs(20)), None, "nothing changed, not silent long enough");
        let quiet = later + SILENCE;
        assert_eq!(a.tick(&editing, quiet), Some(Cue::Progress("Mika is still working: Editing main.rs".into())));
        assert_eq!(a.tick(&Runtime::default(), quiet + SILENCE), None, "an idle bot isn't working");
    }

    #[tokio::test]
    async fn no_progress_right_after_a_reply() {
        let (hub, cfg) = setup("b-after-reply");
        let t0 = Instant::now();
        let mut a = Announcer::new(cfg.clone(), &[], now_ms(), t0);
        let bot = json!({"type": "bot", "bot": {"id": cfg.id}});
        hub.set_runtime(&cfg.id, |r| *r = working("Running tests"));
        assert!(a.event(&hub, &bot, t0).is_some());
        let later = t0 + PROGRESS_EVERY;
        let reply = entry(&hub, &cfg.id, EntryKind::Agent, &json!({"text": "Done.", "final": true}));
        assert!(a.event(&hub, &reply, later).is_some());
        hub.set_runtime(&cfg.id, |r| *r = working("Working…"));
        assert_eq!(a.event(&hub, &bot, later), None, "the turn is ending, not starting new work");
        let still = a.tick(&working("Working…"), later + PROGRESS_EVERY);
        assert_eq!(still, Some(Cue::Progress("Mika: Working…".into())), "still working once the limit passed");
    }

    #[tokio::test]
    async fn replies_and_notices_wait_for_the_app_holding_the_call() {
        let (hub, cfg) = setup("b-kept");
        let call = uuid::Uuid::new_v4().to_string();
        let s = super::super::CallState {
            owner: "phone".into(),
            mode: CallMode::Realtime,
            ..super::super::CallState::new(&cfg.id)
        };
        super::super::save(&hub.store, &call, &s).unwrap();
        WATCHED
            .locked()
            .insert(call.clone(), Watch { task: u64::MAX, touched: Instant::now(), seq: 0, kept: VecDeque::new() });
        let mut events = hub.events.subscribe();
        tell(&hub, &call, &Cue::Progress("Mika: Running tests".into()));
        tell(&hub, &call, &Cue::Reply { from: None, text: "Done.".into() });
        let (first, second) = (events.try_recv().unwrap(), events.try_recv().unwrap());
        assert!(second["seq"].as_i64() > first["seq"].as_i64(), "{first} {second}");

        let kept = replay("phone");
        let kept: Vec<_> = kept.iter().filter(|v| v["call"] == call.as_str()).collect();
        assert_eq!(kept.len(), 1, "progress isn't kept: {kept:?}");
        assert_eq!((kept[0]["kind"].as_str(), &kept[0]["seq"]), (Some("reply"), &second["seq"]));
        assert!(replay("tablet").iter().all(|v| v["call"] != call.as_str()), "only for the device holding it");
        unwatch(&call, u64::MAX);
    }
}
