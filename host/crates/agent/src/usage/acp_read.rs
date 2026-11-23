//! Readings through an agent's own ACP server, which asks its provider with the agent's login.
//!
//! [`command_reading`] runs a slash command that the adapter answers itself, with no model turn. The
//! command runs only once the session has advertised it: sent to an agent that doesn't know it, it
//! would reach the model and spend the plan it was meant to read. One session per agent is resumed
//! for every reading, as each new session leaves a conversation behind on the computer.

use crate::acp::{Acp, Capabilities, Incoming};
use crate::events::Event;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

use super::reading::{self, Stage};
use super::{App, Started, Window};

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

/// `backend`'s server, past `initialize`. A first start can download its build, which is why no
/// timeout wraps this: one cut short would start over at every poll. The handshake has its own.
pub(super) async fn start(app: &dyn App, backend: &str) -> Result<Started, Stage> {
    app.start(backend, NO_BROWSER).await.map_err(|error| {
        tracing::debug!(backend, error = format!("{error:#}"), "couldn't start the agent for its usage");
        Stage::Start
    })
}

/// What `backend` answers to its `command`, through `parse`. `None`: no reading this time.
pub(super) async fn command_reading(
    app: &dyn App,
    backend: &str,
    command: &str,
    parse: fn(&str) -> Option<Vec<Window>>,
) -> Option<Vec<Window>> {
    reading::guarded(app, backend, async {
        let text = ask(app, backend, command).await?;
        parse(&text).ok_or(Stage::Parse)
    })
    .await
}

async fn ask(app: &dyn App, backend: &str, command: &str) -> Result<String, Stage> {
    let Started { acp, mut inbox, init, dir } = start(app, backend).await?;
    let mut talk = Talk { acp: &acp, inbox: &mut inbox, commands: None, text: String::new() };
    let reply = tokio::time::timeout(READ_TIMEOUT, talk.run(app, &init, &dir, backend, command)).await;
    acp.kill().await;
    reply.unwrap_or(Err(Stage::Timeout))
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
        app: &dyn App,
        init: &Value,
        dir: &Path,
        backend: &str,
        command: &str,
    ) -> Result<String, Stage> {
        let session = self.session(app, init, dir, backend).await.map_err(|error| {
            tracing::debug!(backend, error = format!("{error:#}"), "couldn't open the usage session");
            Stage::Session
        })?;
        self.wait_for_commands().await;
        if !self.commands.iter().flatten().any(|name| name == command) {
            return Err(Stage::NoCommand);
        }
        self.text.clear();
        let prompt = json!({"sessionId": session, "prompt": [{"type": "text", "text": format!("/{command}")}]});
        self.request("session/prompt", prompt).await.map_err(|error| {
            tracing::debug!(backend, error = format!("{error:#}"), "the usage command failed");
            Stage::Prompt
        })?;
        Ok(std::mem::take(&mut self.text))
    }

    /// The session kept for readings, or a new one to keep when it's gone or can't be resumed.
    async fn session(&mut self, app: &dyn App, init: &Value, dir: &Path, backend: &str) -> Result<String> {
        if let Some(id) = app.kept_session(backend).filter(|_| can_resume(init)) {
            let resume = json!({"sessionId": id, "cwd": dir, "mcpServers": []});
            if self.request("session/resume", resume).await.is_ok() {
                return Ok(id);
            }
        }
        let opened = self.request("session/new", json!({"cwd": dir, "mcpServers": []})).await?;
        let id = opened["sessionId"].as_str().context("the session has no id")?;
        app.keep_session(backend, id)?;
        Ok(id.to_owned())
    }

    async fn wait_for_commands(&mut self) {
        let wait = async {
            while self.commands.is_none() {
                let Some(incoming) = self.inbox.recv().await else { return };
                if self.take(incoming).is_err() {
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
                    Some(incoming) => self.take(incoming)?,
                    None => bail!("the agent closed"),
                }
            }
        };
        // The answer can overtake the updates sent before it.
        while let Ok(incoming) = self.inbox.try_recv() {
            self.take(incoming)?;
        }
        answered
    }

    fn take(&mut self, incoming: Incoming) -> Result<()> {
        match incoming {
            Incoming::Update(update) => self.note(update.event),
            // A request dropped unanswered refuses itself.
            Incoming::Notification { .. } | Incoming::Request(_) => {}
            Incoming::Closed { .. } => bail!("the agent closed"),
        }
        Ok(())
    }

    fn note(&mut self, event: Event) {
        match event {
            Event::Commands(commands) => self.commands = Some(commands.into_iter().map(|c| c.name).collect()),
            Event::Text { text, .. } => self.text.push_str(&text),
            _ => {}
        }
    }
}

fn can_resume(init: &Value) -> bool {
    Capabilities::from_initialize(init).resume
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;

    #[test]
    fn a_session_is_resumed_only_when_the_agent_offers_it() {
        assert!(can_resume(&json!({"agentCapabilities": {"sessionCapabilities": {"resume": {}}}})));
        assert!(!can_resume(&json!({"agentCapabilities": {"sessionCapabilities": {"list": {}}}})));
        assert!(!can_resume(&json!({})));
    }

    /// A fake agent's scratch folder, and the app's memory of its session.
    struct Fake {
        dir: std::path::PathBuf,
        sessions: std::sync::Mutex<std::collections::HashMap<String, String>>,
    }

    impl App for Fake {
        fn start<'a>(&'a self, _: &'a str, _: &'a [(&'a str, &'a str)]) -> BoxFuture<'a, Result<Started>> {
            Box::pin(async { bail!("the test starts its agent itself") })
        }

        fn kept_session(&self, backend: &str) -> Option<String> {
            self.sessions.lock().unwrap().get(backend).cloned()
        }

        fn keep_session(&self, backend: &str, id: &str) -> Result<()> {
            self.sessions.lock().unwrap().insert(backend.to_owned(), id.to_owned());
            Ok(())
        }

        fn report(&self, _: &str, _: Result<(), Stage>) {}
    }

    impl Fake {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("codync-usage-agent-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir, sessions: std::sync::Mutex::default() }
        }

        /// One reading from the fake agent, which advertises `advertises`, and the methods it was asked.
        async fn read(&self, advertises: &str) -> (Result<String, Stage>, Vec<String>) {
            let log = self.dir.join("calls.log");
            let _ = std::fs::remove_file(&log);
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage_command_agent.py");
            let env = [
                ("USAGE_AGENT_LOG".to_owned(), log.to_string_lossy().into_owned()),
                ("USAGE_AGENT_COMMAND".to_owned(), advertises.to_owned()),
            ];
            let (acp, mut inbox) =
                Acp::spawn(&crate::shell::python(&fixture), env!("CARGO_MANIFEST_DIR"), &env).unwrap();
            let init = acp.request("initialize", json!({"protocolVersion": 1})).await.unwrap();
            let mut talk = Talk { acp: &acp, inbox: &mut inbox, commands: None, text: String::new() };
            let outcome = talk.run(self, &init, &self.dir, "test-agent", "usage").await;
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
        assert_eq!(outcome, Err(Stage::NoCommand));
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
