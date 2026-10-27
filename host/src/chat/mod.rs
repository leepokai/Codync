//! Conversation features on top of bots: messages to the user, reactions and quote replies, group room turns,
//! bot-to-bot requests, artifacts and the Library, team management, a bot's own fallbacks, prompt snapshots, long-term memory, automatic bot names and the lead bot.

pub mod artifacts;
pub mod calls;
pub mod context;
pub mod context_usage;
pub mod fallbacks;
pub mod files;
pub mod group;
pub mod handoff;
pub mod lead;
pub mod library;
pub mod manage;
pub mod memory;
pub mod messages;
pub mod naming;
pub mod oneshot;
pub mod outbox;
pub mod profile;
pub mod prompt;
pub mod reactions;
pub mod replies;
pub mod skills;
pub mod team;
pub mod uploads;
