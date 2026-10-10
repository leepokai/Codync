//! The roster's manual order: where the user dragged each bot.

use super::Hub;
use crate::LockExt;
use anyhow::Result;

impl Hub {
    /// Puts `ids` (the roster as shown, top first) in that order. Unknown and deleted ids are
    /// skipped; bots left out (hidden ones) keep their place.
    pub fn reorder_bots(&self, ids: &[String]) -> Result<()> {
        for (position, id) in (0_i64..).zip(ids) {
            let Some(mut row) = self.store.bot(id)?.filter(|r| !r.deleted) else { continue };
            if row.config.position == position {
                continue;
            }
            row.config.position = position;
            {
                let _g = self.emit_lock.locked();
                self.store.save_bot(&row.config)?;
            }
            self.emit_bot(id);
        }
        Ok(())
    }

    /// A new bot goes on top once the user has arranged the roster; until then it stays at 0,
    /// so recent activity keeps ordering everything.
    pub(super) fn new_bot_position(&self) -> Result<i64> {
        let positions: Vec<i64> =
            self.store.bots()?.into_iter().filter(|b| !b.deleted).map(|b| b.config.position).collect();
        let arranged = positions.iter().any(|&p| p != 0);
        Ok(if arranged { positions.iter().min().copied().unwrap_or(0) - 1 } else { 0 })
    }
}

#[cfg(test)]
mod tests {
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::{BotConfig, Store};

    #[tokio::test]
    async fn dragged_order_sticks_and_new_bots_go_on_top() {
        let dir = std::env::temp_dir().join(format!("codync-order-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "order-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        for id in ["a", "b", "c"] {
            let cfg: BotConfig =
                serde_json::from_value(serde_json::json!({"id": id, "name": id, "backend": "claude", "cwd": "/tmp"}))
                    .unwrap();
            hub.store.save_bot(&cfg).unwrap();
        }
        assert_eq!(hub.new_bot_position().unwrap(), 0, "unarranged: recent activity orders new bots too");

        hub.reorder_bots(&["c".into(), "a".into(), "gone".into(), "b".into()]).unwrap();
        let position = |id: &str| hub.store.bot(id).unwrap().unwrap().config.position;
        assert!(position("c") < position("a") && position("a") < position("b"));
        assert_eq!(hub.new_bot_position().unwrap(), -1, "arranged: a new bot goes on top");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
