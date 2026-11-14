//! Memory for coding-agent bots.
//!
//! The crate decides; the caller acts. It starts no processes, opens no network connections, calls no models
//! and spawns no threads. It reads and writes its own tables on the caller's SQLite connection, inside the
//! caller's transaction, so a memory change and whatever the caller records with it commit together.

pub mod check;
pub mod compaction;
pub mod dream;
pub mod evidence;
pub mod files;
pub mod legacy;
pub mod opening;
pub mod schema;
pub mod store;
pub mod tokens;

pub use rusqlite;
