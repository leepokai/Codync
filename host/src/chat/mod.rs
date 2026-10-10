//! Conversation features on top of bots: messages to the user, group room turns,
//! bot-to-bot requests, prompt snapshots, long-term memory and automatic bot names.

pub mod context;
pub mod files;
pub mod group;
pub mod memory;
pub mod naming;
pub mod outbox;
pub mod team;
pub mod uploads;
