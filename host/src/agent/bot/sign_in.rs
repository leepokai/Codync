//! A turn its agent refused until the user signs in: the sign-in card that asks them to, in the chat they are in (a
//! background thread's goes to its bot's chat, marked as its task's, with a line in the thread's own transcript), holding
//! the turn's messages until they have. A thread's task isn't over then: it waits, needing the user, and goes on in
//! the same thread once they've signed in.

use super::{Actor, NoticeStyle};
use crate::agent::login_profiles::Login;
use crate::analytics::{self, Event};
use crate::chat::work::Outcome;
use crate::market::agent_sign_in;
use crate::remote::push::{self, AlertKind};
use crate::store::{Lane, now_ms};
use serde_json::json;
use std::time::Duration;

/// What became of the messages of a turn that failed for a sign-in. A thread's task goes on with them, so the end of
/// that turn doesn't end the task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Held {
    /// They went out again at once, to a new agent process.
    Again,
    /// A sign-in card holds them until the user signs in to agent `backend` (as the chat names it: `name`).
    SignIn { name: String, backend: String },
}

impl Held {
    /// What a thread whose task waits on this reads as its activity: the sign-in it needs. None once its messages went
    /// out again.
    pub(super) fn needs(&self) -> Option<String> {
        match self {
            Self::SignIn { name, .. } => Some(format!("Needs {name} sign-in")),
            Self::Again => None,
        }
    }
}

impl Actor {
    /// When `e` asks for signing in: marks the harness signed out and puts a sign-in card in the
    /// chat, which sends this turn's messages again once the user has signed in. Whether it did.
    /// A process older than the turn may hold a sign-in replaced since: a new one tries its messages first.
    pub(super) fn invite_sign_in(&mut self, e: &anyhow::Error) -> bool {
        let backend = self.cfg.backend.clone();
        if codync_agent::backends::harness(&backend).is_none() || !codync_agent::acp::needs_sign_in(e) {
            return false;
        }
        // The process keeps the credentials it started with (Claude's revoked token).
        self.restart_stale();
        let held = self.held_for_sign_in().to_vec();
        if self.process_predates_turn() && !held.is_empty() {
            match agent_sign_in::send_again(&self.hub, held.iter().map(String::as_str)) {
                Ok(()) => {
                    self.relay.held = Some(Held::Again);
                    return true;
                }
                Err(error) => tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't retry"),
            }
        }
        let login = Login::of(&self.hub.store, &self.cfg);
        self.note_signed_out(&login);
        self.sign_in_card(&login, &held)
    }

    /// `login` refused the bot's prompt for auth: a login profile, or the computer's own login. Only that one
    /// reads signed out, also over a check that still says signed in (`claude auth
    /// status` with a revoked token).
    pub(super) fn note_signed_out(&self, login: &Login) {
        if let Some(profile) = &login.profile {
            if crate::agent::login_profiles::note_signed_in(&self.hub.store, profile, false) {
                // Signed out, it leaves the Usage screens.
                crate::usage::login_profiles_changed(&self.hub);
            }
        } else {
            codync_agent::backends::set_signed_in(&login.backend, Some(false));
            codync_agent::backends::note_prompt_refused(&login.backend);
        }
    }

    /// The agent's process was already running when this turn started.
    fn process_predates_turn(&self) -> bool {
        let (Some(conn), Some(started_at)) = (&self.conn, self.hub.runtime(&self.id()).started_at) else {
            return false;
        };
        let turn_age = Duration::from_millis(u64::try_from(now_ms() - started_at).unwrap_or(0));
        conn.started.elapsed() > turn_age
    }

    /// `login`'s sign-in card where the user is, holding `entries`: in the chat, or, from a thread, in its bot's chat
    /// with a line in the thread's own saying so. Whether it's there.
    pub(super) fn sign_in_card(&mut self, login: &Login, entries: &[String]) -> bool {
        if codync_agent::backends::harness(&login.backend).is_none() {
            return false;
        }
        let mark = crate::chat::work::mark(&self.hub.store, &self.cfg);
        let (bot, lane) = match &mark {
            Some(mark) => (mark.bot.as_str(), Lane::main(&mark.bot)),
            None => (self.cfg.id.as_str(), self.lane.clone()),
        };
        if let Err(error) = agent_sign_in::invite(&self.hub, bot, &lane, login, entries, mark.as_ref()) {
            tracing::warn!(bot = %self.cfg.id, error = format!("{error:#}"), "couldn't ask to sign in");
            return false;
        }
        let name = self.named(&login.backend, login.profile.as_deref());
        if mark.is_some() {
            self.notice(&format!("Asked in the chat to sign in to {name}."), NoticeStyle::Info);
        }
        if !entries.is_empty() {
            self.relay.held = Some(Held::SignIn { name, backend: login.backend.clone() });
        }
        true
    }

    /// The turn is over: a thread's task ends with `outcome` (its last message is its report to its bot), unless it
    /// goes on past a sign-in, which this returns (its message held for one, or sent again). The bot's own turns
    /// always end.
    pub(super) fn end_task(&mut self, outcome: impl FnOnce() -> Outcome) -> Option<Held> {
        let held = self.relay.held.take().filter(|_| self.cfg.is_thread());
        if held.is_none() {
            self.turn_over(outcome());
        }
        held
    }

    /// After a thread's turn ended (its runtime reads `held.needs()`): a task that waits for a sign-in needs the
    /// user, who is told so in its bot's chat.
    pub(super) fn wait_for_sign_in(&self, held: Option<&Held>) {
        let Some(held @ Held::SignIn { backend, .. }) = held else { return };
        let properties = json!({"agent": backend, "source": self.task_source.kind()});
        analytics::capture(&self.hub, Event::WorkNeedsSignIn, properties.as_object().cloned().unwrap_or_default());
        if let Ok(Some(chat)) = self.hub.store.bot(self.cfg.owner()) {
            let title = format!("{} needs you", self.cfg.name);
            push::notify(
                &self.hub,
                &chat.config,
                None,
                &title,
                &held.needs().unwrap_or_default(),
                AlertKind::NeedsInput,
            );
        }
    }
}
