//! The computer's health: memory, CPU and disk, and what each agent Codync started uses.
//!
//! A sampler reads them every few seconds (`sysinfo`; the host knows the pid of each agent it launched). A policy turns
//! the memory in use into a level, with thresholds the person can change: below `warn` all is well; from `warn` the chat
//! says so; from `critical` no new turn starts, idle agents give their memory back, and an agent whose memory grows
//! abnormally is told about and, if it keeps growing, stopped. The readings come through [`Source`], so tests feed it
//! simulated ones. Idea from Mercury Agent's memory governor (MIT, `THIRD-PARTY-NOTICES.md`); the policy is Codync's.

use crate::LockExt;
use crate::agent::bot::{Cmd, NoticeStyle};
use crate::hub::Hub;
use crate::store::{EntryKind, Lane};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How often the computer is read.
pub const SAMPLE_EVERY: Duration = Duration::from_secs(10);
/// An agent below this much memory is never called abnormal, however fast it grew.
const GROWTH_FLOOR: u64 = 1536 * 1024 * 1024;
/// Abnormal: this many times the least it used in the recent samples.
const GROWTH_FACTOR: u64 = 2;
/// How many samples a process's history keeps.
const HISTORY: usize = 7;
/// Samples in a row that must look abnormal before the agent is stopped.
const PERSIST: u8 = 4;
/// A reading older than this says nothing about now.
const FRESH_MS: i64 = 60_000;
const KV_THRESHOLDS: &str = "health.thresholds";
/// Memory must fall this many points below a threshold before the level drops,
/// so a reading that hovers at the limit doesn't flip every sample.
const HYSTERESIS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Normal,
    Warning,
    Critical,
}

/// Memory use, in percent, from which the chat warns and from which no new turn starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    pub warn: u8,
    pub critical: u8,
}

impl Thresholds {
    pub const DEFAULT: Self = Self { warn: 70, critical: 85 };

    pub fn valid(self) -> bool {
        (1..99).contains(&self.warn) && self.warn < self.critical && self.critical <= 99
    }

    #[cfg(test)]
    pub fn level(self, used: u8) -> Level {
        if used >= self.critical {
            Level::Critical
        } else if used >= self.warn {
            Level::Warning
        } else {
            Level::Normal
        }
    }

    /// The level now, keeping the previous one until memory falls [`HYSTERESIS`]
    /// points below the threshold that raised it. Without it a reading that
    /// oscillates around 85 % would flip between warning and limit every sample,
    /// and the chat would fill with one notice per sample.
    pub fn level_from(self, used: u8, previous: Level) -> Level {
        let critical =
            if previous == Level::Critical { self.critical.saturating_sub(HYSTERESIS) } else { self.critical };
        let warn = if previous == Level::Normal { self.warn } else { self.warn.saturating_sub(HYSTERESIS) };
        if used >= critical {
            Level::Critical
        } else if used >= warn {
            Level::Warning
        } else {
            Level::Normal
        }
    }
}

/// What one agent process uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUse {
    pub bot_id: String,
    pub pid: u32,
    pub rss: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub ram_total: u64,
    pub ram_free: u64,
    pub cpu: f32,
    pub disk_total: u64,
    pub disk_free: u64,
    pub agents: Vec<AgentUse>,
}

impl Reading {
    /// Memory in use, rounded down to a whole percent.
    pub fn ram_used_percent(&self) -> u8 {
        if self.ram_total == 0 {
            return 0;
        }
        let used = self.ram_total.saturating_sub(self.ram_free);
        u8::try_from(used.saturating_mul(100) / self.ram_total).unwrap_or(100).min(100)
    }
}

/// Where the readings come from. `agents` are the (bot, pid) pairs to measure.
pub trait Source: Send + Sync {
    fn read(&self, agents: &[(String, u32)]) -> Reading;
}

/// The real computer.
pub struct System {
    sys: Mutex<sysinfo::System>,
    data_dir: std::path::PathBuf,
}

impl System {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self { sys: Mutex::new(sysinfo::System::new()), data_dir }
    }
}

