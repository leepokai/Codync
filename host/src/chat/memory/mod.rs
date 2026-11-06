//! Bot memory ([design](../../../../docs/design/bot-memory.md)): it records the person, not the work, in Markdown
//! files the bot keeps itself. The rules (checks, who wrote what, the opening, dreaming) live in the `codync_mem`
//! crate; this module runs them in the host: the files on disk, the session opening, the bot's memory tools, the
//! apps' read-only view, dreaming, and the one-time moves from the memory Codync kept before.

mod api;
mod dates;
mod dream;
mod file_api;
mod file_tools;
pub mod files;
mod import;
mod tools;
pub mod turns;

pub use api::call;
pub use dates::ymd;
pub use dream::{DreamEvent, spawn_dreamer};
pub use import::{add_facts, import_old_memory};
pub use tools::{INSTRUCTIONS, call as tool_call, tools};

use crate::hub::Hub;
use crate::store::{BotConfig, Store};

/// Whether a bot uses the memory all bots share. Unless the user chose, a bot in its own workspace does and one
/// working in a project folder doesn't.
pub fn shared(cfg: &BotConfig) -> bool {
    cfg.shared_memory.unwrap_or_else(|| crate::agent::workspace::is_managed(cfg))
}

/// The memory section of a bot's instructions: each `MEMORY.md` it sees, then the index of its notes.
pub fn opening(store: &Store, cfg: &BotConfig) -> String {
    let memory = match store.memory_read(|c| codync_mem::opening::load(c, &cfg.id, shared(cfg))) {
        Ok(opening) => opening.render(|bot| files::folder(bot).display().to_string()),
        Err(error) => {
            tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't read memory");
            String::new()
        }
    };
    format!("{memory}\n\n{}", files::index(store, cfg))
}

