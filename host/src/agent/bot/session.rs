//! The agent process and its ACP sessions: launch, handshake, new/load/fork, thread sessions.

use super::{Actor, Conn, SessionCaps};
use crate::agent::acp::{self, Acp};
use crate::store::{BotConfig, Entry, EntryKind, Lane, lane_key};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

impl Actor {
    /// Which session a lane's turns use: a thread of the bot's own chat has its own
    /// (by root); its chat and every group lane use the main one.
    pub(super) fn slot(&self, lane: &Lane) -> Option<String> {
        (lane.chat == self.cfg.id).then(|| lane.thread.clone()).flatten()
    }

    pub(super) fn session_for(&mut self, slot: Option<&str>) -> Option<String> {
        let Some(root) = slot else { return self.session_id.clone() };
        if let Some(sid) = self.thread_sessions.get(root) {
            return Some(sid.clone());
        }
        let sid = self.hub.store.kv_get(&lane_key("session", &self.cfg.id, &Lane::in_thread(&self.cfg.id, root)));
        let sid = sid.filter(|s| !s.is_empty())?;
        self.thread_sessions.insert(root.to_owned(), sid.clone());
        Some(sid)
    }

    pub(super) fn set_session(&mut self, slot: Option<&str>, sid: Option<&str>) {
        let Some(root) = slot else {
            self.session_id = sid.map(str::to_owned);
            if let Err(error) = self.hub.store.set_session(&self.cfg.id, sid) {
                tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the session");
            }
            return;
        };
        match sid {
            Some(s) => self.thread_sessions.insert(root.to_owned(), s.to_owned()),
            None => self.thread_sessions.remove(root),
        };
        let key = lane_key("session", &self.cfg.id, &Lane::in_thread(&self.cfg.id, root));
        if let Err(error) = self.hub.store.kv_set(&key, sid.unwrap_or_default()) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't store the thread session");
        }
    }

    /// Roots of the threads that have a session (stored or loaded).
    pub(super) fn thread_roots(&self) -> Vec<String> {
        let prefix = lane_key("session", &self.cfg.id, &Lane::main(&self.cfg.id)) + "/";
        let mut roots: Vec<String> = self.thread_sessions.keys().cloned().collect();
        roots.extend(
            self.hub.store.kv_prefix(&prefix).into_iter().filter_map(|k| k.strip_prefix(&prefix).map(str::to_owned)),
        );
        roots.sort();
        roots.dedup();
        roots
    }

    /// What a new thread session is told first: where the thread branches off.
    pub(super) fn thread_intro(&self, root: &str, forked: bool) -> String {
        let quote = |e: &Entry| {
            let who = if e.kind == EntryKind::User.as_str() { "the user" } else { "you" };
            format!("{who}: \"{}\"", acp::truncate(e.data["text"].as_str().unwrap_or_default(), 1500))
        };
        let Some(root_entry) = self.hub.store.entry(root) else {
            return "[Thread] The user opened a thread off your main chat. It is a separate branch of the conversation.".into();
        };
        if forked {
            return format!(
                "[Thread] The user opened a thread on this message from your main chat — {}. This thread is its own \
                 branch: it starts from everything said so far, and nothing said here reaches the main chat. \
                 Just continue the conversation; there's no need to mention this.",
                quote(&root_entry)
            );
        }
        // A fresh session: bring the lines leading up to the message along.
        let before = self.hub.store.history(&self.cfg.id, root_entry.seq, 12).unwrap_or_default();
        let lines: Vec<String> = before
            .iter()
            .chain(std::iter::once(&root_entry))
            .filter(|e| {
                e.kind == EntryKind::User.as_str() || (e.kind == EntryKind::Agent.as_str() && e.data["final"] == true)
            })
            .map(quote)
            .collect();
        format!(
            "[Thread] The user opened a thread on the last of these messages from your main chat (oldest first):\n{}\n\
             This thread is its own branch of the conversation; nothing said here reaches the main chat. \
             Just continue the conversation; there's no need to mention this.",
            lines.join("\n")
        )
    }

    /// ACP connectors, team collaboration, and optional computer control.
    pub(super) fn mcp_servers(&self, slot: Option<&str>) -> Result<Value> {
        let Ok(exe) = std::env::current_exe() else {
            tracing::warn!("can't locate codync-host for MCP servers");
            return Ok(json!([]));
        };
        let port = self.hub.port;
        let mut servers: Vec<Value> =
            (if self.cfg.connectors.is_empty() { Vec::new() } else { crate::market::connectors(&self.hub.store)? })
                .iter()
                .filter(|c| self.cfg.connectors.contains(&c.id))
                .filter_map(|c| c.acp(&exe, port))
                .collect();
        let composio = crate::market::composio::enabled_for(&self.hub.store, &self.cfg.connectors)?;
        let builtin = [
            Some("connectors"),
            Some("chat"),
            Some("team"),
            Some("memory"),
            Some("routines"),
            self.cfg.computer.then_some("computer"),
            composio.then_some("composio"),
        ];
        for name in builtin.into_iter().flatten() {
            let env = if name == "memory" {
                json!([{"name":"CODYNC_MEMORY_LANE", "value":slot.unwrap_or_default()}])
            } else {
                json!([])
            };
            servers.push(json!({
                "name": name, "command": exe,
                "args": ["mcp", name, "--bot", self.cfg.id, "--port", port.to_string()],
                "env": env,
            }));
        }
        Ok(Value::Array(servers))
    }

    /// Drops the main ACP session so the next turn starts a fresh one.
    pub(super) fn forget_session(&mut self) {
        self.set_session(None, None);
    }
}