impl Source for System {
    fn read(&self, agents: &[(String, u32)]) -> Reading {
        let mut sys = self.sys.locked();
        sys.refresh_memory();
        sys.refresh_cpu_usage();
        let pids: Vec<sysinfo::Pid> = agents.iter().map(|(_, pid)| sysinfo::Pid::from_u32(*pid)).collect();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&pids), true);
        let used: Vec<AgentUse> = agents
            .iter()
            .filter_map(|(bot, pid)| {
                let process = sys.process(sysinfo::Pid::from_u32(*pid))?;
                Some(AgentUse { bot_id: bot.clone(), pid: *pid, rss: process.memory() })
            })
            .collect();
        let disks = sysinfo::Disks::new_with_refreshed_list();
        // The disk the host keeps its data on: the one whose mount point is the longest prefix of the data folder.
        let disk = disks
            .list()
            .iter()
            .filter(|d| self.data_dir.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len());
        Reading {
            ram_total: sys.total_memory(),
            ram_free: sys.available_memory(),
            cpu: sys.global_cpu_usage(),
            disk_total: disk.map_or(0, sysinfo::Disk::total_space),
            disk_free: disk.map_or(0, sysinfo::Disk::available_space),
            agents: used,
        }
    }
}

/// A reading with the level it means and when it was taken.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(flatten)]
    pub reading: Reading,
    pub ram_used_percent: u8,
    pub level: Level,
    pub thresholds: Thresholds,
    pub at: i64,
}

/// A new turn was not started because the computer is short of memory. Its text is what the person reads.
#[derive(Debug)]
pub struct Refused {
    used: u8,
    critical: u8,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Not started: this computer's memory is {} % full, above the {} % limit, so Codync is not starting new turns. \
             Close something or wait a moment, then send it again.",
            self.used, self.critical
        )
    }
}

impl std::error::Error for Refused {}

/// What the growth of one process says.
#[derive(Debug, PartialEq, Eq)]
pub enum Growth {
    Steady,
    /// First abnormal sample.
    Grew,
    /// Abnormal for [`PERSIST`] samples in a row: stop it.
    Persists,
}

#[derive(Default)]
struct Track {
    history: VecDeque<u64>,
    abnormal: u8,
}

impl Track {
    fn observe(&mut self, rss: u64) -> Growth {
        let baseline = self.history.iter().copied().min();
        self.history.push_back(rss);
        if self.history.len() > HISTORY {
            self.history.pop_front();
        }
        let grew = rss >= GROWTH_FLOOR
            && baseline.is_some_and(|low| self.history.len() >= 4 && rss >= low.saturating_mul(GROWTH_FACTOR));
        if !grew {
            self.abnormal = 0;
            return Growth::Steady;
        }
        self.abnormal = self.abnormal.saturating_add(1);
        match self.abnormal {
            1 => Growth::Grew,
            PERSIST => Growth::Persists,
            _ => Growth::Steady,
        }
    }
}

struct State {
    agents: HashMap<String, u32>,
    thresholds: Thresholds,
    last: Option<Snapshot>,
    tracks: HashMap<u32, Track>,
    /// The disk was low on the previous sample: keeps an episode from adding a line per reading.
    disk_low: bool,
}

/// What a sample found that the host must act on.
#[derive(Debug, Default)]
pub struct Findings {
    /// The level changed from the first to the second.
    pub level_changed: Option<(Level, Level)>,
    pub growth: Vec<(String, u32, u64, Growth)>,
    /// The disk crossed into or out of being low (`true` is low).
    pub disk: Option<bool>,
}

pub struct Monitor {
    source: Mutex<Box<dyn Source>>,
    state: Mutex<State>,
}

impl Monitor {
    pub fn new(source: Box<dyn Source>, thresholds: Thresholds) -> Self {
        Self {
            source: Mutex::new(source),
            state: Mutex::new(State {
                agents: HashMap::new(),
                thresholds,
                last: None,
                tracks: HashMap::new(),
                disk_low: false,
            }),
        }
    }

    /// Tests feed the monitor readings they choose.
    #[cfg(test)]
    pub fn use_source(&self, source: Box<dyn Source>) {
        *self.source.locked() = source;
    }