/// After memory changed in `bot`'s own layer, or in the shared one (`None`): the apps fetch it again, and every
/// session of a bot that sees the layer hears it on its next message (see [`change_notice`]). The opening itself
/// stays as it is, so the prompt cache does too.
pub fn changed(hub: &Hub, bot: Option<&str>) {
    let bots: Vec<(String, bool)> = match bot {
        Some(bot) => vec![(bot.to_owned(), true)],
        None => hub
            .store
            .bots()
            .map(|rows| {
                rows.into_iter()
                    .filter(|r| !r.deleted && !r.config.is_group())
                    .map(|r| {
                        let sees = shared(&r.config);
                        (r.config.id, sees)
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let revision = hub.store.memory_rev().to_string();
    for (id, sees) in &bots {
        if *sees && let Err(error) = hub.store.kv_set(&revision_key(id), &revision) {
            tracing::warn!(bot = %id, error = format!("{error:#}"), "couldn't flag the memory change");
        }
        hub.emit_bot(id);
    }
}

/// What a bot's memory screen shows changed outside the store (Claude Code's notes came or went): moves
/// `memoryRev` so the apps fetch it again, without telling the bot's sessions.
pub fn screen_changed(hub: &Hub, bot: &str) {
    if let Err(error) = hub.store.memory_write(|_, _, _| Ok(())) {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't move the memory revision");
    }
    hub.emit_bot(bot);
}

/// The memory revision a bot's layers are at.
fn revision_key(bot: &str) -> String {
    format!("memory.revision.{bot}")
}

/// The revision a session has heard: its opening, then each notice since.
fn announced_key(bot: &str, session: &str) -> String {
    format!("memory.announced.{bot}.{session}")
}

/// The revision a session's opening holds.
fn opening_key(bot: &str, session: &str) -> String {
    format!("memory.opening.{bot}.{session}")
}

/// The changes a session made that were put back, and why: its next message hears them.
fn put_back_key(bot: &str, session: &str) -> String {
    format!("memory.putback.{bot}.{session}")
}

/// A bot's change to a memory file, made in `session`, was put back (it broke a rule, and no tool call was there to
/// refuse it): that session hears why on its next message, so it can do it right. A routine run's session, never
/// resumed once the run ends, hears nothing; another session didn't make the change, so it isn't told.
pub fn put_back(store: &Store, bot: &str, session: &str, file: &str, why: &str) {
    let key = put_back_key(bot, session);
    let mut notes = put_backs(store, &key);
    notes.push(format!("Your change to {file} was put back: {why}"));
    let value = serde_json::to_string(&notes).expect("notes are plain strings");
    if let Err(error) = store.kv_set(&key, &value) {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't keep a put-back note for the bot");
    }
}

fn put_backs(store: &Store, key: &str) -> Vec<String> {
    store.kv_get(key).and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// Before the session's next message: its changes that were put back, and, when memory changed since the session
/// last heard, the whole memory as it is now. The revision is the one to acknowledge once delivered.
pub fn change_notice(store: &Store, cfg: &BotConfig, session: &str) -> Option<(String, String)> {
    let revision = current_revision(store, &cfg.id);
    let changed =
        !revision.is_empty() && store.kv_get(&announced_key(&cfg.id, session)).unwrap_or_default() != revision;
    let key = put_back_key(&cfg.id, session);
    let put_back = put_backs(store, &key);
    if !changed && put_back.is_empty() {
        return None;
    }
    if !put_back.is_empty() {
        let _ = store.kv_delete(&key);
    }
    let mut parts = put_back;
    if changed {
        parts.push(format!(
            "Your memory changed. This is all of it now and replaces any earlier memory in this conversation.\n{}",
            opening(store, cfg)
        ));
    }
    Some((format!("<memory_update>\n{}\n</memory_update>", parts.join("\n\n")), revision))
}

/// The session heard memory up to `revision`, from a notice.
pub fn mark_announced(store: &Store, bot: &str, session: &str, revision: &str) -> anyhow::Result<()> {
    store.kv_set(&announced_key(bot, session), revision)
}

/// The session's opening now holds memory at `revision` (a new session, or instructions it got again).
pub fn opened(store: &Store, bot: &str, session: &str, revision: &str) -> anyhow::Result<()> {
    store.kv_set(&opening_key(bot, session), revision)?;
    mark_announced(store, bot, session, revision)
}

/// The agent summarized its context, so the notices it got since its opening may be gone: it is left knowing what
/// its opening holds, and hears anything newer again.
pub fn compacted(store: &Store, bot: &str, session: &str) -> anyhow::Result<()> {
    match store.kv_get(&opening_key(bot, session)) {
        Some(opening) => mark_announced(store, bot, session, &opening),
        None => Ok(()),
    }
}

/// A bot wrote memory itself in `session`, which therefore knows the change: when it had heard everything up to
/// `before`, it has heard this too.
pub fn heard_own_write(store: &Store, bot: &str, session: &str, before: &str) -> anyhow::Result<()> {
    if store.kv_get(&announced_key(bot, session)).unwrap_or_default() == before {
        mark_announced(store, bot, session, &current_revision(store, bot))
    } else {
        Ok(())
    }
}

/// The memory revision a bot's layers are at; an opening rendered now holds it.
pub fn current_revision(store: &Store, bot: &str) -> String {
    store.kv_get(&revision_key(bot)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_hears_each_change_once_and_again_after_a_compaction() {
        let store =
            Store::open(&std::env::temp_dir().join(format!("codync-memory-{}.db", uuid::Uuid::new_v4()))).unwrap();
        let cfg: BotConfig = serde_json::from_value(serde_json::json!({
            "id": "5f0c7a52-0000-4000-8000-000000000002", "name": "Fixer", "backend": "claude", "cwd": "/tmp",
        }))
        .unwrap();
        let changes = |revision: &str| store.kv_set(&revision_key(&cfg.id), revision).unwrap();
        opened(&store, &cfg.id, "s1", "").unwrap();
        assert!(change_notice(&store, &cfg, "s1").is_none());

        changes("7");
        let (notice, revision) = change_notice(&store, &cfg, "s1").expect("a change");
        assert!(notice.starts_with("<memory_update>"), "{notice}");
        mark_announced(&store, &cfg.id, "s1", &revision).unwrap();
        assert!(change_notice(&store, &cfg, "s1").is_none(), "told once");
        compacted(&store, &cfg.id, "s1").unwrap();
        assert!(change_notice(&store, &cfg, "s1").is_some(), "the summary may have dropped the notice");

        // What the session wrote itself, with nothing else new, needs no notice; unless it missed something before.
        opened(&store, &cfg.id, "s1", "7").unwrap();
        changes("8");
        heard_own_write(&store, &cfg.id, "s1", "7").unwrap();
        assert!(change_notice(&store, &cfg, "s1").is_none());
        changes("9");
        changes("10");
        heard_own_write(&store, &cfg.id, "s1", "9").unwrap();
        assert!(change_notice(&store, &cfg, "s1").is_some());
    }

    #[test]
    fn a_put_back_is_told_to_the_session_that_made_the_change() {
        let store =
            Store::open(&std::env::temp_dir().join(format!("codync-memory-{}.db", uuid::Uuid::new_v4()))).unwrap();
        let cfg: BotConfig = serde_json::from_value(serde_json::json!({
            "id": "5f0c7a52-0000-4000-8000-000000000003", "name": "Fixer", "backend": "claude", "cwd": "/tmp",
        }))
        .unwrap();
        put_back(&store, &cfg.id, "routine", "MEMORY.md", "only the user may take a line out");
        assert!(change_notice(&store, &cfg, "chat").is_none(), "another session didn't make it");
        let (notice, _) = change_notice(&store, &cfg, "routine").expect("the routine's session hears it");
        assert!(notice.contains("Your change to MEMORY.md was put back"), "{notice}");
        assert!(change_notice(&store, &cfg, "routine").is_none(), "told once");
    }
}
