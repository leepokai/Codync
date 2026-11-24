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
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

const KV_SESSIONS: &str = "network_sessions";
const KV_MAIL: &str = "network_mail";
/// Letters kept per session, oldest dropped first.
const MAIL_MAX: usize = 100;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// A session keyed by its process is forgotten this long after its last call.
const TEMPORARY_FOR_MS: i64 = DAY_MS;
/// A session with an id and no mail waiting is forgotten this long after its last call.
const KEPT_FOR_MS: i64 = 30 * DAY_MS;

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
    /// An address asked for in the server's config (`--name`), used once, at registration.
    pub hint: Option<String>,
}

impl Caller {
    pub fn key(&self) -> String {
        match &self.session {
            Some(id) => format!("{}:{id}", self.kind.as_str()),
            None => format!("process:{}", self.process),
        }
    }

    /// What its agent's own files say about its session, and the address it would start from.
    /// Reads files: call it off the async workers.
    pub fn look(&self) -> (Info, String) {
        let info = self.session.as_deref().map(|id| info(self.kind, id)).unwrap_or_default();
        let base = self.hint.as_deref().map_or_else(|| default_address(self), crate::network::handle);
        (info, base)
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

/// The sessions and their mail, kept in the store; one change at a time.
#[derive(Default)]
pub struct Sessions {
    lock: Mutex<()>,
    /// Woken when mail arrives, for `check_inbox` waits.
    pub arrived: tokio::sync::Notify,
}

impl Sessions {
    /// The caller's session, registered on its first call (then `true`) with the address `reserve`
    /// gives out, one no bot or session ever had; `title` is what its agent calls it now.
    pub fn register(
        &self,
        store: &Store,
        caller: &Caller,
        title: Option<String>,
        reserve: impl FnOnce() -> Result<String>,
    ) -> Result<(Session, bool)> {
        let _one = self.lock.locked();
        let now = now_ms();
        let mut all = load(store);
        forget_stale(store, &mut all, now)?;
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
            },
        };
        let session = Session { title: title.or(session.title), seen: now, ..session };
        all.insert(key, session.clone());
        save(store, &all)?;
        Ok((session, new))
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
    pub fn post(&self, store: &Store, key: &str, mail: Mail) -> Result<()> {
        {
            let _one = self.lock.locked();
            let mut all = load_mail(store);
            let waiting = all.entry(key.to_owned()).or_default();
            waiting.push(mail);
            let over = waiting.len().saturating_sub(MAIL_MAX);
            waiting.drain(..over);
            save_mail(store, &all)?;
        }
        self.arrived.notify_waiters();
        Ok(())
    }

    pub fn unread(&self, store: &Store, key: &str) -> usize {
        let _one = self.lock.locked();
        load_mail(store).get(key).map_or(0, Vec::len)
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
fn forget_stale(store: &Store, all: &mut BTreeMap<String, Session>, now: i64) -> Result<()> {
    let mut mail = load_mail(store);
    let stale = |key: &String, s: &Session| {
        if s.temporary {
            now - s.seen >= TEMPORARY_FOR_MS
        } else {
            now - s.seen >= KEPT_FOR_MS && !mail.contains_key(key)
        }
    };
    let gone: Vec<String> = all.iter().filter(|(key, s)| stale(key, s)).map(|(key, _)| key.clone()).collect();
    let mut mail_gone = false;
    for key in &gone {
        all.remove(key);
        mail_gone |= mail.remove(key).is_some();
    }
    if mail_gone {
        save_mail(store, &mail)?;
    }
    Ok(())
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
