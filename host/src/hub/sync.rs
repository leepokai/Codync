//! Bot snapshots shared by full, incremental and live synchronization.

use super::Hub;
use crate::LockExt;
use crate::store::{BotRow, ReadScope};
use anyhow::Result;
use serde_json::{Value, json};

impl Hub {
    pub fn bot_json(&self, row: &BotRow) -> Value {
        let cfg = &row.config;
        let mut v = serde_json::to_value(cfg).expect("BotConfig is plain data and always serializes");
        v["managedWorkspace"] = crate::agent::workspace::is_managed(cfg).into();
        let rt = if cfg.is_group() {
            self.group_runtime(cfg)
        } else {
            self.runtime.locked().get(&cfg.id).cloned().unwrap_or_default()
        };
        let last = self.store.last_message(&cfg.id);
        v["rev"] = row.rev.into();
        v["deleted"] = row.deleted.into();
        v["status"] = json!(rt.status);
        v["activity"] = rt.activity.into();
        v["startedAt"] = rt.started_at.into();
        v["workingChat"] = rt.lane.as_ref().map(|l| l.chat.clone()).into();
        v["workingThread"] = rt.lane.and_then(|l| l.thread).into();
        v["unread"] = self.store.unread(&cfg.id, row.read_rev, ReadScope::All).into();
        let preview = last.as_ref().map(|l| match &l.author {
            // A group names who spoke.
            Some(author) if cfg.is_group() => format!("{}: {}", self.bot_name(author), l.text),
            Some(_) => l.text.clone(),
            None => format!("You: {}", l.text),
        });
        v["lastMessage"] = preview.map(|p| crate::agent::acp::truncate(&p, 280)).into();
        v["lastAt"] = last.map_or(cfg.created_at, |l| l.at).into();
        v
    }

    /// Full refreshes retain cached client state, so they need deletion records too.
    pub fn bots_json(&self, since: i64) -> Result<Vec<Value>> {
        Ok(self.store.bots()?.iter().filter(|b| b.rev > since).map(|b| self.bot_json(b)).collect())
    }
}
