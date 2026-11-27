//! Coding agents' sessions on this computer that aren't Codync bots (Claude Code, Codex, Cursor, …),
//! through `codync-host mcp network`: each gets a Network address of its own, keyed by its session
//! id, and a mailbox it reads itself (`check_inbox`, the pull tier).
//!
//! The address is chosen once, when the session first calls (`<repo>-<agent>`, `-2` on a clash),
//! and never follows the agent's title: the same session id coming back (a resume, a restart) gets
//! the same address and its waiting mail, a new session id a new address. Only `set_name` changes
//! it, and the old one is never given out again. A session unseen for a month with no mail waiting
//! is forgotten: coming back after that, it gets a new address. A client that says no session id is
//! keyed by its MCP server's process: that address lasts while the process runs, and listings mark
//! it temporary.

use crate::LockExt;
use crate::store::{Store, now_ms};
use anyhow::{Context, Result, bail};
use codync_network::AgentKind;
use codync_network::sessions::{Info, info};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;

const KV_SESSIONS: &str = "network_sessions";
const KV_MAIL: &str = "network_mail";
/// Letters kept per session, oldest dropped first.
const MAIL_MAX: usize = 100;
const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;
/// Pushes that may wake one session in an hour.
pub const WAKES_PER_HOUR: usize = 12;
/// A session keyed by its process is forgotten this long after its last call.
const TEMPORARY_FOR_MS: i64 = DAY_MS;
/// A session with an id and no mail waiting is forgotten this long after its last call.
const KEPT_FOR_MS: i64 = 30 * DAY_MS;
/// The levels a rule to a session can take: it reads its own mail, so no one is there to hold it
/// to Auto or Ask (its letters would all be refused). The apps' rule pickers show these.
pub const LEVELS: [codync_network::Access; 2] = [codync_network::Access::Allow, codync_network::Access::Deny];
/// How long after its last call a session still shows in the apps.
const LISTED_FOR_MS: i64 = 7 * DAY_MS;

/// The ACP registry id whose logo the apps draw for `kind` (their agent icons are keyed by it).
pub fn registry(kind: AgentKind) -> Option<&'static str> {
    Some(match kind {
        AgentKind::ClaudeCode => "claude-acp",
        AgentKind::Codex => "codex-acp",
        AgentKind::Gemini => "gemini",
        AgentKind::Cursor => "cursor",
        AgentKind::Copilot => "github-copilot-cli",
        AgentKind::Opencode => "opencode",
        AgentKind::Pi => "pi-acp",
        AgentKind::Amp => "amp-acp",
        AgentKind::Grok => "grok-build",
        AgentKind::Antigravity => "antigravity-acp",
        AgentKind::Codync | AgentKind::Terminal | AgentKind::Other => return None,
    })
}

/// Who calls, as its MCP server tells (`mcp::identity`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caller {
    pub kind: AgentKind,
    /// The agent's own session id, when it says one.
    pub session: Option<String>,
    /// The MCP server's process (`<pid>-<nonce>`), which keys a session without an id.
    pub process: String,
    pub cwd: String,
}

/// The key of `kind`'s session `id`.
pub fn session_key(kind: AgentKind, id: &str) -> String {
    format!("{}:{id}", kind.as_str())
}

impl Caller {
    pub fn key(&self) -> String {
        match &self.session {
            Some(id) => session_key(self.kind, id),
            None => format!("process:{}", self.process),
        }
    }

    /// Its MCP server's pid, the first part of `process`.
    pub fn pid(&self) -> Option<u32> {
        self.process.split('-').next()?.parse().ok()
    }

    /// Its MCP server's process with that process's start, which tells later whether the agent
    /// still runs; `None` where nothing can tell (Windows). Blocking: runs `ps`.
    pub fn running(&self) -> Option<(u32, String)> {
        let pid = self.pid()?;
        Some((pid, crate::dev_servers::process_start(pid)?))
    }

