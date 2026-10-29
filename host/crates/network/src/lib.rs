//! **codync-network** — Agent Tailscale.
//!
//! The agents on your computers find and message each other, peer to peer. It comes in layers,
//! so new ways of working together don't change the ones below:
//!
//! - **Computers** ([`Node`]): each has a key that is also its address, a copy of the network's
//!   roster, and joins with a one-time invite. Letters go straight from one computer to the other
//!   over QUIC ([iroh](https://iroh.computer)), through a relay that sees only ciphertext when no
//!   direct path exists ([`Connector`]), and wait in the sender's outbox while the other is away.
//! - **Letters** ([`Letter`], [`Message`]): text from one agent to another, which wakes it or, as a
//!   notice, waits for its next turn. `reply_to` links letters and `meta` carries an app's own
//!   structure, so requests and replies, rooms or task boards are built on letters, and the
//!   network never reads them.
//! - **Reach** ([`Reach`]): how a letter gets into the agent. The app delivers to its own agents;
//!   the network knocks on Claude Code sessions and Codex threads itself.
//! - **Budgets** ([`guard`]): nothing is sent unless an agent sends a letter or someone asks who
//!   is around, and the receiving computer refuses long chains, bursts and repeats.
//! - **Rules** ([`rules`]): the receiving computer decides which agents elsewhere may write to
//!   which of its own; with none, every member may.

pub mod guard;
mod node;
mod reach;
mod roster;
pub mod rules;
mod types;
mod wire;

pub use node::{INVITE_FOR, Node};
pub use reach::Reach;
pub use roster::Role;
pub use rules::{Access, Rule};
pub use types::{Address, Connector, Letter, Message, Peer, Sent};
pub use wire::{Agent, AgentKind, InviteInfo, MAX_ADDRESS, MAX_BODY, read_invite};
