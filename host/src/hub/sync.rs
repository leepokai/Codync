//! Bot snapshots shared by full, incremental and live synchronization.

use super::Hub;
use crate::LockExt;
use crate::store::BotRow;
use anyhow::Result;
use serde_json::{Value, json};

impl Hub {
    pub fn bot_json(&self, row: &BotRow) -> Value {
        let cfg = &row.config;
        let mut v = serde_json::to_value(cfg).expect("BotConfig is plain data and always serializes");
        v["managedWorkspace"] = crate::agent::workspace::is_managed(cfg).into();
        // Null where there is no switch to show.
        v["harnessMemory"] =
            (!cfg.is_group() && crate::agent::own_memory::switchable(cfg)).then_some(cfg.harness_memory).into();
        if !cfg.is_group() {
            v["harnessMemoryShared"] = crate::agent::own_memory::shared(cfg).into();
            v["sharedMemory"] = crate::chat::memory::shared(cfg).into();
            v["memoryRev"] = self.store.memory_rev().into();
        }
        let rt = if cfg.is_group() {
            // A group has no agent: Fast mode belongs to its members.
            crate::hub::Runtime {
                fast_mode: crate::agent::config::FastModeSupport::default(),
                ..self.group_runtime(cfg)
            }
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
        v["fastModeSupport"] = rt.fast_mode.as_str().into();
        if !cfg.is_group() {
            v["agentStep"] = json!(rt.agent_step);
            v["resting"] = json!(crate::agent::fallback::resting(&self.store, cfg, crate::store::now_ms()));
        }
        v["fastModeIssue"] = rt.fast_mode.issue().into();
        v["unread"] = self.store.unread(&cfg.id, row.read_rev).into();
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
