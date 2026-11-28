//! The bot's ACP sessions: starting its agent, new/reopen, routine runs' sessions.

use super::fast_mode::Listing;
use super::launch::{Background, claude_meta, launch_commands, select_model, start_agent, start_first, system_prompt};
use super::{Actor, Adapter, Conn, Instructions, ServerWatch, background_work};
use crate::agent::own_memory;
use crate::analytics::Retired;
use crate::chat::{context, memory};
use crate::store::run_session_key;
use anyhow::{Context, Result, anyhow, bail};
use codync_agent::acp::{self, Incoming};
use codync_agent::events::{Event, Update};
use codync_agent::mcp_names::{self, Renamed};
use codync_agent::registry::Candidate;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Seconds pi waits for a tool call (60 by default): past the host's own limit for any of them (`mcp::call`, the
/// longest a team change waiting for its approval).
const PI_CALL_TIMEOUT: u64 = crate::chat::manage::APPROVAL_TIMEOUT.as_secs() + 60;

impl Actor {
    /// Renames the servers `agent`'s own config already names (`codync_agent::mcp_names`).
    async fn avoid_clashes(&self, agent: mcp_names::Agent, servers: &mut Value) -> Renamed {
        let home = self.saved_env(agent.home_var());
        mcp_names::avoid_clashes(agent, servers, &self.cfg.cwd, home, crate::agent::APP).await
    }

    /// Which session the running turn uses: a routine run has one of its own (by its slot); everything else uses
    /// the main one.
    pub(super) fn slot(&self) -> Option<String> {
        self.active_workflow
            .as_deref()
            .map(crate::workflows::session_slot)
            .or_else(|| self.active_routine.as_deref().map(crate::routines::session_slot))
    }

    pub(super) fn session_for(&mut self, slot: Option<&str>) -> Option<String> {
        let Some(slot) = slot else { return self.session_id.clone() };
        if let Some(sid) = self.run_sessions.get(slot) {
            return Some(sid.clone());
        }
        let sid = self.hub.store.kv_get(&run_session_key(&self.cfg.id, slot));
        let sid = sid.filter(|s| !s.is_empty())?;
        self.run_sessions.insert(slot.to_owned(), sid.clone());
        Some(sid)
    }

    pub(super) fn set_session(&mut self, slot: Option<&str>, sid: Option<&str>) {
        let Some(slot) = slot else {
            self.session_id = sid.map(str::to_owned);
            if let Err(error) = self.hub.store.set_session(&self.cfg.id, sid) {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the session");
            }
            return;
        };
        let key = run_session_key(&self.cfg.id, slot);
        let stored = if let Some(s) = sid {
            self.run_sessions.insert(slot.to_owned(), s.to_owned());
            self.hub.store.kv_set(&key, s)
        } else {
            self.run_sessions.remove(slot);
            self.hub.store.kv_delete(&key)
        };
        if let Err(error) = stored {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the routine run's session");
        }
    }

    /// Slots of the routine runs that have a session (stored or loaded).
    pub(super) fn run_slots(&self) -> Vec<String> {
        let prefix = run_session_key(&self.cfg.id, "");
        let mut slots: Vec<String> = self.run_sessions.keys().cloned().collect();
        slots.extend(
            self.hub.store.kv_prefix(&prefix).into_iter().filter_map(|k| k.strip_prefix(&prefix).map(str::to_owned)),
        );
        slots.sort();
        slots.dedup();
        slots
    }

    /// What every session request carries: the folder and MCP servers, and the memory folders for an agent that
    /// takes folders besides its own.
    fn session_params(&self, servers: &Value, session: Option<&str>) -> Value {
        let mut params = json!({"cwd": self.cfg.cwd, "mcpServers": servers});
        if let Some(sid) = session {
            params["sessionId"] = sid.into();
        }
        if !self.cfg.plain && self.conn.as_ref().is_some_and(|c| c.sessions.additional_directories) {
            params["additionalDirectories"] = json!(memory::files::folders(&self.cfg.as_owner()));
        }
        params
    }

    /// What the turn's session calls Codync's MCP servers that it knows by another name.
    pub(super) fn renamed(&self) -> &Renamed {
        static NONE: Renamed = Renamed::new();
        let sid = self.turn_session.as_ref();
        self.conn.as_ref().zip(sid).and_then(|(c, sid)| c.renamed.get(sid)).unwrap_or(&NONE)
    }

