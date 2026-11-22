//! Each agent's plan usage, read the way the agent itself can report it, with no model turn: its CLI's own report
//! (`claude -p /usage`, `omp usage`, `amp usage`, `hermes usage`, `auggie account status`, `kilo profile`, `kiro-cli
//! /usage`, a `kimi` screen), its server's account API (Codex's app server, Copilot's headless server, Grok's ACP
//! billing extension, Antigravity's ACP `/usage` command or its IDE's language server), or its files on this computer
//! (OpenCode's database, Vibe's session logs, the `JetBrains` IDE quota file).
//!
//! Cursor is the one exception: its saved CLI or app login reads Cursor's dashboard service (the weekly unified
//! allowance) or the website's usage summary (legacy monthly plans), with the user's own token, which is never logged.
//!
//! Each agent's module reads it and returns [`Window`]s. Which agents to read, how often, and what to keep and show
//! are the app's. Readers that start the agent's ACP server, or that report how a check ended, take the app as [`App`].

pub mod amp;
pub mod antigravity;
pub mod auggie;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod grok;
pub mod hermes;
pub mod junie;
pub mod kilo;
pub mod kimi;
pub mod kiro;
pub mod omp;
pub mod opencode;
pub mod vibe;

mod acp_read;
mod command;
mod reading;

pub use reading::{Outcome, Stage, busy};

use crate::acp::{Acp, Incoming};
use futures::future::BoxFuture;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// What the readers that need it take from the app that runs the agents: starting one the way the app does, the
/// session kept for its readings, and how each check ended.
pub trait App: Sync {
    /// `backend`'s ACP server past `initialize`, on the computer's own login with the keys the app keeps for it,
    /// `extra` added to its environment. A first start can download the agent: no timeout wraps this.
    fn start<'a>(&'a self, backend: &'a str, extra: &'a [(&'a str, &'a str)])
    -> BoxFuture<'a, anyhow::Result<Started>>;

    /// The session kept for `backend`'s readings, when there is one.
    fn kept_session(&self, backend: &str) -> Option<String>;

    /// Keeps `id` as `backend`'s session for the next readings: each new session leaves a conversation behind.
    fn keep_session(&self, backend: &str, id: &str) -> anyhow::Result<()>;

    /// How a check of `agent` ended: read, or the [`Stage`] it stopped at. A check that found nothing to read (the
    /// CLI isn't installed, signed out) reports nothing.
    fn report(&self, agent: &str, check: Result<(), Stage>);
}

/// An agent [`App::start`] started.
pub struct Started {
    pub acp: Arc<Acp>,
    /// Must stay open: the reader stops when nobody listens, and the agent then never answers.
    pub inbox: mpsc::UnboundedReceiver<Incoming>,
    /// Its answer to `initialize`.
    pub init: Value,
    /// The folder it runs in, where a reading's session opens.
    pub dir: PathBuf,
}

/// One limit of a plan, or a note beside it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub id: String,
    pub label: String,
    /// Share of the cap already used, 0–100. Zero when [`Self::text`] is set.
    pub percent: f64,
    /// Epoch milliseconds, when known.
    pub resets_at: Option<i64>,
    /// Human text when only that is known ("Sep 26 at 12pm (Asia/Taipei)").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_text: Option<String>,
    /// A reading that isn't a share of a cap ("$4.20", "1.2M tokens", "Unlimited").
    /// Shown instead of the percent, and it never counts as the tightest limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Window {
    /// A limit: `percent` of it used, resetting at `resets_at` (epoch milliseconds) when known.
    pub fn new(id: impl Into<String>, label: impl Into<String>, percent: f64, resets_at: Option<i64>) -> Self {
        Self { id: id.into(), label: label.into(), percent, resets_at, resets_text: None, text: None }
    }

    /// A row with no bar: today's tokens, an uncapped balance, an unlimited plan.
    pub fn note(id: impl Into<String>, label: impl Into<String>, text: impl Into<String>) -> Self {
        Self { text: Some(text.into()), ..Self::new(id, label, 0.0, None) }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn iso_ms(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|at| at.timestamp_millis())
}

/// A window id made from its label.
fn slug(label: &str) -> String {
    label.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
}

fn finite_i64(n: f64) -> Option<i64> {
    if !n.is_finite() {
        return None;
    }
    let rounded = n.round();
    #[allow(clippy::cast_precision_loss)]
    let (min, max) = (i64::MIN as f64, i64::MAX as f64);
    if !(min..=max).contains(&rounded) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(rounded as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_not_serialized_as_a_missing_percent() {
        let window = Window::note("cost", "Cost today", "$1.50");
        let value = serde_json::to_value(&window).unwrap();
        assert_eq!(value["text"], "$1.50");
        assert_eq!(value["percent"], 0.0);
        assert!(value.get("resetsText").is_none());
    }
}
