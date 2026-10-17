//! The agent process and its ACP sessions: launch, handshake, new/load/fork, thread sessions.

use super::{Actor, Conn, NoticeStyle, SessionCaps};
use crate::agent::acp::{self, Acp};
use crate::store::{BotConfig, Entry, EntryKind, Lane, lane_key};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

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
    pub(super) fn mcp_servers(&self) -> Result<Value> {
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
                json!([{"name":"CODYNC_MEMORY_LANE", "value":self.slot(&self.lane).unwrap_or_default()}])
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

    /// Memory failed to start: said once per actor, then the bot works without it.
    pub(super) fn memory_unavailable(&mut self, error: &anyhow::Error) {
        tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "memory unavailable");
        if !std::mem::replace(&mut self.memory_warned, true) {
            self.notice(
                &format!("Memory is unavailable; the bot works without it for now. {error:#}"),
                NoticeStyle::Info,
            );
        }
    }

    /// Drops the main ACP session so the next turn starts a fresh one.
    pub(super) fn forget_session(&mut self) {
        if self.conn.as_ref().is_some_and(|conn| conn.allocation_uncertain) {
            self.conn = None;
        }
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

/// Switches a session (`session/new` or `session/load` answered `res`) to the bot's
/// chosen model, then sets that model's options (effort, fast…). Returns the answer holding
/// the session's config options now: a model can change them.
pub(crate) async fn select_model(acp: &Acp, res: &Value, sid: &str, cfg: &BotConfig) -> Result<Value> {
    let Some(model) = cfg.model.as_deref().filter(|m| !m.is_empty()) else { return Ok(res.clone()) };
    let Some(config_id) = crate::agent::auth::model_config(res).and_then(|c| c["id"].as_str()) else {
        let r = acp.request("session/set_model", json!({"sessionId": sid, "modelId": model})).await;
        return r.map(|_| res.clone()).map_err(|e| model_error(model, &e));
    };
    let (model, mut options) = split_variant(res, model);
    options.extend(cfg.model_options.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let set = |id: &str, value: &str| {
        acp.request("session/set_config_option", json!({"sessionId": sid, "configId": id, "value": value}))
    };
    let mut current = set(config_id, model).await.map_err(|e| model_error(model, &e))?;
    for (id, value) in options {
        let answer = set(id, value).await.map_err(|e| model_error(&format!("{model} with {id}={value}"), &e))?;
        if answer["configOptions"].is_array() {
            current = answer;
        }
    }
    Ok(if current["configOptions"].is_array() { current } else { res.clone() })
}

fn model_error(model: &str, e: &anyhow::Error) -> anyhow::Error {
    tracing::warn!(model, error = format!("{e:#}"), "couldn't set model");
    anyhow!("Couldn't select model {model}: {e:#}")
}

/// Cursor names a model variant `grok-4.7[reasoning_effort=high,fast=true]`. When the agent
/// lists plain models instead, the bracket becomes options of the plain model.
fn split_variant<'a>(res: &Value, model: &'a str) -> (&'a str, Vec<(&'a str, &'a str)>) {
    let listed = |id: &str| crate::agent::auth::model_ids(res).iter().any(|m| m == id);
    if let Some((name, params)) = model.strip_suffix(']').and_then(|m| m.split_once('['))
        && let Some(params) =
            params.split(',').filter(|p| !p.is_empty()).map(|p| p.split_once('=')).collect::<Option<Vec<_>>>()
        && !listed(model)
        && listed(name)
    {
        return (name, params);
    }
    (model, Vec::new())
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
                // `parameterizedModelPicker`: Cursor lists plain models with their options
                // (effort, fast…) instead of one fixed variant each; model discovery asks the same.
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                    "session": {"compaction": {}},
                    "_meta": {"parameterizedModelPicker": true},
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
            let message = format!("the agent didn't answer the ACP handshake within {}s", budget.as_secs());
            return Err(acp::Failure::error(acp::FailureKind::Timeout, message));
        }
    };
    let sessions = SessionCaps {
        load: init["agentCapabilities"]["loadSession"].as_bool().unwrap_or(false),
        fork: init["agentCapabilities"]["sessionCapabilities"]["fork"].is_object(),
        close: init["agentCapabilities"]["sessionCapabilities"]["close"].is_object(),
    };
    let claude = init["agentCapabilities"]["_meta"]["claudeCode"].is_object();
    Ok(Conn {
        acp,
        rx,
        sessions,
        claude,
        loaded: HashMap::new(),
        started: Instant::now(),
        allocation_uncertain: false,
        pending_models: HashMap::new(),
        fast_options: HashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_ids_become_options_only_when_the_agent_lists_plain_models() {
        let plain = json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
            "options": [{"value": "grok-4.7"}]}]});
        let variant = "grok-4.7[context=256k,reasoning_effort=medium,fast=false]";
        assert_eq!(
            split_variant(&plain, variant),
            ("grok-4.7", vec![("context", "256k"), ("reasoning_effort", "medium"), ("fast", "false")])
        );
        assert_eq!(split_variant(&plain, "grok-4.7"), ("grok-4.7", vec![]));
        let variants = json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
            "options": [{"value": variant}]}]});
        assert_eq!(split_variant(&variants, variant), (variant, vec![]));
        assert_eq!(split_variant(&plain, "other[x=1]"), ("other[x=1]", vec![]));
        // Claude's `opus[1m]` is a model id of its own, not a variant.
        assert_eq!(split_variant(&json!({"models": {"availableModels": [{"modelId": "opus"}]}}), "opus[1m]").1, vec![]);
        // Cursor's Auto: `default[]` with variants, `default` without.
        let auto = json!({"models": {"availableModels": [{"modelId": "default"}]}});
        assert_eq!(split_variant(&auto, "default[]"), ("default", vec![]));
    }
}