/// Claude's `_meta`: the bot's instructions appended to Claude Code's own system prompt.
pub(super) fn system_meta(system: &str) -> Value {
    json!({"systemPrompt": {"append": system}})
}

/// Shell commands that may start the bot's agent over ACP, best first.
/// `progress` receives short status lines ("Downloading Cursor…").
pub(crate) async fn launch_commands(cfg: &BotConfig, progress: impl Fn(&str)) -> Result<Vec<String>> {
    Ok(match cfg.command.as_deref().map(str::trim) {
        Some(c) if !c.is_empty() => vec![c.to_owned()],
        _ => crate::agent::backends::launch_candidates(&cfg.backend, progress)
            .await?
            .iter()
            .map(crate::agent::registry::Cmd::acp)
            .collect(),
    })
}

/// Switches a new session (`session/new` answered `res`) to the bot's chosen model.
pub(crate) async fn select_model(acp: &Acp, res: &Value, sid: &str, cfg: &BotConfig) -> Result<()> {
    let Some(model) = cfg.model.as_deref().filter(|m| !m.is_empty()) else { return Ok(()) };
    let r = match crate::agent::auth::model_config(res).and_then(|c| c["id"].as_str()) {
        Some(config_id) => {
            acp.request("session/set_config_option", json!({"sessionId": sid, "configId": config_id, "value": model}))
                .await
        }
        None => acp.request("session/set_model", json!({"sessionId": sid, "modelId": model})).await,
    };
    if let Err(e) = r {
        tracing::warn!(model, error = format!("{e:#}"), "couldn't set model");
        bail!("Couldn't select model {model}: {e:#}");
    }
    Ok(())
}

/// Spawns an ACP agent and completes the `initialize` handshake.
pub(crate) async fn start_agent(command: &str, cwd: &str, env: &[(String, String)], budget: Duration) -> Result<Conn> {
    let (acp, rx) = Acp::spawn(command, cwd, env)?;
    let init = tokio::time::timeout(
        budget,
        acp.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                // `session.compaction`: tell us when the agent summarizes its context.
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                    "session": {"compaction": {}},
                },
                "clientInfo": {"name": "codync", "title": "Codync", "version": env!("CARGO_PKG_VERSION")},
            }),
        ),
    )
    .await;
    let init = match init {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            acp.kill().await;
            return Err(e);
        }
        Err(_) => {
            acp.kill().await;
            return Err(anyhow!("the agent didn't answer the ACP handshake within {}s", budget.as_secs()));
        }
    };
    let sessions = SessionCaps {
        load: init["agentCapabilities"]["loadSession"].as_bool().unwrap_or(false),
        fork: init["agentCapabilities"]["sessionCapabilities"]["fork"].is_object(),
        close: init["agentCapabilities"]["sessionCapabilities"]["close"].is_object(),
    };
    let claude = init["agentCapabilities"]["_meta"]["claudeCode"].is_object();
    Ok(Conn { acp, rx, sessions, claude, loaded: HashMap::new(), allocation_uncertain: false })
}
