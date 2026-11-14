//! Roster sections: a name the user files bots and group chats under (`BotConfig::section`), at
//! most one per row. A section exists while a bot is in it.

use super::Hub;
use crate::LockExt;
use anyhow::{Result, bail};

/// The longest section name, in characters.
const MAX_SECTION_CHARS: usize = 40;

/// A section name as bots keep it: trimmed, `None` when nothing is left. Refused when too long or
/// with a control character.
pub(super) fn normalize_section(raw: Option<&str>) -> Result<Option<String>> {
    let Some(name) = raw.map(str::trim).filter(|n| !n.is_empty()) else { return Ok(None) };
    if name.chars().count() > MAX_SECTION_CHARS {
        bail!("A section name can be at most {MAX_SECTION_CHARS} characters.");
    }
    if name.chars().any(char::is_control) {
        bail!("A section name can't contain control characters.");
    }
    Ok(Some(name.to_owned()))
}

impl Hub {
    /// Moves every bot in section `from` (trimmed) to `to`: `None` or an empty name takes them out of
    /// it, an existing name merges the two. Saved under one `emit_lock`, then each moved bot is emitted.
    pub fn rename_section(&self, from: &str, to: Option<&str>) -> Result<()> {
        let to = normalize_section(to)?;
        let from = from.trim();
        if to.as_deref() == Some(from) {
            return Ok(());
        }
        let mut moved = vec![];
        let saved = self.save_section(from, to.as_deref(), &mut moved);
        // Emitted even when a save failed: a saved row has its rev already, and a client past it must not miss it.
        for id in &moved {
            self.emit_bot(id);
        }
        saved
    }

    /// Saves the new section of each live bot in `from` under one `emit_lock`, noting every id in `moved`.
    fn save_section(&self, from: &str, to: Option<&str>, moved: &mut Vec<String>) -> Result<()> {
        let _g = self.emit_lock.locked();
        for mut row in self.store.bots()? {
            if row.deleted || row.config.section.as_deref() != Some(from) {
                continue;
            }
            row.config.section = to.map(str::to_owned);
            self.store.save_bot(&row.config)?;
            moved.push(row.config.id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Store};
    use serde_json::json;

    #[tokio::test]
    async fn renaming_moves_merges_and_removes_a_section() {
        let dir = std::env::temp_dir().join(format!("codync-sections-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "sections-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        for (id, section) in
            [("a", Some("Work")), ("b", Some("Work")), ("c", Some("Home")), ("d", None), ("e", Some("Work"))]
        {
            let cfg: BotConfig = serde_json::from_value(
                json!({"id": id, "name": id, "backend": "claude", "cwd": "/tmp", "section": section}),
            )
            .unwrap();
            hub.store.save_bot(&cfg).unwrap();
        }
        hub.store.delete_bot("e").unwrap();
        let section = |id: &str| hub.store.bot(id).unwrap().unwrap().config.section;
        let mut events = hub.events.subscribe();

        hub.rename_section(" Work ", Some("Home")).unwrap();
        assert!(["a", "b", "c"].into_iter().all(|id| section(id).as_deref() == Some("Home")), "merged");
        let emitted = std::iter::from_fn(|| events.try_recv().ok()).filter(|e| e["type"] == "bot").count();
        assert_eq!(emitted, 2, "each moved bot is emitted once, a deleted one is skipped");

        let long = hub.rename_section("Home", Some("x".repeat(41).as_str())).unwrap_err();
        assert_eq!(long.to_string(), "A section name can be at most 40 characters.");
        assert_eq!(section("a").as_deref(), Some("Home"), "a refused name moves nothing");

        hub.rename_section("Home", Some("  Play ")).unwrap();
        assert_eq!(section("c").as_deref(), Some("Play"), "trimmed");
        hub.rename_section("Play", None).unwrap();
        assert!(["a", "b", "c", "d"].into_iter().all(|id| section(id).is_none()), "removed; its bots stay");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
