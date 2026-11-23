//! Codex's own app lists every session kept in its folder, a bot's among the user's own chats. A bot's new Codex
//! session is put in a "Codync" section there (`threadSection/*` of Codex's API), so that list tells them apart.
//! It goes there once, after the turn that first saved it (another process can't see it before), and stays where
//! the user moves it after that. A section the user removed comes back with the next new session.

use super::{Actor, Adapter};
use crate::agent::CODEX_CLIENT;
use crate::analytics::{self, Event};
use anyhow::Result;
use codync_agent::acp::Acp;
use codync_agent::backends;
use codync_agent::codex_app::{self, SectionStep};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::error::Elapsed;

const SECTION: &str = "Codync";
const TIMEOUT: Duration = Duration::from_secs(20);
/// One at a time: two sessions filed at once would each make the section.
static FILING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The new Codex sessions that aren't in the section yet.
#[derive(Default)]
pub(super) struct Unfiled(HashSet<String>);

impl Unfiled {
    /// Notes a session the agent just opened. Never one it loaded again: that one was noted when it was new, and
    /// is the user's to place since.
    pub(super) fn opened(&mut self, adapter: Adapter, session: &str) {
        if adapter == Adapter::Codex {
            self.0.insert(session.to_owned());
        }
    }

    /// The session to file after a turn of `turn_session` ended well, which saved it: that one, the first time.
    fn due(&mut self, turn_session: Option<&str>) -> Option<String> {
        self.0.take(turn_session?)
    }
}

impl Actor {
    /// After a turn that ended well: puts its session, when it is a new Codex one, in the section, in the
    /// background.
    pub(super) fn file_codex_session(&mut self) {
        let Some(thread) = self.unfiled.due(self.turn_session.as_deref()) else { return };
        if !backends::agent_checks_on("codex") {
            return;
        }
        let home = self.conn.as_ref().and_then(|conn| conn.codex_home.clone());
        let env: Vec<_> = home.into_iter().map(|home| ("CODEX_HOME".to_owned(), home)).collect();
        let hub = self.hub.clone();
        tokio::spawn(async move {
            let outcome = file_with(move || codex_app::start(&env), &thread, TIMEOUT).await;
            let mut properties = json!({"outcome": outcome.word()});
            if let Outcome::Failed(step) = outcome {
                properties["error"] = step.word().into();
            }
            analytics::capture(&hub, Event::CodexSessionFiled, properties.as_object().cloned().unwrap_or_default());
        });
    }
}

/// How filing a session ended (`codex_session_filed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Filed,
    /// No Codex CLI on the computer, or one from before sections.
    Unsupported,
    Failed(Failure),
}

impl Outcome {
    fn word(self) -> &'static str {
        match self {
            Self::Filed => "filed",
            Self::Unsupported => "unsupported",
            Self::Failed(_) => "failed",
        }
    }
}

/// The step filing failed at (the event's `error`). A request's error carries its step as context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    /// Codex's API didn't start, or refused the handshake.
    Start,
    Timeout,
    Step(SectionStep),
}

impl Failure {
    fn word(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Timeout => "timeout",
            Self::Step(step) => step.word(),
        }
    }

    /// The step `error` came from: the one it carries, else `codex_app::session`'s own.
    fn of(error: &anyhow::Error) -> Self {
        match error.downcast_ref::<SectionStep>() {
            Some(&step) => Self::Step(step),
            None if error.is::<Elapsed>() => Self::Timeout,
            None => Self::Start,
        }
    }
}

async fn file_with(
    start: impl FnOnce() -> Result<Option<Arc<Acp>>> + Send + 'static,
    thread: &str,
    timeout: Duration,
) -> Outcome {
    let _one_at_a_time = FILING.lock().await;
    let filed = codex_app::session(start, CODEX_CLIENT, timeout, async |client| {
        codex_app::move_to_section(client, thread, SECTION).await
    })
    .await;
    match filed {
        Ok(Some(true)) => Outcome::Filed,
        Ok(Some(false) | None) => Outcome::Unsupported,
        Err(error) => {
            let step = Failure::of(&error);
            let error = format!("{error:#}");
            tracing::warn!(step = step.word(), error, "couldn't put the bot's session in Codex's Codync section");
            Outcome::Failed(step)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture(mode: &str, timeout: Duration) -> Outcome {
        let outcome = file_with(
            codex_app::fixture(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_sections.py"),
                mode,
            ),
            "thread-1",
            timeout,
        )
        .await;
        codync_agent::acp::stopped().await;
        outcome
    }

    const PATIENT: Duration = Duration::from_secs(10);

    #[test]
    fn a_new_codex_session_is_due_once_after_its_own_turn() {
        let mut unfiled = Unfiled::default();
        unfiled.opened(Adapter::Codex, "chat");
        unfiled.opened(Adapter::Codex, "routine");
        unfiled.opened(Adapter::Claude, "claude");
        assert_eq!(unfiled.due(None), None);
        assert_eq!(unfiled.due(Some("loaded-again")), None);
        assert_eq!(unfiled.due(Some("claude")), None);
        // A session opened meanwhile doesn't take the chat's place.
        assert_eq!(unfiled.due(Some("routine")).as_deref(), Some("routine"));
        assert_eq!(unfiled.due(Some("chat")).as_deref(), Some("chat"));
        assert_eq!(unfiled.due(Some("chat")), None);
    }

    #[tokio::test]
    async fn a_session_joins_the_section_codex_already_has() {
        assert_eq!(fixture("existing", PATIENT).await, Outcome::Filed);
    }

    #[tokio::test]
    async fn the_section_is_made_when_no_page_lists_it() {
        assert_eq!(fixture("missing", PATIENT).await, Outcome::Filed);
    }

    #[tokio::test]
    async fn a_codex_from_before_sections_is_left_alone() {
        assert_eq!(fixture("old", PATIENT).await, Outcome::Unsupported);
    }

    #[tokio::test]
    async fn no_codex_on_the_computer_is_not_a_failure() {
        assert_eq!(file_with(|| Ok(None), "thread-1", PATIENT).await, Outcome::Unsupported);
    }

    #[tokio::test]
    async fn a_failure_names_the_step_it_happened_at() {
        let denied = || Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into());
        assert_eq!(file_with(denied, "thread-1", PATIENT).await, Outcome::Failed(Failure::Start));
        assert_eq!(fixture("hang", Duration::from_millis(100)).await, Outcome::Failed(Failure::Timeout));
        // Sections Codex has but couldn't list aren't an old Codex.
        assert_eq!(fixture("busy", PATIENT).await, Outcome::Failed(Failure::Step(SectionStep::List)));
        assert_eq!(fixture("unknown-thread", PATIENT).await, Outcome::Failed(Failure::Step(SectionStep::Move)));
    }
}
