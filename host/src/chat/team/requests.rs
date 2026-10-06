//! Admission, capacity and the wait graph for bot requests.

use crate::LockExt;
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tokio::sync::watch;

use super::MAX_PENDING_MESSAGES;

pub(super) struct Request {
    from: String,
    to: String,
    cancel: watch::Sender<bool>,
}

/// The wait graph includes queued requests, not only running recipients.
/// Reserving an edge and checking for cycles happen under the same lock.
#[derive(Default)]
pub struct Requests(pub(super) Mutex<RequestState>);

#[derive(Default)]
pub(super) struct RequestState {
    pub(super) pending: HashMap<String, Request>,
    pub(super) active: HashSet<String>,
    pub(super) messages: usize,
}

impl Requests {
    pub(super) fn reserve_message(&self, from: &str) -> Result<()> {
        let mut state = self.0.locked();
        if !state.active.contains(from) {
            bail!("the requesting bot is no longer working");
        }
        if state.messages >= MAX_PENDING_MESSAGES {
            bail!("too many pending bot messages");
        }
        state.messages += 1;
        Ok(())
    }

    pub(super) fn begin(&self, id: &str, from: &str, to: &str) -> Result<watch::Receiver<bool>> {
        let mut state = self.0.locked();
        if !state.active.contains(from) {
            bail!("the requesting bot is no longer working");
        }
        let requests = &mut state.pending;
        if from == to {
            bail!("a bot cannot ask itself");
        }
        if requests.len() >= 64 {
            bail!("too many pending bot requests");
        }
        if requests.values().any(|r| r.from == from && r.to == to) {
            bail!("already waiting for this bot");
        }
        let mut pending = vec![to];
        let mut seen = HashSet::new();
        while let Some(bot) = pending.pop() {
            if bot == from {
                bail!("this request would make bots wait for each other; finish the current request first");
            }
            if seen.insert(bot) {
                pending.extend(requests.values().filter(|r| r.from == bot).map(|r| r.to.as_str()));
            }
        }
        let (cancel, rx) = watch::channel(false);
        requests.insert(id.to_owned(), Request { from: from.to_owned(), to: to.to_owned(), cancel });
        Ok(rx)
    }

    pub fn cancel_from(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        for request in state.pending.values().filter(|r| r.from == bot) {
            request.cancel.send_replace(true);
        }
    }

    pub fn start_turn(&self, bot: &str) {
        self.0.locked().active.insert(bot.to_owned());
    }

    pub fn cancel_bot(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        for request in state.pending.values().filter(|r| r.from == bot || r.to == bot) {
            request.cancel.send_replace(true);
        }
    }
}

