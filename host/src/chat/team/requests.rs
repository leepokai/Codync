//! Admission, capacity and the hop limit for bot messages.

use crate::LockExt;
use anyhow::{Result, bail};
use std::collections::HashMap;
use std::sync::Mutex;

use super::MAX_PENDING_MESSAGES;

const MAX_REQUEST_HOPS: u8 = 8;
pub(super) const MAX_PENDING_MESSAGES_PER_SENDER: usize = 16;

/// Nothing waits on a message, so there is no wait graph: capacity and the chain's hop count
/// bound what bots can start.
#[derive(Default)]
pub struct Requests(pub(super) Mutex<RequestState>);

/// A running turn, as bot messages see it.
#[derive(Clone, Copy)]
pub enum Turn {
    /// A turn in the bot's own chat, `hops` handoffs into a chain (0 for the user's own turn).
    Chat(u8),
    /// Answering another bot's request: the answer goes back in its reply, so it messages no one.
    Request,
}

#[derive(Default)]
pub(super) struct RequestState {
    /// Bots in a turn.
    pub(super) active: HashMap<String, Turn>,
    pub(super) messages: usize,
    pub(super) messages_by_sender: HashMap<String, usize>,
}

impl Requests {
    pub(super) fn reserve_message(&self, from: &str) -> Result<u8> {
        let mut state = self.0.locked();
        let hops = state.next_hop(from)?;
        if state.messages >= MAX_PENDING_MESSAGES {
            bail!("too many pending bot messages");
        }
        let count = state.messages_by_sender.entry(from.to_owned()).or_default();
        if *count >= MAX_PENDING_MESSAGES_PER_SENDER {
            bail!("too many pending bot messages from this bot (limit {MAX_PENDING_MESSAGES_PER_SENDER})");
        }
        *count += 1;
        state.messages += 1;
        Ok(hops)
    }

    pub(super) fn release_message(&self, from: &str) {
        let mut state = self.0.locked();
        state.messages -= 1;
        if let Some(count) = state.messages_by_sender.get_mut(from) {
            *count -= 1;
            if *count == 0 {
                state.messages_by_sender.remove(from);
            }
        }
    }

    pub fn start_turn(&self, bot: &str, turn: Turn) {
        self.0.locked().active.insert(bot.to_owned(), turn);
    }

    /// `bot`'s turn ended (or it was deleted): it can't send more until its next turn.
    pub fn end_turn(&self, bot: &str) {
        self.0.locked().active.remove(bot);
    }
}

impl RequestState {
    fn next_hop(&self, from: &str) -> Result<u8> {
        match self.active.get(from) {
            None => bail!("the requesting bot is no longer working"),
            Some(Turn::Request) => {
                bail!(
                    "message_bot isn't available while answering another bot's request: say what you need in your reply"
                )
            }
            Some(&Turn::Chat(hops)) if hops >= MAX_REQUEST_HOPS => {
                bail!("bot request hop limit ({MAX_REQUEST_HOPS}) reached; ask the user before starting another chain")
            }
            Some(&Turn::Chat(hops)) => Ok(hops + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_chain_rejects_messages_without_reserving_capacity() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS));
        let message = requests.reserve_message("a");
        assert!(message.unwrap_err().to_string().contains("hop limit"));
        assert_eq!(requests.0.locked().messages, 0);
        requests.start_turn("a", Turn::Chat(0));
        assert_eq!(requests.reserve_message("a").unwrap(), 1);
    }

    #[test]
    fn last_allowed_handoff_is_admitted() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(MAX_REQUEST_HOPS - 1));
        assert_eq!(requests.reserve_message("a").unwrap(), MAX_REQUEST_HOPS);
    }

    #[test]
    fn a_bot_answering_a_request_cannot_send() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Request);
        assert!(requests.reserve_message("a").unwrap_err().to_string().contains("answering another bot's request"));
        assert_eq!(requests.0.locked().messages, 0);
    }

    #[test]
    fn a_bot_outside_a_turn_cannot_send() {
        let requests = Requests::default();
        assert!(requests.reserve_message("a").unwrap_err().to_string().contains("no longer working"));
        requests.start_turn("a", Turn::Chat(0));
        requests.end_turn("a");
        assert!(requests.reserve_message("a").is_err());
    }

    #[test]
    fn sender_quota_leaves_capacity_for_other_bots() {
        let requests = Requests::default();
        for bot in ["a", "b"] {
            requests.start_turn(bot, Turn::Chat(0));
        }
        for _ in 0..MAX_PENDING_MESSAGES_PER_SENDER {
            requests.reserve_message("a").unwrap();
        }
        let result = requests.reserve_message("a");
        assert!(result.unwrap_err().to_string().contains("from this bot (limit 16)"));
        assert_eq!(requests.0.locked().messages, MAX_PENDING_MESSAGES_PER_SENDER);
        requests.reserve_message("b").unwrap();
        requests.release_message("a");
        requests.reserve_message("a").unwrap();
        for _ in 0..MAX_PENDING_MESSAGES_PER_SENDER {
            requests.release_message("a");
        }
        requests.release_message("b");
        let state = requests.0.locked();
        assert_eq!(state.messages, 0);
        assert!(state.messages_by_sender.is_empty());
    }

    #[test]
    fn global_capacity_still_bounds_messages_from_multiple_senders() {
        let requests = Requests::default();
        for bot in ["a", "b", "c", "d"] {
            requests.start_turn(bot, Turn::Chat(0));
            for _ in 0..MAX_PENDING_MESSAGES_PER_SENDER {
                requests.reserve_message(bot).unwrap();
            }
        }
        assert_eq!(requests.0.locked().messages, MAX_PENDING_MESSAGES);
        requests.start_turn("e", Turn::Chat(0));
        let result = requests.reserve_message("e");
        assert_eq!(result.unwrap_err().to_string(), "too many pending bot messages");
        assert!(!requests.0.locked().messages_by_sender.contains_key("e"));
        requests.release_message("a");
        requests.reserve_message("e").unwrap();
        assert_eq!(requests.0.locked().messages, MAX_PENDING_MESSAGES);
    }

    #[test]
    fn concurrent_reservations_cannot_exceed_the_sender_quota() {
        let requests = Requests::default();
        requests.start_turn("a", Turn::Chat(0));
        let barrier = std::sync::Barrier::new(MAX_PENDING_MESSAGES);
        let admitted = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..MAX_PENDING_MESSAGES)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        requests.reserve_message("a").is_ok()
                    })
                })
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).filter(|admitted| *admitted).count()
        });
        assert_eq!(admitted, MAX_PENDING_MESSAGES_PER_SENDER);
        assert_eq!(requests.0.locked().messages, admitted);
        for _ in 0..admitted {
            requests.release_message("a");
        }
        assert!(requests.0.locked().messages_by_sender.is_empty());
    }
}
