//! Small, durable MRU list, shared by all clients of this computer.
use super::{Store, now_ms};
use crate::LockExt;
use anyhow::Result;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentWorkspace {
    pub path: String,
    pub used_at: i64,
}

impl Store {
    pub fn recent_workspaces(&self) -> Result<Vec<RecentWorkspace>> {
        self.kv_read("recent_workspaces")?.map_or_else(|| Ok(Vec::new()), |v| Ok(serde_json::from_str(&v)?))
    }

    pub fn remember_workspace(&self, path: &str) -> Result<()> {
        let db = self.db.locked();
        let existing: Option<String> =
            db.query_row("SELECT v FROM kv WHERE k = 'recent_workspaces'", [], |r| r.get(0)).optional()?;
        let mut recent: Vec<RecentWorkspace> = existing.map_or_else(|| Ok(Vec::new()), |v| serde_json::from_str(&v))?;
        recent.retain(|item| item.path != path);
        recent.insert(0, RecentWorkspace { path: path.into(), used_at: now_ms() });
        recent.truncate(12);
        db.execute(
            "INSERT INTO kv(k,v) VALUES('recent_workspaces',?1) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
            [serde_json::to_string(&recent)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn recent_workspaces_deduplicate_and_keep_latest_twelve() {
        let store = super::super::tests::temp_store();
        for i in 0..15 {
            store.remember_workspace(&format!("/project/{i}")).unwrap();
        }
        store.remember_workspace("/project/5").unwrap();
        let recent = store.recent_workspaces().unwrap();
        assert_eq!(recent.len(), 12);
        assert_eq!(recent[0].path, "/project/5");
        assert_eq!(recent[1].path, "/project/14");
        assert_eq!(recent.iter().filter(|r| r.path == "/project/5").count(), 1);
    }
}