    /// A folder saved for the bot's agent (`setAgentEnv`) as `key`, such as where its own config is (`CODEX_HOME`).
    fn saved_env(&self, key: &str) -> Option<std::path::PathBuf> {
        let saved = crate::agent::auth::saved_value(&self.hub.store, &self.cfg.backend, key).unwrap_or_else(|e| {
            tracing::warn!(error = format!("{e:#}"), "couldn't read the agent's saved environment");
            None
        });
        saved.map(Into::into)
    }

    /// ACP connectors, team collaboration, and optional team management and computer control.
    pub(super) fn mcp_servers(&self) -> Result<Value> {
        // An agent on its own (`BotConfig::plain`) has its own tools only, and a thread building a workflow its tools.
        if self.cfg.plain && self.cfg.workflow.is_none() {
            return Ok(json!([]));
        }
        let Ok(exe) = std::env::current_exe() else {
            tracing::warn!("can't locate codync-host for MCP servers");
            return Ok(json!([]));
        };
        let port = self.hub.port;
        if self.cfg.plain {
            return Ok(json!([crate::mcp::acp_server("workflows", &exe, &["workflows", "--bot", &self.cfg.id], port)]));
        }
        let mut servers: Vec<Value> =
            (if self.cfg.connectors.is_empty() { Vec::new() } else { crate::market::connectors(&self.hub.store)? })
                .iter()
                .filter(|c| self.cfg.connectors.contains(&c.id))
                .filter_map(|c| c.acp(&exe, port))
                .collect();
        let composio = crate::market::composio::enabled_for(&self.hub.store, &self.cfg.connectors)?;
        // A thread on the bot's own work or the user's gets neither `routines` nor `team`; a routine's run and a
        // request keep both, as that turn had them in the chat (a run saves its progress, sets follow-ups and may ask a
        // bot; an asked bot runs its own routine, and its `message_bot` is refused with why).
        let bots_own = !self.cfg.is_thread() || !self.task_source.own();
        let builtin = [
            Some("connectors"),
            Some("chat"),
            bots_own.then_some("team"),
            self.cfg.manage_team.then_some("manage"),
            Some("memory"),
            bots_own.then_some("routines"),
            Some("workflows"),
            self.cfg.computer.then_some("computer"),
            composio.then_some("composio"),
        ];
        for name in builtin.into_iter().flatten() {
            // A thread's `memory` server is the thread's too: it reads and writes its bot's memory (`memory::tool_call`),
            // and what it writes is its own turn's.
            servers.push(crate::mcp::acp_server(name, &exe, &[name, "--bot", &self.cfg.id], port));
        }
        Ok(Value::Array(servers))
    }

    /// Starts the agent one way: resolved first (a registry build can download), with the harness's own memory set
    /// for the bot.
    async fn start_candidate(
        &self,
        candidate: Candidate,
        mut env: Vec<(String, String)>,
        budget: Duration,
        progress: impl Fn(&str),
    ) -> Result<Conn> {
        let mut cmd = candidate.resolve(progress).await?;
        codync_agent::backends::prepare_launch_command(&self.cfg.backend, &mut cmd, &env);
        let mut command = [cmd.acp()];
        // A user-scope `codync` network server the agent loads knows it serves a bot.
        env.push((crate::mcp::BOT_ENV.into(), self.cfg.id.clone()));
        self.hub.set_runtime(&self.id(), |r| r.activity = "Starting agent…".into());
        own_memory::apply(&self.cfg, &mut env, &mut command).await;
        let background = if self.cfg.is_thread() { Background::Reported } else { Background::Unreported };
        let conn = start_agent(&self.cfg.backend, &command[0], &self.cfg.cwd, &env, budget, background)
            .await
            .inspect_err(|e| {
                tracing::info!(command = command[0].as_str(), error = format!("{e:#}"), "agent failed to start");
            })?;
        self.hub.dev_servers.agent_started(&self.cfg, &conn.acp, &command[0]);
        Ok(conn)
    }

