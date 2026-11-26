//! Pause transitions and the admission points a pause has to close.
//!
//! A bot's `paused` flag is saved in its config, but saving it is not enough: routine triggers, another bot's request
//! and a turn's first prompt can all be inside setup when the user pauses. This gate gives every pause/resume one
//! ordered transaction, and makes the prompt write wait for that transaction while an admitted prompt lets the
//! transition wait for the write it already accepted. In-memory by design: a request or a mailbox command does not
//! outlive the host process.

use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A pipe that stays full must not hold its bot's actor or pause transaction forever.
pub const PROMPT_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Pause {
    /// One update at a time, held from the config's pause flag through its effects.
    transition: Mutex<()>,
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    /// Bots whose pause transaction has closed admission.
    blocked: HashSet<String>,
    /// How many pauses each bot has begun. A request made before one is refused even if a resume wins the race.
    epochs: HashMap<String, u64>,
    /// Prompt writes admitted before a pause: the transition waits for them to reach the agent.
    writing: HashMap<String, usize>,
}

/// One read of a bot's pause generation. Not a second counter: `epoch` is [`Pause::epoch`], and
/// `blocked` is the admission bit `block` sets in the same update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generation {
    pub epoch: u64,
    pub blocked: bool,
}

/// A prompt write admitted before a pause. Dropped once the write has reached the agent (or failed).
pub struct Admission<'a> {
    pause: &'a Pause,
    bot: String,
}

impl Pause {
    /// The exclusive pause transaction. Hold it from the pause flag's save through every effect that must not
    /// interleave with a resume, and through routine work that must not slip between a pause and its cleanup.
    pub fn transition(&self) -> MutexGuard<'_, ()> {
        self.transition.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Closes new prompt writes for `bot`, records the pause, and waits for a write admitted just before it.
    /// The writer has the same deadline, after which it kills its agent and drops the admission; the timeout here
    /// is a final bound, not permission for a broken agent to hold a pause indefinitely.
    pub fn block(&self, bot: &str) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *state.epochs.entry(bot.to_owned()).or_default() += 1;
        state.blocked.insert(bot.to_owned());
        let deadline = Instant::now() + PROMPT_WRITE_TIMEOUT;
        while state.writing.get(bot).is_some_and(|writing| *writing > 0) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                tracing::error!(%bot, "a prompt write stayed admitted past its deadline; continuing the pause");
                break;
            }
            let (waiting, timeout) =
                self.wake.wait_timeout(state, remaining).unwrap_or_else(std::sync::PoisonError::into_inner);
            state = waiting;
            if timeout.timed_out() && state.writing.get(bot).is_some_and(|writing| *writing > 0) {
                tracing::error!(%bot, "a prompt write stayed admitted past its deadline; continuing the pause");
                break;
            }
        }
    }

    /// Opens prompt writes again. The caller has already discarded work that came due while blocked.
    pub fn unblock(&self, bot: &str) {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked.remove(bot);
    }

    /// Whether the transition for `bot` has closed its admission.
    pub fn blocked(&self, bot: &str) -> bool {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked.contains(bot)
    }

    /// The pause generation `bot` is on. It changes only when a pause begins.
    pub fn epoch(&self, bot: &str) -> u64 {
        self.generation(bot).epoch
    }

    /// The generation and whether admission is closed, from one lock. A resume does not move the
    /// generation, so the two together are what a message or a notice has to remember: the epoch
    /// alone cannot tell "arrived during this pause" from "arrived after its resume".
    pub fn generation(&self, bot: &str) -> Generation {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Generation { epoch: state.epochs.get(bot).copied().unwrap_or(0), blocked: state.blocked.contains(bot) }
    }

    /// Admits one prompt write only if no pause has begun since `epoch` was read. A pause that started and a resume
    /// that finished during the turn's setup still invalidates it. The returned ticket keeps a pause transition
    /// waiting until the write reaches the agent.
    pub fn admit_prompt(&self, bot: &str, epoch: u64) -> Option<Admission<'_>> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.blocked.contains(bot) || state.epochs.get(bot).copied().unwrap_or(0) != epoch {
            return None;
        }
        *state.writing.entry(bot.to_owned()).or_default() += 1;
        Some(Admission { pause: self, bot: bot.to_owned() })
    }
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        let mut state = self.pause.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(writing) = state.writing.get_mut(&self.bot) {
            *writing = writing.saturating_sub(1);
            if *writing == 0 {
                state.writing.remove(&self.bot);
            }
        }
        self.pause.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn a_pause_waits_for_an_admitted_prompt_and_closes_new_ones() {
        let pause = std::sync::Arc::new(Pause::default());
        let admitted = pause.admit_prompt("bot", pause.epoch("bot")).unwrap();
        let (done, waited) = mpsc::channel();
        let pausing = pause.clone();
        std::thread::spawn(move || {
            pausing.block("bot");
            done.send(()).unwrap();
        });
        assert!(waited.recv_timeout(Duration::from_millis(100)).is_err(), "the pause returned before the prompt write");
        assert!(
            pause.admit_prompt("bot", pause.epoch("bot")).is_none(),
            "a second prompt was admitted during the pause"
        );
        drop(admitted);
        waited.recv_timeout(Duration::from_secs(1)).expect("the pause finished after the prompt write");
        assert!(pause.blocked("bot"));
        pause.unblock("bot");
        assert!(pause.admit_prompt("bot", pause.epoch("bot")).is_some(), "a turn started after the resume is admitted");
        assert!(
            pause.admit_prompt("bot", 0).is_none(),
            "a turn whose setup began before the pause is not admitted after the resume"
        );
    }

    #[test]
    fn pause_and_resume_effects_cannot_interleave() {
        let pause = std::sync::Arc::new(Pause::default());
        let (started, running) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let first = pause.clone();
        std::thread::spawn(move || {
            let _transition = first.transition();
            started.send(()).unwrap();
            held.recv().unwrap();
        });
        running.recv().unwrap();
        let (acquired, waiting) = mpsc::channel();
        let second = pause.clone();
        std::thread::spawn(move || {
            let _transition = second.transition();
            acquired.send(()).unwrap();
        });
        assert!(
            waiting.recv_timeout(Duration::from_millis(100)).is_err(),
            "a second pause/resume effect began while the first was still running"
        );
        release.send(()).unwrap();
        waiting.recv_timeout(Duration::from_secs(1)).expect("the second transition ran after the first");
    }

    #[test]
    fn a_pause_advances_the_generation_even_when_it_was_already_blocked() {
        let pause = Pause::default();
        assert_eq!(pause.epoch("bot"), 0);
        pause.block("bot");
        assert_eq!(pause.epoch("bot"), 1);
        pause.unblock("bot");
        pause.block("bot");
        assert_eq!(pause.epoch("bot"), 2);
    }
}
