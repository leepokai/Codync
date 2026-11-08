//! Readings through an agent's own ACP server, which asks its provider with the agent's login.
//!
//! [`command_reading`] runs a slash command that the adapter answers itself, with no model turn. The
//! command runs only once the session has advertised it: sent to an agent that doesn't know it, it
//! would reach the model and spend the plan it was meant to read. One session per agent is resumed
//! for every reading, as each new session leaves a conversation behind on the computer.

use crate::agent::acp::{Acp, Incoming};
use crate::agent::auth::{self, Started};
use crate::agent::backends;
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;

use super::reading::{self, UsageReadStage};
use super::{Window, services};

/// A signed-out agent waits here for a sign-in that never comes.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a session gets to advertise its commands, which follow its answer.
const COMMANDS_TIMEOUT: Duration = Duration::from_secs(5);
/// A check in the background must never open a browser to sign in. Python and most CLIs honor
/// this; Windows has no command that ignores the address, so its reader checks the login first.
#[cfg(unix)]
const NO_BROWSER: &[(&str, &str)] = &[("BROWSER", "true")];
#[cfg(windows)]
const NO_BROWSER: &[(&str, &str)] = &[];

/// What `reading` finds through `backend`'s server, one at a time. Nothing while a bot here runs on
/// an older build: installing the new one would remove it, and that bot's next start brings it.
pub(super) async fn guarded(
    hub: &Hub,
    backend: &str,
    reading: impl Future<Output = Result<Vec<Window>, UsageReadStage>>,
) -> Option<Vec<Window>> {
    if services::runs(&hub.store, backend) && backends::update_pending(backend) {
        return None;
    }
    reading::guarded(hub, backend, reading).await
}

/// `backend`'s server, past `initialize`. A first start can download its build, which is why no
/// timeout wraps this: one cut short would start over at every poll. The handshake has its own.
pub(super) async fn start(store: &Store, backend: &str) -> Result<Started, UsageReadStage> {
    auth::start_with(store, backend, NO_BROWSER).await.map_err(|error| {
        tracing::debug!(backend, error = format!("{error:#}"), "couldn't start the agent for its usage");
        UsageReadStage::Start
    })
}

/// What `backend` answers to its `command`, through `parse`. `None`: no reading this time.
pub(super) async fn command_reading(
    hub: &Hub,
    backend: &str,
    command: &str,
    parse: fn(&str) -> Option<Vec<Window>>,
) -> Option<Vec<Window>> {
    guarded(hub, backend, async {
        let text = ask(&hub.store, backend, command).await?;
        parse(&text).ok_or(UsageReadStage::Parse)
    })
    .await
}

async fn ask(store: &Store, backend: &str, command: &str) -> Result<String, UsageReadStage> {
    let Started { acp, mut inbox, init, .. } = start(store, backend).await?;
    let mut talk = Talk { acp: &acp, inbox: &mut inbox, commands: None, text: String::new() };
    let reply = tokio::time::timeout(READ_TIMEOUT, talk.run(store, &init, backend, command)).await;
    acp.kill().await;
    reply.unwrap_or(Err(UsageReadStage::Timeout))
}

/// One conversation with the started agent.
struct Talk<'a> {
    acp: &'a Acp,
    inbox: &'a mut mpsc::UnboundedReceiver<Incoming>,
    /// What the session advertised, once it has.
    commands: Option<Vec<String>>,
    /// The reply so far.
    text: String,
}