    /// `ensure_session` for the turn: a routine run's setup must end by its time limit, and setup cut short
    /// mustn't leave the agent opening a session the host never learns of.
    pub(super) async fn ensure_turn_session(&mut self, slot: Option<&str>) -> Result<()> {
        let Some(deadline) = self.routine_deadline else { return self.ensure_session(slot).await };
        let setup = tokio::time::timeout_at(deadline.into(), self.ensure_session(slot)).await;
        if setup.is_err() {
            self.drop_unanswered().await;
        }
        setup.context("routine session setup timed out")?
    }

    /// Makes sure the agent runs and `slot`'s session (main, or a routine run's) is live;
    /// sets `turn_session`.
    async fn ensure_session(&mut self, slot: Option<&str>) -> Result<()> {
        let profile = self.cfg.login_profile.as_deref();
        if !self.cfg.connectors.is_empty()
            || crate::agent::auth::launch_reads_vault(&self.hub.store, &self.cfg.backend, profile)?
        {
            crate::market::vault::unlock(self.hub.clone()).await?;
        }
        // A stale process starts again and resumes its session. One that can't resume keeps its process and session:
        // its next new session gets the new tools.
        if std::mem::take(&mut self.restart_before_turn)
            && self.conn.as_ref().is_some_and(|c| c.sessions.reopen().is_some())
        {
            self.kill_agent().await;
        }
        let mut servers = self.mcp_servers()?;
        // pi gets its servers when it starts (`codync_agent::pi`), so they take pi's names before that.
        let pi = self.cfg.backend == "pi";
        let mut renamed =
            if pi { self.avoid_clashes(mcp_names::Agent::Pi, &mut servers).await } else { Renamed::new() };
        if self.conn.is_none() {
            let (hub, id) = (self.hub.clone(), self.cfg.id.clone());
            let progress = move |msg: &str| {
                let msg = msg.to_owned();
                hub.set_runtime(&id, |r| r.activity = msg);
            };
            let candidates = launch_commands(&self.cfg)?;
            // Keys saved from an "environment variable" sign-in, and what the harness needs to start.
            let profile = self.cfg.login_profile.as_deref();
            let on_profile = crate::agent::login_profiles::prepare(&self.hub.store, &self.cfg.backend, profile).await?;
            let (mut env, grants) = crate::agent::auth::launch_env(&self.hub.store, &self.cfg.backend, profile)?;
            if self.cfg.backend == "codex" && !env.iter().any(|(k, _)| k == "CODEX_CONFIG") {
                // Codex's `/compact` takes no instructions: the keep-list is its config's `compact_prompt`.
                let config = json!({"compact_prompt": codync_mem::compaction::KEEP});
                env.push(("CODEX_CONFIG".into(), config.to_string()));
            }
            if pi {
                let dir = crate::service::data_dir().join("pi");
                env.extend(codync_agent::pi::env(dir, crate::agent::APP, &servers, PI_CALL_TIMEOUT).await?);
            }
            let mut conn = start_first(candidates, &env, |candidate, env, fallback_left| {
                // A local CLI too old for ACP just hangs; don't make the user wait long before the fallback.
                let budget = Duration::from_secs(if fallback_left { 20 } else { 180 });
                self.start_candidate(candidate, env, budget, &progress)
            })
            .await?;
            grants.held_by(&conn.acp);
            conn.on_profile = on_profile;
            conn.codex_home =
                env.iter().find(|(key, home)| key == "CODEX_HOME" && !home.is_empty()).map(|(_, home)| home.clone());
            self.conn = Some(conn);
        }
        if self.conn.as_ref().is_some_and(|c| c.adapter() == Adapter::Codex) {
            renamed = self.avoid_clashes(mcp_names::Agent::Codex, &mut servers).await;
        }
        // An agent on its own gets no system prompt of Codync's, and `compose` adds no first-message one.
        let instructions = if self.cfg.plain {
            Instructions::FirstMessage
        } else {
            self.conn.as_ref().map_or(Instructions::FirstMessage, Conn::instructions)
        };
        // Claude gets the frozen instructions as its system prompt. When a compaction
        // re-rendered them, loading the session again swaps the prompt in (the adapter
        // rebuilds its query and resumes the same conversation).
        let current = self.session_for(slot);
        let snapshot = match (&current, instructions) {
            (Some(sid), Instructions::ClaudeMeta) => Some(self.snapshot(sid).await?),
            _ => None,
        };
        let Some(conn) = self.conn.as_ref() else { bail!("agent not running") };
        if let Some(sid) = current {
            let prompt_current =
                snapshot.as_ref().is_none_or(|s| self.applied_system.get(&sid).is_some_and(|t| *t == s.system));
            if conn.loaded.contains_key(&sid) && prompt_current {
                if !self.servers_dropped().await {
                    self.turn_session = Some(sid);
                    return Ok(());
                }
                self.retire(Retired::ServersLost).await;
                return Box::pin(self.ensure_session(slot)).await;
            }
            let Some(reopen) = conn.sessions.reopen() else {
                tracing::info!(bot = %self.cfg.id, "the agent can't resume sessions; starting a fresh one");
                self.set_session(slot, None);
                return Box::pin(self.ensure_session(slot)).await;
            };
            self.hub.set_runtime(&self.cfg.id, |r| r.activity = "Resuming session…".into());
            let mut params = self.session_params(&servers, Some(&sid));
            if let Some(s) = &snapshot {
                params["_meta"] = claude_meta(&self.cfg, &s.system);
            }
            if instructions == Instructions::SystemPrompt {
                params["systemPrompt"] = system_prompt(&self.snapshot(&sid).await?.system);
            }
            let res = self.session_request(reopen.method(), params).await;
            let Some(conn) = self.conn.as_mut() else { return res.map(drop) };
            let about_agent = drop_replay(conn);
            match res {
                Ok(res) => {
                    // A resumed session can come back on the agent's last-used model (Cursor
                    // keeps one for all its sessions), so the bot's own is set again.
                    let res = if res.get("configOptions").is_some() || res.get("models").is_some() {
                        match select_model(&conn.acp, &res, &sid, &self.cfg).await {
                            Ok(selected) => selected,
                            Err(error) => {
                                // The agent now holds a session the host doesn't track, and `session/resume` refuses
                                // one it holds. Stopped, it takes the session again on the next message.
                                if reopen == acp::Reopen::Resume {
                                    self.kill_agent().await;
                                }
                                return Err(error);
                            }
                        }
                    } else {
                        res
                    };
                    conn.loaded.insert(sid.clone(), Instant::now());
                    conn.renamed.insert(sid.clone(), renamed);
                    watch_servers(conn).await;
                    self.note_options(&sid, &res, Listing::Session);
                    for update in &about_agent {
                        self.on_session_wide(update);
                    }
                    if let Some(s) = snapshot {
                        // Its system prompt now names the bot and holds memory as rendered.
                        context::mark_announced(&self.hub.store, self.cfg.owner(), &s, s.system_identity.clone())?;
                        memory::opened(&self.hub.store, self.cfg.owner(), &sid, &s.memory)?;
                        self.applied_system.insert(sid.clone(), s.system);
                    }
                    self.turn_session = Some(sid);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(bot = %self.cfg.id, method = reopen.method(), error = format!("{e:#}"), "couldn't reopen the session; starting fresh");
                }
            }
        }
        // A thread starts from its task alone, never from the chat (`context::render` gives it its own profile).
        if slot.is_none() {
            crate::chat::work::began(&self.hub, &self.cfg);
        }
        let (revision, system) = {
            let (hub, cfg) = (self.hub.clone(), self.cfg.clone());
            tokio::task::spawn_blocking(move || {
                (memory::current_revision(&hub.store, cfg.owner()), context::render(&hub.store, &cfg))
            })
            .await?
        };
        let mut params = self.session_params(&servers, None);
        self.instruct(&mut params, instructions, &system);
        let res = self.session_request("session/new", params).await?;
        let sid = self.adopt_opened(&res, slot, renamed).await?;
        context::adopt(&self.hub.store, &self.cfg, &sid, system.clone(), revision)?;
        if instructions == Instructions::ClaudeMeta {
            self.applied_system.insert(sid, system);
        }
        Ok(())
    }

