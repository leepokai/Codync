//! The values an app hands the network and gets back from it.

use crate::roster::Role;
use crate::wire::{self, Agent, Envelope};
use anyhow::{Result, anyhow, bail};
use iroh::EndpointId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;
use std::str::FromStr;

/// How computers reach each other when they can't connect directly. Written as `public`,
/// `private` or the relay's URL, also in JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Connector {
    /// iroh's public relays and address lookup. Free, rate-limited, meant for trying things out.
    #[default]
    Public,
    /// Your own `iroh-relay`, and nothing of anyone else's.
    Relay(String),
    /// No relay at all: only computers reachable directly (same network, Tailscale, VPN).
    Private,
}

impl FromStr for Connector {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            url if url.starts_with("https://") || url.starts_with("http://") => Ok(Self::Relay(url.into())),
            _ => bail!("connector is public, private or a relay URL"),
        }
    }
}

impl fmt::Display for Connector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public => f.write_str("public"),
            Self::Private => f.write_str("private"),
            Self::Relay(url) => f.write_str(url),
        }
    }
}

impl Serialize for Connector {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Connector {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

/// `agent@computer`, like ssh's `user@host`. A bare `agent` is looked up with
/// [`Node::resolve`](crate::Node::resolve).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub agent: String,
    pub computer: String,
}

impl FromStr for Address {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let (agent, computer) = s.trim().rsplit_once('@').ok_or_else(|| anyhow!("an address is agent@computer"))?;
        if agent.is_empty() || computer.is_empty() {
            bail!("an address is agent@computer");
        }
        Ok(Self { agent: agent.into(), computer: computer.into() })
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.agent, self.computer)
    }
}

/// A message on its way out. Ways of working together (requests and replies, rooms, task
/// boards…) are built on letters: `reply_to` links them, `meta` carries the app's own structure,
/// and the network never reads either.
#[derive(Clone, Debug)]
pub struct Letter {
    /// Made here, so the sender knows it before the letter goes (a reply names it in `reply_to`).
    pub id: String,
    /// The sending agent: one of this computer's.
    pub from: String,
    pub to: Address,
    pub body: String,
    /// `false` makes it a notice: it waits for the agent's next turn instead of starting one.
    pub wake: bool,
    pub reply_to: Option<String>,
    /// Handoffs so far in this chain; 0 when a person started it.
    pub hops: u8,
    pub meta: Option<Value>,
}

impl Letter {
    pub fn new(from: impl Into<String>, to: Address, body: impl Into<String>) -> Self {
        Self {
            id: wire::random_hex(12),
            from: from.into(),
            to,
            body: body.into(),
            wake: true,
            reply_to: None,
            hops: 0,
            meta: None,
        }
    }

    #[must_use]
    pub fn notice(mut self) -> Self {
        self.wake = false;
        self
    }

    #[must_use]
    pub fn reply_to(mut self, id: impl Into<String>) -> Self {
        self.reply_to = Some(id.into());
        self
    }

    #[must_use]
    pub fn hops(mut self, hops: u8) -> Self {
        self.hops = hops;
        self
    }

    #[must_use]
    pub fn meta(mut self, meta: Value) -> Self {
        self.meta = Some(meta);
        self
    }

    /// The letter as it goes on the wire: the receiving computer knows the sender by its key.
    pub(crate) fn seal(self, sent_at: u64) -> Envelope {
        let Self { id, from, to, body, wake, reply_to, hops, meta } = self;
        Envelope { id, from, to: to.agent, body, wake, reply_to, hops, meta, sent_at }
    }
}

/// A message that reached one of this computer's agents.
#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
    pub from: Address,
    pub to: String,
    pub body: String,
    pub wake: bool,
    pub reply_to: Option<String>,
    pub hops: u8,
    pub meta: Option<Value>,
}

impl Envelope {
    /// The letter as the agent gets it, from the computer that sent it.
    pub(crate) fn open(self, computer: String) -> Message {
        let Self { id, from, to, body, wake, reply_to, hops, meta, sent_at: _ } = self;
        Message { id, from: Address { agent: from, computer }, to, body, wake, reply_to, hops, meta }
    }
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub id: EndpointId,
    pub name: String,
    pub role: Role,
    pub me: bool,
    /// It answered the last time this computer asked (or wrote).
    pub online: bool,
    pub agents: Vec<Agent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sent {
    /// It reached the agent: the app took it, or the agent's own harness did.
    Delivered,
    /// The computer is away; it gets the letter when it's back (within a day).
    Queued,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn connectors_read_and_write_as_text() {
        for text in ["public", "private", "https://relay.example"] {
            let connector: Connector = serde_json::from_value(json!(text)).unwrap();
            assert_eq!(serde_json::to_value(&connector).unwrap(), text);
        }
        assert!(serde_json::from_value::<Connector>(json!("ftp://relay.example")).is_err());
    }
}
