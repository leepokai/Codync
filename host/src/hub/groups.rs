//! Group chats as the hub keeps them: what a busy one shows, and the groups a deleted bot leaves.

use super::{BotStatus, Hub, Runtime};
use crate::LockExt;
use crate::store::BotConfig;
use anyhow::Result;

impl Hub {
    /// A group is busy while one of its members works in it: that member's status and activity.
    pub fn group_runtime(&self, group: &BotConfig) -> Runtime {
        let busy = {
            let runtime = self.runtime.locked();
            group.members.iter().find_map(|m| {
                let r = runtime.get(m)?;
                (r.status != BotStatus::Idle && r.lane.as_ref().is_some_and(|l| l.chat == group.id))
                    .then(|| (m.clone(), r.clone()))
            })
        };
        let Some((member, mut r)) = busy else { return Runtime::default() };
        let name = self.bot_name(&member);
        r.activity = if r.activity.is_empty() { name } else { format!("{name}: {}", r.activity) };
        r
    }

    /// Takes bot `id` out of every group it was in; what it said there stays. Each group is read again
    /// under the lock, so an edit or a section rename since this scan isn't written back.
    pub(super) fn leave_groups(&self, id: &str) -> Result<()> {
        let groups: Vec<String> =
            self.store.bots()?.into_iter().filter(|r| !r.deleted && r.config.is_group()).map(|r| r.config.id).collect();
        for group in groups {
            let left = {
                let _g = self.emit_lock.locked();
                match self.store.bot(&group)?.filter(|r| !r.deleted) {
                    Some(mut row) if row.config.members.iter().any(|m| m == id) => {
                        row.config.members.retain(|m| m != id);
                        self.store.save_bot(&row.config)?;
                        true
                    }
                    _ => false,
                }
            };
            if left {
                self.emit_bot(&group);
            }
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
    async fn a_deleted_bot_leaves_its_groups_and_they_keep_their_latest_section() {
        let dir = std::env::temp_dir().join(format!("codync-groups-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let hub = Hub::new(store, "groups-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        for v in [
            json!({"id": "a", "name": "a", "backend": "claude", "cwd": "/tmp"}),
            json!({"id": "b", "name": "b", "backend": "claude", "cwd": "/tmp"}),
            json!({"id": "g", "name": "g", "kind": "group", "members": ["a", "b"], "section": "Work"}),
        ] {
            hub.store.save_bot(&serde_json::from_value::<BotConfig>(v).unwrap()).unwrap();
        }
        hub.rename_section("Work", Some("Play")).unwrap();
        hub.delete_bot("a").unwrap();
        let group = hub.store.bot("g").unwrap().unwrap().config;
        assert_eq!((group.members, group.section.as_deref()), (vec!["b".to_owned()], Some("Play")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