    /// Puts the bot's instructions in a session request's `params`, for an agent that takes them there.
    fn instruct(&self, params: &mut Value, instructions: Instructions, system: &str) {
        match instructions {
            Instructions::ClaudeMeta => params["_meta"] = claude_meta(&self.cfg, system),
            Instructions::SystemPrompt => params["systemPrompt"] = system_prompt(system),
            Instructions::FirstMessage => {}
        }
    }

    /// Takes the session the agent just opened (`res`: its answer to `session/new`) as `slot`'s
    /// and this turn's: on the bot's model, with its options noted. Its id.
    async fn adopt_opened(&mut self, res: &Value, slot: Option<&str>, renamed: Renamed) -> Result<String> {
        let Some(conn) = self.conn.as_mut() else { bail!("agent not running") };
        let sid = res["sessionId"].as_str().ok_or_else(|| anyhow!("agent returned no sessionId"))?.to_owned();
        conn.loaded.insert(sid.clone(), Instant::now());
        conn.renamed.insert(sid.clone(), renamed);
        watch_servers(conn).await;
        self.unfiled.opened(conn.adapter(), &sid);
        let options = select_model(&conn.acp, res, &sid, &self.cfg).await?;
        self.note_options(&sid, &options, Listing::Session);
        self.set_session(slot, Some(&sid));
        self.session_fresh = true;
        self.turn_session = Some(sid.clone());
        Ok(sid)
    }

