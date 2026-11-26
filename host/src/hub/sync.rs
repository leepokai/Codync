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
        // `worktreeMode` as the bool older apps read and send back.
        v["threadWorktrees"] = cfg.worktree_mode.as_bool().into();
        // Null where there is no switch to show.
        v["harnessMemory"] =
            (!cfg.is_group() && crate::agent::own_memory::switchable(cfg)).then_some(cfg.harness_memory).into();
        // Null where Codync doesn't summarize the chat. The sizes it may take: none until the agent tells its window.
        let compacts = crate::agent::bot::offers_threshold(&self.store, cfg);
        v["compaction"] = if compacts { v["compaction"].take() } else { Value::Null };
        let window = compacts.then(|| crate::agent::bot::context_window(&self.store, &cfg.id)).flatten();
        v["compactionTokens"] = json!(window.and_then(codync_mem::compaction::sizes));
        if !cfg.is_group() {
            v["harnessMemoryShared"] = crate::agent::own_memory::shared(cfg).into();
            v["sharedMemory"] = crate::chat::memory::shared(cfg).into();
            v["memoryRev"] = self.store.memory_rev().into();
            v["devServers"] = self.dev_servers.wire(&cfg.id);
            // What the bot works on away from the chat, and what waits for it.
            v["work"] = crate::chat::work::wire(self, cfg);
            // Which of its projects are in a git repository (`projectGitRepos`, in `projects`' order): where the
            // worktree mode matters, and a client shows it. `gitRepo`, for older apps: its default project is one
            // (the bot's own folder never gets worktrees).
            let repos: Vec<bool> = cfg.projects.iter().map(|p| crate::chat::work::in_repo(p)).collect();
            v["gitRepo"] = repos.first().copied().unwrap_or_default().into();
            v["projectGitRepos"] = repos.into();
            // Whether the host can run git at all: without it a repository gets no worktrees either.
            v["gitInstalled"] = crate::git::installed().into();
        }
        let rt = if cfg.is_group() {
            // A group has no agent: Fast mode belongs to its members.
            crate::hub::Runtime {
                fast_mode: codync_agent::config::FastModeSupport::default(),
                ..self.group_runtime(cfg)
            }
        } else {
            self.runtime.locked().get(&cfg.id).cloned().unwrap_or_default()
        };
        let last = self.store.last_message(&cfg.id);
        v["rev"] = row.rev.into();
        v["deleted"] = row.deleted.into();
        v["status"] = json!(rt.status);
        v["activity"] = rt.activity.clone().into();
        v["startedAt"] = rt.started_at.into();
        v["workingChat"] = rt.lane.as_ref().map(|l| l.chat.clone()).into();
        v["fastModeSupport"] = rt.fast_mode.as_str().into();
        if !cfg.is_group() {
            // The commands of the agent the bot runs on now: a fallback's while it answers.
            let agent = rt.agent_step.as_ref().map_or(cfg.backend.as_str(), |s| s.agent.as_str());
            v["agentCommands"] = json!(crate::agent::bot::agent_commands(&self.store, &cfg.id, agent, &cfg.skills));
            // How it stands to what shows it outside its chat (a Live Activity, an ongoing notification, a tray):
            // the chat's status with its work in the background, as the pushes say it.
            let (status, started_at) = crate::chat::work::outside(self, cfg, &rt);
            v["outside"] = json!({"status": status, "startedAt": started_at});
            v["agentStep"] = json!(rt.agent_step);
            v["resting"] = json!(crate::agent::fallback::resting(&self.store, cfg, crate::store::now_ms()));
        }
        v["fastModeIssue"] = rt.fast_mode.issue().into();
        let badge = self.store.badge(&cfg.id, row.read_rev);
        v["unread"] = badge.unread.into();
        // How many of those are cards waiting on the user: the badge turns amber.
        v["waiting"] = badge.waiting.into();
        let preview = last.as_ref().map(|l| match &l.author {
            // A group names who spoke.
            Some(author) if cfg.is_group() => format!("{}: {}", self.bot_name(author), l.text),
            Some(_) => l.text.clone(),
            None => format!("You: {}", l.text),
        });
        v["lastMessage"] = preview.map(|p| codync_agent::acp::truncate(&p, 280)).into();
        v["lastAt"] = last.map_or(cfg.created_at, |l| l.at).into();
        v
    }

    /// Full refreshes retain cached client state, so they need deletion records too.
    pub fn bots_json(&self, since: i64) -> Result<Vec<Value>> {
        Ok(self.store.bots()?.iter().filter(|b| b.rev > since).map(|b| self.bot_json(b)).collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::store::{EntryKind, Lane, Store};
    use serde_json::json;

    #[test]
    fn a_bot_offers_compaction_sizes_once_its_agent_tells_the_window() {
        let dir = std::env::temp_dir().join(format!("codync-compaction-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("t.db")).unwrap();
        for (id, backend) in [("c", "claude"), ("g", "gemini")] {
            let bot = json!({"id": id, "name": id, "backend": backend, "cwd": "/tmp", "compaction": 300_000});
            store.save_bot(&serde_json::from_value(bot).unwrap()).unwrap();
        }
        let hub = Hub::new(store, "test".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0);

        let claude = hub.bot_value_for_test("c");
        assert_eq!(claude["compaction"], 300_000);
        assert!(claude["compactionTokens"].is_null(), "no window reported yet");
        hub.store.kv_set("context.window.c", "1000000").unwrap();
        assert_eq!(
            hub.bot_value_for_test("c")["compactionTokens"],
            json!({"min": 50_000, "max": 960_000, "step": 10_000, "presets": (1..=9).map(|n| n * 100_000).collect::<Vec<u64>>()})
        );

        let gemini = hub.bot_value_for_test("g");
        assert!(gemini["compaction"].is_null() && gemini["compactionTokens"].is_null(), "Codync can't compact it");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_waiting_card_keeps_its_badge_until_answered() {
        // Issue #366.
        let dir = std::env::temp_dir().join(format!("codync-waiting-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let bot = json!({"id": "b", "name": "b", "backend": "claude", "cwd": "/tmp"});
        store.save_bot(&serde_json::from_value(bot).unwrap()).unwrap();
        let hub = Hub::new(store, "test".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0);
        let card = || hub.add_entry(&Lane::main("b"), EntryKind::Permission, 1, &json!({"status": "pending"})).unwrap();
        let (first, _second) = (card(), card());
        let mut events = hub.events.subscribe();
        let mut badges = || {
            std::iter::from_fn(|| events.try_recv().ok())
                .filter(|e| e["type"] == "bot")
                .map(|e| (e["bot"]["unread"].clone(), e["bot"]["waiting"].clone()))
                .collect::<Vec<_>>()
        };

        let rev = hub.store.bot("b").unwrap().unwrap().rev;
        hub.mark_read("b").unwrap();
        assert_eq!((badges(), hub.store.bot("b").unwrap().unwrap().rev), (vec![], rev), "only cards wait: a no-op");
        assert_eq!(hub.bot_value("b").unwrap()["unread"], 2, "both still counted");

        hub.set_entry(&first.id, &json!({"status": "answered"})).unwrap();
        assert_eq!(badges(), [(json!(1), json!(1))], "answering one card sends the bot");
        std::fs::remove_dir_all(&dir).ok();
    }
}
