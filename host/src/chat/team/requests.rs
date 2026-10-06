//! Admission, capacity and the wait graph for bot requests.

use crate::LockExt;
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tokio::sync::watch;

use super::{BotRequest, MAX_PENDING_MESSAGES};

const MAX_REQUEST_HOPS: u8 = 8;

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
    pub(super) active: HashMap<String, u8>,
    pub(super) messages: usize,
}

impl Requests {
    pub(super) fn reserve_message(&self, from: &str) -> Result<u8> {
        let mut state = self.0.locked();
        let hops = state.next_hop(from)?;
        if state.messages >= MAX_PENDING_MESSAGES {
            bail!("too many pending bot messages");
        }
        state.messages += 1;
        Ok(hops)
    }

    pub(super) fn begin(&self, id: &str, from: &str, to: &str) -> Result<(watch::Receiver<bool>, u8)> {
        let mut state = self.0.locked();
        let hops = state.next_hop(from)?;
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
        Ok((rx, hops))
    }

    pub fn cancel_from(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        for request in state.pending.values().filter(|r| r.from == bot) {
            request.cancel.send_replace(true);
        }
    }

    pub fn start_turn(&self, bot: &str, request: Option<&BotRequest>) {
        self.0.locked().active.insert(bot.to_owned(), request.map_or(0, |r| r.hops));
    }

    pub fn cancel_bot(&self, bot: &str) {
        let mut state = self.0.locked();
        state.active.remove(bot);
        for request in state.pending.values().filter(|r| r.from == bot || r.to == bot) {
            request.cancel.send_replace(true);
        }
    }
}

impl RequestState {
    fn next_hop(&self, from: &str) -> Result<u8> {
        let hops = *self.active.get(from).ok_or_else(|| anyhow::anyhow!("the requesting bot is no longer working"))?;
        if hops >= MAX_REQUEST_HOPS {
            bail!("bot request hop limit ({MAX_REQUEST_HOPS}) reached; ask the user before starting another chain");
        }
        Ok(hops + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_chain_rejects_messages_and_asks_without_reserving_capacity() {
        let requests = Requests::default();
        requests.0.locked().active.insert("a".into(), MAX_REQUEST_HOPS);
        let message = requests.reserve_message("a");
        assert!(message.unwrap_err().to_string().contains("hop limit"));
        let ask = requests.begin("ask", "a", "b");
        assert!(ask.unwrap_err().to_string().contains("hop limit"));
        let state = requests.0.locked();
        assert_eq!(state.messages, 0);
        assert!(state.pending.is_empty());
        drop(state);
        requests.start_turn("a", None);
        assert_eq!(requests.reserve_message("a").unwrap(), 1);
    }

    #[test]
    fn last_allowed_handoff_is_admitted() {
        let requests = Requests::default();
        requests.0.locked().active.insert("a".into(), MAX_REQUEST_HOPS - 1);
        assert_eq!(requests.reserve_message("a").unwrap(), MAX_REQUEST_HOPS);
        let (_, hops) = requests.begin("ask", "a", "b").unwrap();
        assert_eq!(hops, MAX_REQUEST_HOPS);
    }
}