    /// What its agent's own files say about its session, and the address it would start from.
    /// Reads files: call it off the async workers.
    pub fn look(&self) -> (Info, String) {
        let info = self.session.as_deref().map(|id| info(self.kind, id)).unwrap_or_default();
        (info, default_address(self))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// `<kind>:<session id>`, or `process:<pid>` for a temporary one.
    pub key: String,
    pub kind: AgentKind,
    pub address: String,
    pub cwd: String,
    /// The agent's own title for the conversation, as last read; display only.
    pub title: Option<String>,
    /// Working on a turn, as its agent says when it is listed; never kept.
    #[serde(skip)]
    pub busy: Option<bool>,
    /// Keyed by its MCP server's process: gone when that process is.
    pub temporary: bool,
    /// The name its last `set_name` asked for (its address may be that name numbered).
    #[serde(default)]
    pub named: Option<String>,
    pub seen: i64,
    /// How it hears of its mail: raised by its agent's hook (turn) or its MCP server's wait (push),
    /// never lowered.
    #[serde(default)]
    pub tier: Tier,
    /// The newest letter a hook already told it of.
    #[serde(default)]
    pub announced: Option<String>,
    /// Its MCP server's process at its last call (`Caller::running`).
    #[serde(default)]
    pub running: Option<(u32, String)>,
}

/// How a session hears of its mail, from least to most direct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// It reads its mail when it calls `check_inbox`; every tool result says how many wait.
    #[default]
    Pull,
    /// Its hook tells it at its next tool call or when its turn ends.
    Turn,
    /// Its MCP server tells it as soon as a letter comes, starting a turn when it is idle.
    Push,
}

/// What a hook of a session finds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hooked {
    /// The letters waiting, when one came since the last hook told of them; else 0.
    pub unread: usize,
    /// The newest of them, which the hook confirms once it told of them (`told`).
    pub newest: Option<String>,
    /// This call raised its tier.
    pub raised: bool,
}

/// Whether a session's agent runs now, as the apps show it: working or idle where its agent says
/// (Claude Code), else running or not by its MCP server's process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Live {
    Working,
    Idle,
    Online,
    Offline,
}

impl Live {
    /// What its agent says: working on a turn, or idle.
    pub fn of_busy(busy: Option<bool>) -> Option<Self> {
        busy.map(|busy| if busy { Self::Working } else { Self::Idle })
    }

    pub fn runs(self) -> bool {
        self != Self::Offline
    }

    /// What `s`'s agent and process say now; `None` where nothing can tell (Windows, no process
    /// known). A process that called since for another of `all` (Claude Code after `/clear`) no
    /// longer runs `s`. Blocking: reads files and runs `ps`.
    pub fn read(s: &Session, all: &[Session]) -> Option<Self> {
        if let Some(live) = Self::of_busy(s.id().and_then(|id| codync_network::sessions::busy(s.kind, id))) {
            return Some(live);
        }
        let (pid, started) = s.running.as_ref()?;
        // One Claude Code process runs one session at a time; others (Codex threads) may share one.
        let moved = |o: &Session| o.key != s.key && o.running == s.running && o.seen > s.seen;
        if s.kind == AgentKind::ClaudeCode && all.iter().any(moved) {
            return Some(Self::Offline);
        }
        // ponytail: one `ps` per session; one `ps -A` for all if listings grow long.
        let now = crate::dev_servers::process_start(*pid);
        Some(if now.as_ref() == Some(started) { Self::Online } else { Self::Offline })
    }
}

/// A session as the apps list it.
pub struct Listed {
    pub session: Session,
    /// What its agent was doing when last looked at.
    pub state: Option<Live>,
    /// Folded under the older ones: neither running nor called in the last day.
    pub older: bool,
}

impl Session {
    /// The agent's own id for it; `None` for a temporary one.
    pub fn id(&self) -> Option<&str> {
        if self.temporary { None } else { self.key.split_once(':').map(|(_, id)| id) }
    }
}

/// A letter waiting in a session's mailbox.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mail {
    pub id: String,
    /// Who sent it: an address here (`api-claude`) or elsewhere (`reviewer@laptop`).
    pub from: String,
    pub body: String,
    pub reply_to: Option<String>,
    pub at: i64,
}

/// What a call that registers a session changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Registered {
    /// The caller is new.
    pub new: bool,
    /// What this computer offers its networks changed: a new session's address, one's title, or
    /// sessions forgotten (temporary ones aren't offered).
    pub offer: bool,
}