    /// The thresholds saved on this computer, else the defaults.
    pub fn saved_thresholds(store: &crate::store::Store) -> Thresholds {
        store
            .kv_read(KV_THRESHOLDS)
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<Thresholds>(&raw).ok())
            .filter(|t| t.valid())
            .unwrap_or(Thresholds::DEFAULT)
    }

    /// A bot's agent process started (or restarted) with this pid.
    pub fn set_agent(&self, bot: &str, pid: Option<u32>) {
        let mut state = self.state.locked();
        match pid {
            Some(pid) => {
                state.agents.insert(bot.to_owned(), pid);
            }
            None => {
                state.agents.remove(bot);
            }
        }
    }

    /// Saves new thresholds; the next sample uses them.
    pub fn set_thresholds(&self, store: &crate::store::Store, thresholds: Thresholds) -> anyhow::Result<()> {
        anyhow::ensure!(thresholds.valid(), "Use a warning level below the limit, both between 1 and 99 %.");
        store.kv_set(KV_THRESHOLDS, &serde_json::to_string(&thresholds)?)?;
        let mut state = self.state.locked();
        state.thresholds = thresholds;
        state.last = state.last.take().map(|mut s| {
            s.thresholds = thresholds;
            s.level = thresholds.level_from(s.ram_used_percent, s.level);
            s
        });
        Ok(())
    }

    /// Reads the computer now and says what changed.
    pub fn sample(&self, now: i64) -> Findings {
        let agents: Vec<(String, u32)> = self.state.locked().agents.iter().map(|(b, p)| (b.clone(), *p)).collect();
        let reading = self.source.locked().read(&agents);
        let mut state = self.state.locked();
        // Agents that are gone are not measured again; a reused pid must not inherit a history.
        let alive: Vec<u32> = reading.agents.iter().map(|a| a.pid).collect();
        state.agents.retain(|_, pid| alive.contains(pid));
        state.tracks.retain(|pid, _| alive.contains(pid));
        let used = reading.ram_used_percent();
        let thresholds = state.thresholds;
        let before = state.last.as_ref().map_or(Level::Normal, |s| s.level);
        let level = thresholds.level_from(used, before);
        let mut growth = Vec::new();
        for agent in &reading.agents {
            let verdict = state.tracks.entry(agent.pid).or_default().observe(agent.rss);
            if verdict != Growth::Steady {
                growth.push((agent.bot_id.clone(), agent.pid, agent.rss, verdict));
            }
        }
        let free_pct = reading.disk_free.saturating_mul(100).checked_div(reading.disk_total).unwrap_or(100);
        // Warn at 10 % free, and stop saying so only once 12 % is free again.
        let disk_low = if state.disk_low { free_pct <= 12 } else { free_pct <= 10 };
        let disk = (disk_low != state.disk_low).then_some(disk_low);
        state.disk_low = disk_low;
        state.last = Some(Snapshot { reading, ram_used_percent: used, level, thresholds, at: now });
        Findings { level_changed: (level != before).then_some((before, level)), growth, disk }
    }

    /// The last reading, for the screens.
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.state.locked().last.clone()
    }

    /// Whether a new turn may start: not while memory is at the limit, judged by a reading that is still fresh.
    pub fn admit(&self, now: i64) -> Result<(), Refused> {
        let state = self.state.locked();
        match &state.last {
            Some(s) if s.level == Level::Critical && now - s.at <= FRESH_MS => {
                Err(Refused { used: s.ram_used_percent, critical: s.thresholds.critical })
            }
            _ => Ok(()),
        }
    }
}

/// What the chat says when memory crosses a level.
pub fn level_text(level: Level, used: u8, thresholds: Thresholds) -> Option<(String, NoticeStyle)> {
    match level {
        Level::Normal => None,
        Level::Warning => Some((
            if used >= thresholds.warn {
                format!(
                    "This computer's memory is {used} % full (warning from {} %). Close what you don't need; at {} % Codync stops starting new turns.",
                    thresholds.warn, thresholds.critical
                )
            } else {
                format!(
                    "This computer's memory is {used} % full and easing below the {} % warning level.",
                    thresholds.warn
                )
            },
            NoticeStyle::Info,
        )),
        Level::Critical => Some((
            if used >= thresholds.critical {
                format!(
                    "This computer's memory is {used} % full, above the {} % limit. Codync is not starting new turns and is freeing the agents that are idle.",
                    thresholds.critical
                )
            } else {
                format!(
                    "This computer's memory is {used} % full and easing. Codync keeps new turns paused until it falls below {} %.",
                    thresholds.critical.saturating_sub(HYSTERESIS)
                )
            },
            NoticeStyle::Error,
        )),
    }
}

fn gib(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    format!("{}.{} GB", bytes / GB, (bytes % GB) * 10 / GB)
}

/// Samples forever: tells the chats when memory crosses a level, frees idle agents under pressure, and deals with an
/// agent whose memory runs away.
pub async fn run(hub: Arc<Hub>) {
    loop {
        tokio::time::sleep(SAMPLE_EVERY).await;
        let monitor = hub.clone();
        let findings = tokio::task::spawn_blocking(move || monitor.health.sample(crate::store::now_ms())).await;
        match findings {
            Ok(findings) => act(&hub, &findings),
            Err(error) => tracing::warn!(error = format!("{error:#}"), "the computer could not be read"),
        }
    }
}

fn say(hub: &Hub, bot: &str, text: &str, style: NoticeStyle) {
    let turn = hub.store.max_turn(bot);
    hub.add_entry(&Lane::main(bot), EntryKind::Notice, turn, &json!({"text": text, "style": style}));
}

