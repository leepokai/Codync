//! One of the agent's sessions, with the controls a turn uses and what their answers mean.

use super::{Acp, Answer};
use anyhow::Result;
use serde_json::{Value, json};

/// A session the agent holds, by its id. Borrowed: an app keeps the `Acp` and the id, and takes this to act.
#[derive(Clone, Copy)]
pub struct Session<'a> {
    acp: &'a Acp,
    id: &'a str,
}

impl Acp {
    pub fn session<'a>(&'a self, id: &'a str) -> Session<'a> {
        Session { acp: self, id }
    }
}

impl Session<'_> {
    pub fn id(&self) -> &str {
        self.id
    }

    /// Starts a turn on `prompt` (ACP content blocks). Written before this returns, so a [`cancel`](Self::cancel)
    /// sent after it reaches the agent after the prompt; the answer is the turn's end ([`StopReason::of`]).
    pub async fn prompt(&self, prompt: Value) -> Result<Answer> {
        self.acp.send("session/prompt", json!({"sessionId": self.id, "prompt": prompt})).await
    }

    /// Asks the agent to stop the running turn, which then ends as `cancelled`.
    pub async fn cancel(&self) -> Result<()> {
        self.acp.notify("session/cancel", json!({"sessionId": self.id})).await
    }

    /// Hands `prompt` to the running turn (`_session/steering`, where [`Capabilities::steering`](super::Capabilities)
    /// says the agent can). A turn that just ended doesn't take it: it is left for the next one.
    pub async fn steer(&self, prompt: Value) -> Result<Steered> {
        let params = json!({
            "sessionId": self.id,
            "prompt": prompt,
            "_meta": {"steering": {"idleBehavior": "promptRequired"}},
        });
        let answer = self.acp.request("_session/steering", params).await?;
        Ok(match answer["outcome"].as_str() {
            Some("injected") => Steered::Joined,
            Some("promptRequired") => Steered::Next,
            other => Steered::Other(other.unwrap_or_default().to_owned()),
        })
    }
}

/// What the agent made of a steered message.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Steered {
    /// It joined the running turn.
    Joined,
    /// No turn was running to take it: it needs a turn of its own.
    Next,
    /// An outcome this crate doesn't know.
    Other(String),
}

/// Why a turn ended (a `session/prompt` answer's `stopReason`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    EndTurn,
    Cancelled,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    /// A reason this crate doesn't know, as sent.
    Other(String),
}

impl StopReason {
    /// The reason in a `session/prompt` answer; one that names none ended normally.
    pub fn of(answer: &Value) -> Self {
        match answer["stopReason"].as_str().unwrap_or("end_turn") {
            "end_turn" => Self::EndTurn,
            "cancelled" => Self::Cancelled,
            "max_tokens" => Self::MaxTokens,
            "max_turn_requests" => Self::MaxTurnRequests,
            "refusal" => Self::Refusal,
            other => Self::Other(other.to_owned()),
        }
    }

    /// As ACP names it.
    pub fn as_str(&self) -> &str {
        match self {
            Self::EndTurn => "end_turn",
            Self::Cancelled => "cancelled",
            Self::MaxTokens => "max_tokens",
            Self::MaxTurnRequests => "max_turn_requests",
            Self::Refusal => "refusal",
            Self::Other(other) => other,
        }
    }
}
