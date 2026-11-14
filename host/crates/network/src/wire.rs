//! What computers say to each other: one JSON request and one JSON answer per QUIC stream.

use crate::roster::Roster;
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use iroh::endpoint::Connection;
use iroh::{EndpointAddr, EndpointId};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write;

pub const ALPN: &[u8] = b"codync-network/0";
pub const MAX_BODY: usize = 64 * 1024;
const MAX_META: usize = 16 * 1024;
/// The longest `from`, `to` and `reply_to` in bytes: an agent's address name (40 characters of
/// any script, a number, `agent-`), a letter id.
pub const MAX_ADDRESS: usize = 256;
/// The longest letter id (`random_hex(12)` makes 24).
pub(crate) const MAX_ID: usize = 64;
const MAX_FRAME: usize = 1024 * 1024;
const TICKET_PREFIX: &str = "codync-network:";

/// An agent a computer offers to the network.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub name: String,
    /// What it is good at, so other agents pick the right one.
    #[serde(default)]
    pub about: String,
    /// What it is, so others can pick one kind. Set from its [`Reach`](crate::Reach) for the
    /// sessions the network knocks on itself.
    #[serde(default)]
    pub kind: AgentKind,
}

/// What kind of agent is offered: a Codync bot, a Claude Code session, a Codex thread, a person
/// at a terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentKind {
    Codync,
    ClaudeCode,
    Codex,
    Terminal,
    /// Not said (a computer from before kinds), or one this version doesn't know.
    #[default]
    #[serde(other)]
    Other,
}

/// A letter on the wire. The sending computer is whoever opened the connection (iroh proves
/// its key), so it isn't written here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    #[serde(default = "yes")]
    pub wake: bool,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub hops: u8,
    #[serde(default)]
    pub meta: Option<Value>,
    pub sent_at: u64,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    /// The only request a computer outside the network may make.
    Join {
        network: EndpointId,
        secret: String,
        name: String,
    },
    /// Swap copies of the roster and the agents on offer.
    Hello {
        roster: Roster,
        agents: Vec<Agent>,
    },
    Deliver {
        envelope: Envelope,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Response {
    Welcome {
        roster: Roster,
    },
    Hello {
        roster: Roster,
        agents: Vec<Agent>,
    },
    Stored,
    Refused {
        reason: String,
    },
    /// Not yet: the receiving computer hasn't offered its agents since it started. The sender
    /// keeps the letter and tries again (an older sender, which can't read this, does the same).
    Later {
        reason: String,
    },
}

/// An invite: where to find the inviting computer and the one-time secret it waits for.
#[derive(Serialize, Deserialize)]
pub struct Ticket {
    pub network: EndpointId,
    pub secret: String,
    pub addr: EndpointAddr,
    /// What the inviting computer calls the network, for a computer in several of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Ticket {
    pub fn encode(&self) -> Result<String> {
        Ok(format!("{TICKET_PREFIX}{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?)))
    }

    pub fn decode(text: &str) -> Result<Self> {
        let body = text.trim().strip_prefix(TICKET_PREFIX).ok_or_else(|| anyhow!("not a codync-network invite"))?;
        let bytes = URL_SAFE_NO_PAD.decode(body).context("the invite is cut off or mistyped")?;
        serde_json::from_slice(&bytes).context("the invite is cut off or mistyped")
    }
}

/// What an invite says about its network, before joining it.
pub struct InviteInfo {
    /// The network's id (its root key), as [`Node::network_id`](crate::Node::network_id) gives it.
    pub network: String,
    /// What the inviting computer calls the network, if it named it.
    pub name: Option<String>,
}

pub fn read_invite(invite: &str) -> Result<InviteInfo> {
    let ticket = Ticket::decode(invite)?;
    Ok(InviteInfo { network: ticket.network.to_string(), name: ticket.name })
}

pub async fn call(conn: &Connection, request: &Request) -> Result<Response> {
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(&serde_json::to_vec(request)?).await?;
    send.finish()?;
    read(&mut recv).await
}

pub async fn read<T: DeserializeOwned>(recv: &mut iroh::endpoint::RecvStream) -> Result<T> {
    let bytes = recv.read_to_end(MAX_FRAME).await?;
    serde_json::from_slice(&bytes).context("unreadable frame")
}

pub async fn answer(send: &mut iroh::endpoint::SendStream, response: &Response) -> Result<()> {
    send.write_all(&serde_json::to_vec(response)?).await?;
    send.finish()?;
    Ok(())
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the OS random source is always there");
    hex(&buf)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    (0..text.len()).step_by(2).map(|i| text.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect()
}

pub fn now_ms() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}

pub fn check(envelope: &Envelope) -> Result<()> {
    if envelope.body.trim().is_empty() || envelope.body.len() > MAX_BODY {
        bail!("a message must be nonempty and at most {MAX_BODY} bytes");
    }
    if envelope.meta.as_ref().is_some_and(|m| m.to_string().len() > MAX_META) {
        bail!("a message's meta is at most {MAX_META} bytes");
    }
    let addresses = [Some(&envelope.from), Some(&envelope.to), envelope.reply_to.as_ref()];
    if envelope.id.is_empty()
        || envelope.id.len() > MAX_ID
        || addresses.into_iter().flatten().any(|a| a.len() > MAX_ADDRESS)
    {
        bail!("a message's id is 1–{MAX_ID} bytes and its addresses at most {MAX_ADDRESS}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letters_id_and_addresses_have_bounds() {
        let letter = |id: &str, from: String| Envelope {
            id: id.into(),
            from,
            to: "reviewer".into(),
            body: "hi".into(),
            wake: true,
            reply_to: None,
            hops: 0,
            meta: None,
            sent_at: 0,
        };
        assert!(check(&letter(&random_hex(12), "agent-lead".into())).is_ok());
        assert!(check(&letter(&random_hex(12), "x".repeat(1 << 20))).is_err());
        assert!(check(&letter(&random_hex(12), "審".repeat(40))).is_ok(), "any script's address names");
        assert!(check(&letter(&"a".repeat(100), "lead".into())).is_err());
        assert!(check(&letter("", "lead".into())).is_err());
        let mut reply = letter(&random_hex(12), "lead".into());
        reply.reply_to = Some("r".repeat(MAX_ADDRESS + 1));
        assert!(check(&reply).is_err());
    }

    #[test]
    fn tickets_round_trip_and_reject_garbage() {
        let addr = EndpointAddr::new(iroh::SecretKey::generate().public());
        let ticket = Ticket { network: addr.id, secret: random_hex(16), addr: addr.clone(), name: None };
        let text = ticket.encode().unwrap();
        let back = Ticket::decode(&format!("  {text}\n")).unwrap();
        assert_eq!((back.network, back.secret, back.addr), (ticket.network, ticket.secret, addr));
        assert!(Ticket::decode("hello").is_err());
        assert!(Ticket::decode(&text[..text.len() - 4]).is_err());
    }
}