    /// A session request the agent doesn't answer in time stops the agent: a hung one
    /// would otherwise hold the bot, and its Stop, forever.
    pub(super) async fn session_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let conn = self.conn.as_mut().ok_or_else(|| anyhow!("agent not running"))?;
        conn.opening = matches!(method, "session/new" | "session/load" | "session/resume");
        let answered = tokio::time::timeout(SESSION_TIMEOUT, conn.acp.request(method, params)).await;
        conn.opening = false;
        if let Ok(result) = answered {
            return result;
        }
        self.kill_agent().await;
        let message = format!("the agent didn't answer {method} within {}s", SESSION_TIMEOUT.as_secs());
        Err(acp::Failure::error(acp::FailureKind::Timeout, message))
    }

    /// A routine's time limit cut a session request short. An agent that can resume stops, so a session it might
    /// still open goes with its process; one that can't keeps running rather than lose the sessions it has.
    async fn drop_unanswered(&mut self) {
        let Some(conn) = self.conn.as_mut() else { return };
        if !std::mem::take(&mut conn.opening) {
            return;
        }
        if conn.sessions.reopen().is_none() {
            // ponytail: that session lasts until the agent next stops; keep the cut-short request's Answer past the
            // time limit and session/close the id it returns, if that ever matters.
            tracing::warn!(bot = %self.cfg.id, "a session request was cut short; the agent can't resume, so it keeps running");
            return;
        }
        self.kill_agent().await;
    }

    /// Drops the main ACP session so the next turn starts a fresh one.
    pub(super) fn forget_session(&mut self) {
        self.set_session(None, None);
    }

    /// Whether the agent dropped Codync's MCP servers from its sessions (OpenCode 2.x once it has been idle a while,
    /// #508), so it must start again to get them back: loading a session again in the same process doesn't, OpenCode
    /// registers a session's servers once. Not while stopping it would end work it runs detached: the next turn looks
    /// again.
    async fn servers_dropped(&self) -> bool {
        let Some(conn) = &self.conn else { return false };
        if conn.servers != ServerWatch::Seen || background_work::codync_servers_run(&conn.acp).await {
            return false;
        }
        if background_work::holds_restart(&conn.acp).await {
            tracing::warn!(bot = %self.cfg.id, "the agent dropped Codync's MCP servers; keeping it for the work it runs detached");
            return false;
        }
        true
    }
}

/// Marks Codync's MCP servers seen once a session opened with them shows them under the agent (`ServerWatch`).
async fn watch_servers(conn: &mut Conn) {
    if conn.servers == ServerWatch::Unseen && background_work::codync_servers_run(&conn.acp).await {
        conn.servers = ServerWatch::Seen;
    }
}

/// How long opening a session (`session/new`, `session/load`, `session/resume`) may take.
const SESSION_TIMEOUT: Duration = Duration::from_mins(3);

/// An agent replays a session's history as `session/update` before it answers a reopen: drops it, and
/// refuses the requests in it. What it says about itself meanwhile (its commands) isn't history: returned for
/// `on_session_wide`.
fn drop_replay(conn: &mut Conn) -> Vec<Update> {
    let mut about_agent = Vec::new();
    while let Ok(inc) = conn.rx.try_recv() {
        // A request in it is refused as it drops.
        if let Incoming::Update(update) = inc
            && matches!(update.event, Event::Commands(_))
        {
            about_agent.push(update);
        }
    }
    about_agent
}