/// The sessions and their mail, kept in the store; one change at a time.
#[derive(Default)]
pub struct Sessions {
    lock: Mutex<()>,
    /// Woken when mail arrives, for `check_inbox` waits.
    pub arrived: tokio::sync::Notify,
    /// What each session's agent was doing when last looked at, by key, and when (Unix ms); never kept.
    live: Mutex<HashMap<String, (Live, i64)>>,
    /// Each session's wakes by a push.
    wakes: Mutex<HashMap<String, Wakes>>,
}

/// When a session was woken by a push within the hour, and the letter a push last couldn't wake it for.
#[derive(Default)]
struct Wakes {
    at: Vec<i64>,
    limited: Option<String>,
}

/// Whether a push may wake a session for a letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    Allowed,
    /// Woken `WAKES_PER_HOUR` times this hour: the first refusal for this letter.
    Limited,
    /// Refused for this letter already.
    StillLimited,
}

impl Sessions {
    /// The caller's session, registered on its first call with the address `reserve` gives out,
    /// one no bot or session ever had, and what the call changed; `title` is what its agent calls
    /// it now.
    pub fn register(
        &self,
        store: &Store,
        caller: &Caller,
        title: Option<String>,
        running: Option<(u32, String)>,
        reserve: impl FnOnce() -> Result<String>,
    ) -> Result<(Session, Registered)> {
        let _one = self.lock.locked();
        let now = now_ms();
        let mut all = load(store);
        let forgot = forget_stale(store, &mut all, now)?;
        let key = caller.key();
        let new = !all.contains_key(&key);
        let session = match all.remove(&key) {
            Some(known) => known,
            None => Session {
                key: key.clone(),
                kind: caller.kind,
                address: reserve()?,
                cwd: caller.cwd.clone(),
                title: None,
                busy: None,
                temporary: caller.session.is_none(),
                named: None,
                seen: now,
                tier: Tier::Pull,
                announced: None,
                running: None,
            },
        };
        let retitled = title.is_some() && title != session.title;
        let session =
            Session { title: title.or(session.title), seen: now, running: running.or(session.running), ..session };
        all.insert(key, session.clone());
        save(store, &all)?;
        let offer = forgot || (!session.temporary && (new || retitled));
        Ok((session, Registered { new, offer }))
    }

    /// Takes what `key`'s agent does now; true when that changed what the apps show.
    pub fn set_live(&self, key: &str, state: Option<Live>) -> bool {
        let mut live = self.live.locked();
        let before = match state {
            Some(state) => live.insert(key.to_owned(), (state, now_ms())),
            None => live.remove(key),
        };
        before.map(|(state, _)| state) != state
    }

    /// Takes what the listed sessions' agents did when a look `started` (Unix ms), forgetting the
    /// rest, except what a session's own call said since; true when that changed what the apps show.
    pub fn renew_live(&self, now: HashMap<String, Live>, started: i64) -> bool {
        let mut live = self.live.locked();
        let mut renewed: HashMap<String, (Live, i64)> =
            now.into_iter().map(|(key, state)| (key, (state, started))).collect();
        renewed.extend(live.iter().filter(|(_, (_, at))| *at > started).map(|(key, entry)| (key.clone(), *entry)));
        let states = |map: &HashMap<String, (Live, i64)>| -> HashMap<String, Live> {
            map.iter().map(|(key, (state, _))| (key.clone(), *state)).collect()
        };
        let changed = states(&live) != states(&renewed);
        *live = renewed;
        changed
    }

    /// The sessions the apps show: not temporary, and seen in the last week or named by `kept`
    /// (a rule that opens it); running first, then the most recent.
    pub fn listed(&self, store: &Store, kept: impl Fn(&Session) -> bool) -> Vec<Listed> {
        let all = {
            let _one = self.lock.locked();
            load(store)
        };
        let now = now_ms();
        let live = self.live.locked();
        let mut listed: Vec<Listed> = all
            .into_values()
            .filter(|s| !s.temporary && (now - s.seen < LISTED_FOR_MS || kept(s)))
            .map(|session| {
                let state = live.get(&session.key).map(|(state, _)| *state);
                let runs = state.is_some_and(Live::runs);
                Listed { older: !runs && now - session.seen >= DAY_MS, state, session }
            })
            .collect();
        listed.sort_by_key(|l| (std::cmp::Reverse(l.state.is_some_and(Live::runs)), std::cmp::Reverse(l.session.seen)));
        listed
    }

