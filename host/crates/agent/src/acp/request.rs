//! A request the agent sent the app (a permission, a question, …), answered once: by the app, or with what refuses it
//! when the app drops it unanswered, so the agent never waits on a request no one holds.

use super::{Failure, FailureKind, Writer};
use crate::events::ToolCall;
use anyhow::Result;
use serde_json::{Value, json};
use std::sync::Weak;
use tokio::runtime::Handle;

/// `session/request_permission`.
pub const PERMISSION: &str = "session/request_permission";
/// `elicitation/create` (unstable in ACP v1): the agent's own question to the user.
pub const ELICITATION: &str = "elicitation/create";
/// JSON-RPC's "method not found".
pub const METHOD_NOT_FOUND: i64 = -32601;

/// A request from the agent. Dropped unanswered, it answers by itself: a permission as `cancelled`, a question as
/// `cancel`, anything else as not supported.
pub struct Request {
    id: Value,
    method: String,
    params: Value,
    /// Not the agent itself: a card holding a request mustn't keep the agent's process alive.
    writer: Weak<Writer>,
    /// The runtime reading the agent: answers go out on it, so one reaches the agent from a thread outside any runtime
    /// (a UI dropping a card) and from a caller that stops waiting for it.
    runtime: Handle,
    answered: bool,
}

impl Request {
    /// Made on the runtime reading the agent.
    pub(super) fn new(id: Value, method: String, params: Value, writer: Weak<Writer>) -> Self {
        Self { id, method, params, writer, runtime: Handle::current(), answered: false }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    /// The session it is about.
    pub fn session(&self) -> Option<&str> {
        self.params["sessionId"].as_str()
    }

    /// The permission it asks for, when it is a `session/request_permission`.
    pub fn permission(&self) -> Option<Permission> {
        (self.method == PERMISSION).then(|| Permission::parse(&self.params))
    }

    /// Answers it with `result`.
    pub async fn respond(self, result: Value) -> Result<()> {
        let msg = json!({"jsonrpc": "2.0", "id": self.id, "result": result});
        self.send(msg).await
    }

    /// Answers it with an error.
    pub async fn refuse(self, code: i64, message: &str) -> Result<()> {
        let msg = json!({"jsonrpc": "2.0", "id": self.id, "error": {"code": code, "message": message}});
        self.send(msg).await
    }

    /// Answers a permission with the option picked, or `cancelled` without one.
    pub async fn choose(self, option: Option<&str>) -> Result<()> {
        self.respond(permission_answer(option)).await
    }

    /// Writes its answer on a task of its own, which finishes even when the caller stops waiting.
    async fn send(mut self, msg: Value) -> Result<()> {
        self.answered = true;
        let Some(writer) = self.writer.upgrade() else {
            return Err(Failure::error(FailureKind::Exited, "agent stopped"));
        };
        self.runtime.spawn(async move { writer.write(msg).await }).await?
    }

    /// What answers it when no one did.
    fn unanswered(&self) -> Value {
        let id = self.id.clone();
        match self.method.as_str() {
            PERMISSION => json!({"jsonrpc": "2.0", "id": id, "result": permission_answer(None)}),
            ELICITATION => json!({"jsonrpc": "2.0", "id": id, "result": {"action": "cancel"}}),
            method => json!({"jsonrpc": "2.0", "id": id,
                "error": {"code": METHOD_NOT_FOUND, "message": format!("{method} is not supported")}}),
        }
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        if self.answered {
            return;
        }
        // Gone with the agent: nothing waits for an answer.
        let Some(writer) = self.writer.upgrade() else {
            return;
        };
        let msg = self.unanswered();
        self.runtime.spawn(async move {
            let _ = writer.write(msg).await;
        });
    }
}

/// The result for a `session/request_permission`: the option picked, or `cancelled` without one.
fn permission_answer(option_id: Option<&str>) -> Value {
    match option_id {
        Some(id) => json!({"outcome": {"outcome": "selected", "optionId": id}}),
        None => json!({"outcome": {"outcome": "cancelled"}}),
    }
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request").field("id", &self.id).field("method", &self.method).finish_non_exhaustive()
    }
}

/// An agent's `session/request_permission`: the call it wants to make and the answers it offers.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Permission {
    pub session: Option<String>,
    pub tool: ToolCall,
    /// The command line the call runs, when it runs one (`rawInput.command`, a string or its words).
    pub command: Option<String>,
    pub options: Vec<PermissionOption>,
}

impl Permission {
    /// The request in a `session/request_permission`'s `params`.
    pub fn parse(params: &Value) -> Self {
        let tool = &params["toolCall"];
        let raw = &tool["rawInput"]["command"];
        let command = raw.as_str().map(str::to_owned).or_else(|| {
            raw.as_array().map(|words| words.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
        });
        let text = |o: &Value, key: &str| o[key].as_str().map(str::to_owned);
        let options = params["options"].as_array().into_iter().flatten();
        Self {
            session: params["sessionId"].as_str().map(str::to_owned),
            tool: ToolCall::parse(tool),
            command,
            options: options
                .map(|o| PermissionOption { id: text(o, "optionId"), name: text(o, "name"), kind: text(o, "kind") })
                .collect(),
        }
    }

    /// The option that lets the call run without asking again for it alone (`allow_once`, else `allow_always`),
    /// or the first one where the agent names no kind.
    pub fn allow(&self) -> Option<&str> {
        ["allow_once", "allow_always"]
            .iter()
            .find_map(|kind| self.options.iter().find(|o| o.kind.as_deref() == Some(kind)))
            .or_else(|| self.options.first())?
            .id
            .as_deref()
    }
}

/// One answer a permission request offers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PermissionOption {
    pub id: Option<String>,
    pub name: Option<String>,
    /// `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    pub kind: Option<String>,
}