impl Talk<'_> {
    async fn run(
        &mut self,
        store: &Store,
        init: &Value,
        backend: &str,
        command: &str,
    ) -> Result<String, UsageReadStage> {
        let session = self.session(store, init, backend).await.map_err(|error| {
            tracing::debug!(backend, error = format!("{error:#}"), "couldn't open the usage session");
            UsageReadStage::Session
        })?;
        self.wait_for_commands().await;
        if !self.commands.iter().flatten().any(|name| name == command) {
            return Err(UsageReadStage::NoCommand);
        }
        self.text.clear();
        let prompt = json!({"sessionId": session, "prompt": [{"type": "text", "text": format!("/{command}")}]});
        self.request("session/prompt", prompt).await.map_err(|error| {
            tracing::debug!(backend, error = format!("{error:#}"), "the usage command failed");
            UsageReadStage::Prompt
        })?;
        Ok(std::mem::take(&mut self.text))
    }

    /// The session kept for readings, or a new one to keep when it's gone or can't be resumed.
    async fn session(&mut self, store: &Store, init: &Value, backend: &str) -> Result<String> {
        let dir = auth::probe_dir();
        let key = format!("usage.session.{backend}");
        if let Some(id) = store.kv_get(&key).filter(|_| can_resume(init)) {
            let resume = json!({"sessionId": id, "cwd": dir, "mcpServers": []});
            if self.request("session/resume", resume).await.is_ok() {
                return Ok(id);
            }
        }
        let opened = self.request("session/new", json!({"cwd": dir, "mcpServers": []})).await?;
        let id = opened["sessionId"].as_str().context("the session has no id")?;
        store.kv_set(&key, id)?;
        Ok(id.to_owned())
    }

    async fn wait_for_commands(&mut self) {
        let wait = async {
            while self.commands.is_none() {
                let Some(incoming) = self.inbox.recv().await else { return };
                if self.take(incoming).await.is_err() {
                    return;
                }
            }
        };
        // No list in time reads as no command.
        let _ = tokio::time::timeout(COMMANDS_TIMEOUT, wait).await;
    }

    /// `method`'s answer, keeping what the agent says meanwhile.
    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let answer = self.acp.send(method, params).await?.response();
        tokio::pin!(answer);
        let answered = loop {
            tokio::select! {
                answered = &mut answer => break answered,
                incoming = self.inbox.recv() => match incoming {
                    Some(incoming) => self.take(incoming).await?,
                    None => bail!("the agent closed"),
                }
            }
        };
        // The answer can overtake the updates sent before it.
        while let Ok(incoming) = self.inbox.try_recv() {
            self.take(incoming).await?;
        }
        answered
    }

    async fn take(&mut self, incoming: Incoming) -> Result<()> {
        match incoming {
            Incoming::Notification { method, params } if method == "session/update" => self.note(&params["update"]),
            Incoming::Notification { .. } => {}
            Incoming::Request { id, .. } => {
                self.acp.respond_error(id, -32601, "not supported in a usage check").await?;
            }
            Incoming::Closed { .. } => bail!("the agent closed"),
        }
        Ok(())
    }

    fn note(&mut self, update: &Value) {
        match update["sessionUpdate"].as_str() {
            Some("available_commands_update") => self.commands = Some(command_names(update)),
            Some("agent_message_chunk") => self.text.push_str(update["content"]["text"].as_str().unwrap_or_default()),
            _ => {}
        }
    }
}

fn can_resume(init: &Value) -> bool {
    init["agentCapabilities"]["sessionCapabilities"]["resume"].is_object()
}

fn command_names(update: &Value) -> Vec<String> {
    let commands = update["availableCommands"].as_array().into_iter().flatten();
    commands.filter_map(|command| command["name"].as_str().map(str::to_owned)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_resumed_only_when_the_agent_offers_it() {
        assert!(can_resume(&json!({"agentCapabilities": {"sessionCapabilities": {"resume": {}}}})));
        assert!(!can_resume(&json!({"agentCapabilities": {"sessionCapabilities": {"list": {}}}})));
        assert!(!can_resume(&json!({})));
    }

    #[test]
    fn the_advertised_commands_are_read_by_name() {
        let update = json!({"availableCommands": [{"name": "plan"}, {"name": "usage", "description": "x"}, {}]});
        assert_eq!(command_names(&update), ["plan", "usage"]);
    }

    /// A fake agent's scratch folder, and the host's memory of its session.
    struct Fake {
        dir: std::path::PathBuf,
        store: Store,
    }

    impl Fake {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("codync-usage-agent-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir, store: Store::open(std::path::Path::new(":memory:")).unwrap() }
        }

        /// One reading from the fake agent, which advertises `advertises`, and the methods it was asked.
        async fn read(&self, advertises: &str) -> (Result<String, UsageReadStage>, Vec<String>) {
            let log = self.dir.join("calls.log");
            let _ = std::fs::remove_file(&log);
            let fixture =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage_command_agent.py");
            let env = [
                ("USAGE_AGENT_LOG".to_owned(), log.to_string_lossy().into_owned()),
                ("USAGE_AGENT_COMMAND".to_owned(), advertises.to_owned()),
            ];
            let (acp, mut inbox) =
                Acp::spawn(&crate::shell::python(&fixture), env!("CARGO_MANIFEST_DIR"), &env).unwrap();
            let init = acp.request("initialize", json!({"protocolVersion": 1})).await.unwrap();
            let mut talk = Talk { acp: &acp, inbox: &mut inbox, commands: None, text: String::new() };
            let outcome = talk.run(&self.store, &init, "test-agent", "usage").await;
            acp.kill().await;
            let calls = std::fs::read_to_string(&log).unwrap();
            (outcome, calls.lines().map(str::to_owned).collect())
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[tokio::test]
    async fn a_command_the_session_does_not_advertise_is_never_sent() {
        let fake = Fake::new();
        let (outcome, calls) = fake.read("plan").await;
        assert_eq!(outcome, Err(UsageReadStage::NoCommand));
        assert!(!calls.iter().any(|call| call == "session/prompt"), "{calls:?}");
    }

    #[tokio::test]
    async fn readings_share_one_resumed_session() {
        let fake = Fake::new();
        let (first, calls) = fake.read("usage").await;
        assert_eq!(first.as_deref(), Ok("the report"));
        assert_eq!(calls, ["initialize", "session/new", "session/prompt"]);
        let (second, calls) = fake.read("usage").await;
        assert_eq!(second.as_deref(), Ok("the report"), "only the command's own reply counts");
        assert_eq!(calls, ["initialize", "session/resume", "session/prompt"]);
    }
}