    /// Gives the session at `key` the address `reserve` hands out; its old one is retired.
    pub fn rename(
        &self,
        store: &Store,
        key: &str,
        named: &str,
        reserve: impl FnOnce() -> Result<String>,
    ) -> Result<Session> {
        let _one = self.lock.locked();
        let mut all = load(store);
        let Some(session) = all.get_mut(key) else { bail!("this session isn't registered") };
        // Checked under the lock: an overlapping call for the same name keeps the address it got.
        if session.named.as_deref() == Some(named) {
            return Ok(session.clone());
        }
        session.address = reserve()?;
        session.named = Some(named.to_owned());
        let session = session.clone();
        save(store, &all)?;
        Ok(session)
    }

    pub fn by_key(&self, store: &Store, key: &str) -> Option<Session> {
        let _one = self.lock.locked();
        load(store).remove(key)
    }

    pub fn all(&self, store: &Store) -> Vec<Session> {
        let _one = self.lock.locked();
        load(store).into_values().collect()
    }

    pub fn by_address(&self, store: &Store, address: &str) -> Option<Session> {
        let _one = self.lock.locked();
        load(store).into_values().find(|s| s.address.eq_ignore_ascii_case(address))
    }

    /// Leaves `mail` for the session at `key`.
    /// Leaves `mail` for the session at `key`; returns the ids of the oldest letters a full mailbox
    /// dropped unread to make room.
    pub fn post(&self, store: &Store, key: &str, mail: Mail) -> Result<Vec<String>> {
        let dropped = {
            let _one = self.lock.locked();
            // Checked under the same lock as the write: a session forgotten meanwhile gets no mail.
            if !load(store).contains_key(key) {
                bail!("that session is gone: it was forgotten after a long time unseen");
            }
            let mut all = load_mail(store);
            let waiting = all.entry(key.to_owned()).or_default();
            waiting.push(mail);
            let over = waiting.len().saturating_sub(MAIL_MAX);
            let dropped = waiting.drain(..over).map(|m| m.id).collect();
            save_mail(store, &all)?;
            dropped
        };
        self.arrived.notify_waiters();
        Ok(dropped)
    }

    pub fn unread(&self, store: &Store, key: &str) -> usize {
        let _one = self.lock.locked();
        load_mail(store).get(key).map_or(0, Vec::len)
    }

    /// What a hook of the session at `key` tells it: how many letters wait, while its newest letter
    /// isn't one a hook already told it of (`told`), so a hook that runs at every step says it once.
    /// `None` for a session not registered (a hook never registers one: only its MCP server knows
    /// the session the same way). A hook counts as the session being seen, saved once a day; it
    /// raises the session's tier to `tier`.
    pub fn hook(&self, store: &Store, key: &str, tier: Tier) -> Result<Option<Hooked>> {
        let _one = self.lock.locked();
        let mut all = load(store);
        let Some(session) = all.get_mut(key) else { return Ok(None) };
        let mail = load_mail(store);
        let waiting = mail.get(key).map_or(&[][..], Vec::as_slice);
        let newest = waiting.last().map(|m| m.id.clone()).filter(|id| session.announced.as_ref() != Some(id));
        let raised = session.tier < tier;
        let now = now_ms();
        if raised || now - session.seen >= DAY_MS {
            session.tier = session.tier.max(tier);
            session.seen = now;
            save(store, &all)?;
        }
        Ok(Some(Hooked { unread: if newest.is_some() { waiting.len() } else { 0 }, newest, raised }))
    }

    /// Whether a push may wake the session at `key` now for `letter`: at most `WAKES_PER_HOUR` times
    /// an hour, so two sessions that answer each other's letters can't keep each other working
    /// unattended. Asking spends nothing: a wake counts once it was told (`woke`).
    pub fn may_wake(&self, key: &str, letter: &str) -> Wake {
        let now = now_ms();
        let mut wakes = self.wakes.locked();
        let w = wakes.entry(key.to_owned()).or_default();
        w.at.retain(|at| now - at < HOUR_MS);
        if w.at.len() < WAKES_PER_HOUR {
            return Wake::Allowed;
        }
        if w.limited.as_deref() == Some(letter) {
            return Wake::StillLimited;
        }
        w.limited = Some(letter.to_owned());
        Wake::Limited
    }

