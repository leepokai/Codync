//! The current main session's last reported context use, shared by every client. This is the agent's
//! context measurement, not cumulative billing usage or the separate realtime voice session.

use crate::hub::Hub;
use crate::store::{BotConfig, BotRow, Store};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub max_tokens: u64,
    pub percent: u8,
    pub level: Level,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Normal,
    Warning,
    Danger,
}

impl ContextUsage {
    fn new(used: u64, size: u64) -> Option<Self> {
        if size == 0 {
            return None;
        }
        // Wide arithmetic also handles malformed, very large adapter measurements safely. Retain the
        // reported counts in the detail while the ring itself stops at 100%.
        let percent = u8::try_from((u128::from(used) * 100 / u128::from(size)).min(100)).ok()?;
        let level = match percent {
            95.. => Level::Danger,
            80.. => Level::Warning,
            _ => Level::Normal,
        };
        Some(Self { used_tokens: used, max_tokens: size, percent, level })
    }
}

fn key(bot: &str, backend: &str, session: &str) -> String {
    format!("context.usage.{bot}.{backend}.{session}")
}

pub fn for_bot(store: &Store, row: &BotRow) -> Option<ContextUsage> {
    if row.config.is_group() {
        return None;
    }
    let backend = store.kv_get(&format!("fallback.on.{}", row.config.id));
    let backend = backend.as_deref().unwrap_or(&row.config.backend);
    let value = store.kv_get(&key(&row.config.id, backend, row.session_id.as_deref()?))?;
    let usage: ContextUsage = serde_json::from_str(&value).ok()?;
    // Derive presentation from the recorded counts, including when reading older persisted data.
    ContextUsage::new(usage.used_tokens, usage.max_tokens)
}

/// A successful session/new is fresh even if the agent reuses a process-local session ID.
pub fn clear(store: &Store, cfg: &BotConfig, session: &str) -> anyhow::Result<()> {
    store.kv_delete(&key(&cfg.id, &cfg.backend, session))
}

pub fn record(hub: &Hub, cfg: &BotConfig, session: &str, used: u64, size: u64) {
    let bot = &cfg.id;
    let run = || -> anyhow::Result<()> {
        let Some(row) = hub.store.bot(bot)? else { return Ok(()) };
        // Routine sessions must never replace the main chat's reading.
        if row.config.is_group() || row.session_id.as_deref() != Some(session) {
            return Ok(());
        }
        let key = key(bot, &cfg.backend, session);
        let before = hub.store.kv_get(&key);
        let after = ContextUsage::new(used, size).map(|u| serde_json::to_string(&u)).transpose()?;
        if before == after {
            return Ok(());
        }
        match after {
            Some(value) => hub.store.kv_set(&key, &value)?,
            None => hub.store.kv_delete(&key)?,
        }
        hub.emit_bot(bot);
        Ok(())
    };
    if let Err(error) = run() {
        tracing::warn!(bot = %bot, error = format!("{error:#}"), "couldn't record context usage");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parked_agents_with_the_same_session_id_keep_separate_readings() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let cfg: BotConfig = serde_json::from_value(serde_json::json!({
            "id": "b", "name": "Bot", "backend": "claude"
        }))
        .unwrap();
        let row = BotRow { config: cfg.clone(), session_id: Some("s1".into()), rev: 0, deleted: false, read_rev: 0 };
        for (backend, used) in [("claude", 50_000), ("codex", 100_000)] {
            let value = serde_json::to_string(&ContextUsage::new(used, 200_000).unwrap()).unwrap();
            store.kv_set(&key("b", backend, "s1"), &value).unwrap();
        }
        assert_eq!(for_bot(&store, &row).unwrap().percent, 25);
        store.kv_set("fallback.on.b", "codex").unwrap();
        assert_eq!(for_bot(&store, &row).unwrap().percent, 50);
        let fallback = BotConfig { backend: "codex".into(), ..cfg };
        clear(&store, &fallback, "s1").unwrap();
        assert!(for_bot(&store, &row).is_none());
        store.kv_delete("fallback.on.b").unwrap();
        assert_eq!(for_bot(&store, &row).unwrap().percent, 25);
    }

    #[test]
    fn unavailable_capacity_is_unknown_and_real_zero_is_reported() {
        assert_eq!(ContextUsage::new(0, 0), None);
        assert_eq!(ContextUsage::new(50, 0), None);
        let zero = ContextUsage::new(0, 200_000).unwrap();
        assert_eq!(zero.percent, 0);
        assert_eq!(zero.level, Level::Normal);
    }

    #[test]
    fn thresholds_and_overflow_keep_the_reported_counts() {
        for (used, percent, level) in [
            (159_999, 79, Level::Normal),
            (160_000, 80, Level::Warning),
            (190_000, 95, Level::Danger),
            (210_000, 100, Level::Danger),
        ] {
            let usage = ContextUsage::new(used, 200_000).unwrap();
            assert_eq!((usage.used_tokens, usage.percent, usage.level), (used, percent, level));
        }
        assert_eq!(ContextUsage::new(u64::MAX, 1).unwrap().percent, 100);
        assert_eq!(ContextUsage::new(u64::MAX, u64::MAX).unwrap().percent, 100);
    }
}