/// The computer's memory banner for `bot`: one notice per episode. A new
/// reading, of any level, updates the same row in place, so a reading that
/// hovers around a threshold never adds another row to the chat.
fn say_health(hub: &Hub, bot: &str, text: &str, style: NoticeStyle, level: &str) {
    let data = json!({"text": text, "style": style, "health": true, "level": level});
    let lane = Lane::main(bot);
    if let Some(last) = hub.store.last_health_notice(&lane)
        && last.data["level"] != "normal"
    {
        hub.set_entry(&last.id, &data);
        return;
    }
    let turn = hub.store.max_turn(bot);
    hub.add_entry(&lane, EntryKind::Notice, turn, &data);
}

/// Memory came back down: turn the episode's banner into a recovery line, so
/// the chat reads "back to normal" instead of leaving the last alarm standing.
fn recover_health(hub: &Hub, bot: &str, used: u8) {
    let lane = Lane::main(bot);
    let Some(last) = hub.store.last_health_notice(&lane) else { return };
    if last.data["level"] == "normal" {
        return;
    }
    let data = json!({
        "text": format!("This computer's memory is back to normal ({used} % full)."),
        "style": NoticeStyle::Info,
        "health": true,
        "level": "normal",
    });
    hub.set_entry(&last.id, &data);
}

fn act(hub: &Arc<Hub>, findings: &Findings) {
    let Some(snapshot) = hub.health.snapshot() else { return };
    let working: Vec<String> = snapshot.reading.agents.iter().map(|a| a.bot_id.clone()).collect();
    if let Some((_, level)) = findings.level_changed {
        if let Some((text, style)) = level_text(level, snapshot.ram_used_percent, snapshot.thresholds) {
            let name = match level {
                Level::Warning => "warning",
                Level::Critical => "critical",
                Level::Normal => "normal",
            };
            for bot in &working {
                say_health(hub, bot, &text, style, name);
            }
        } else if level == Level::Normal {
            // `level_text` has no line for Normal: turn the episode's banner into
            // a recovery line, so the chat doesn't leave the last alarm standing.
            for bot in &working {
                recover_health(hub, bot, snapshot.ram_used_percent);
            }
        }
        if level == Level::Critical {
            // Idle agents give their memory back at once; busy ones are left alone.
            for bot in &working {
                let _ = hub.send_cmd(bot, Cmd::FreeMemory);
            }
        }
    }
    if let Some(low) = findings.disk {
        let free = gib(snapshot.reading.disk_free);
        for bot in &working {
            let (text, style) = if low {
                (
                    format!(
                        "This computer's disk is almost full ({free} free). Free space, or saving and agent work may fail."
                    ),
                    NoticeStyle::Error,
                )
            } else {
                (format!("This computer's disk has room again ({free} free)."), NoticeStyle::Info)
            };
            say(hub, bot, &text, style);
        }
    }
    for (bot, _, rss, growth) in &findings.growth {
        match growth {
            Growth::Steady => {}
            Growth::Grew => say(
                hub,
                bot,
                &format!(
                    "This bot's agent is using {} and its memory keeps growing. If it keeps growing, Codync will stop it.",
                    gib(*rss)
                ),
                NoticeStyle::Error,
            ),
            Growth::Persists => {
                say(
                    hub,
                    bot,
                    &format!(
                        "This bot's agent kept growing (now {}), so Codync stopped it to protect the computer.",
                        gib(*rss)
                    ),
                    NoticeStyle::Error,
                );
                if let Err(error) = hub.stop_bot(bot) {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        bot,
                        "an agent using too much memory could not be stopped"
                    );
                }
                // Once the turn is over the agent itself goes, with the memory it holds.
                let _ = hub.send_cmd(bot, Cmd::FreeMemory);
            }
        }
    }
}

/// The answer to `hostHealth`: the last reading with the bots' names, read now if none was taken yet.
pub fn value(hub: &Hub) -> serde_json::Value {
    if hub.health.snapshot().is_none() {
        hub.health.sample(crate::store::now_ms());
    }
    let Some(snapshot) = hub.health.snapshot() else { return json!({}) };
    let mut value = serde_json::to_value(&snapshot).unwrap_or_default();
    value["refusing"] = (snapshot.level == Level::Critical).into();
    value["agents"] = snapshot
        .reading
        .agents
        .iter()
        .map(|a| json!({"botId": a.bot_id, "botName": hub.bot_name(&a.bot_id), "pid": a.pid, "rss": a.rss}))
        .collect();
    value
}

#[cfg(test)]
mod tests;