    /// A push woke the session at `key`.
    pub fn woke(&self, key: &str) {
        self.wakes.locked().entry(key.to_owned()).or_default().at.push(now_ms());
    }

    /// The session's hook printed the notice of the letters up to `letter`: later hooks keep quiet
    /// about them. Recorded only once printed, so a notice its hook never got is told again.
    /// True when it saved that: false when the letter is no longer the newest waiting (read, or
    /// a newer one came) or was already told.
    pub fn told(&self, store: &Store, key: &str, letter: &str) -> Result<bool> {
        let _one = self.lock.locked();
        let mut all = load(store);
        let Some(session) = all.get_mut(key) else { return Ok(false) };
        // An older hook's late confirmation doesn't undo a newer one.
        let mail = load_mail(store);
        let newest = mail.get(key).and_then(|waiting| waiting.last()).map(|m| m.id.as_str());
        if newest != Some(letter) || session.announced.as_deref() == Some(letter) {
            return Ok(false);
        }
        session.announced = Some(letter.to_owned());
        save(store, &all)?;
        Ok(true)
    }

    /// The waiting mail of the session at `key`, oldest first; it counts as read once taken.
    pub fn take(&self, store: &Store, key: &str) -> Result<Vec<Mail>> {
        let _one = self.lock.locked();
        let mut all = load_mail(store);
        let taken = all.remove(key).unwrap_or_default();
        if !taken.is_empty() {
            save_mail(store, &all)?;
        }
        Ok(taken)
    }
}

/// Forgets the sessions unseen for long (a temporary one with its mail); their addresses stay given.
/// True when it forgot any that weren't temporary (those may be offered to a network).
fn forget_stale(store: &Store, all: &mut BTreeMap<String, Session>, now: i64) -> Result<bool> {
    // Most calls find nothing past its time: the mail is read only when something may go.
    let due = |s: &Session| now - s.seen >= if s.temporary { TEMPORARY_FOR_MS } else { KEPT_FOR_MS };
    if !all.values().any(due) {
        return Ok(false);
    }
    let mut mail = load_mail(store);
    let stale = |key: &String, s: &Session| {
        if s.temporary {
            now - s.seen >= TEMPORARY_FOR_MS
        } else {
            now - s.seen >= KEPT_FOR_MS && !mail.contains_key(key)
        }
    };
    let gone: Vec<String> = all.iter().filter(|(key, s)| stale(key, s)).map(|(key, _)| key.clone()).collect();
    let offered = gone.iter().any(|key| all.get(key).is_some_and(|s| !s.temporary));
    let mut mail_gone = false;
    for key in &gone {
        all.remove(key);
        mail_gone |= mail.remove(key).is_some();
    }
    if mail_gone {
        save_mail(store, &mail)?;
    }
    Ok(offered)
}

/// `<repo or folder>-<agent>`: `api-claude`.
fn default_address(caller: &Caller) -> String {
    let dir = Path::new(&caller.cwd);
    let root = dir.ancestors().find(|d| d.join(".git").exists()).unwrap_or(dir);
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let agent = match caller.kind {
        AgentKind::ClaudeCode => "claude",
        AgentKind::Other => "agent",
        kind => kind.as_str(),
    };
    if name.is_empty() {
        return agent.to_owned();
    }
    // A long folder name gives way, so the agent stays in the address.
    let folder: String =
        crate::network::handle(&name).chars().take(super::handles::MAX_LEN - agent.len() - 1).collect();
    crate::network::handle(&format!("{folder}-{agent}"))
}

fn load(store: &Store) -> BTreeMap<String, Session> {
    store.kv_get(KV_SESSIONS).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn save(store: &Store, all: &BTreeMap<String, Session>) -> Result<()> {
    store.kv_set(KV_SESSIONS, &serde_json::to_string(all)?).context("couldn't save the sessions")
}

fn load_mail(store: &Store) -> BTreeMap<String, Vec<Mail>> {
    store.kv_get(KV_MAIL).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn save_mail(store: &Store, all: &BTreeMap<String, Vec<Mail>>) -> Result<()> {
    store.kv_set(KV_MAIL, &serde_json::to_string(all)?).context("couldn't save the sessions' mail")
}

#[cfg(test)]
mod tests;
