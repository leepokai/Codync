//! What the bot does when a turn takes a daily token budget (`budget`) to a new level: the chat says so, and at the
//! limit the routines of the bot (or of every bot, for the fleet's limit) pause until the user turns them on again.

use super::{Actor, NoticeStyle};
use crate::budget::{Crossed, FLEET, Level};

impl Actor {
    /// Counts a finished turn's `usage` and reacts to any budget it crossed.
    pub(super) fn count_spend(&self, usage: &serde_json::Value) {
        let tokens = crate::budget::spent(usage);
        for (key, crossed) in self.hub.budget.record(&self.hub.store, &self.cfg.id, crate::store::now_ms(), tokens) {
            self.budget_crossed(&key, crossed);
        }
    }

    fn budget_crossed(&self, key: &str, crossed: Crossed) {
        let whose = if key == FLEET { "the fleet's" } else { "this bot's" };
        let Crossed { level, used, limit } = crossed;
        match level {
            Level::Within => {}
            Level::Warning => self.notice(
                &format!(
                    "{used} of {whose} {limit} daily tokens used. Routines pause at the limit.{}",
                    self.saver_note()
                ),
                NoticeStyle::Info,
            ),
            Level::Exceeded => {
                let paused = self.pause_routines(key);
                self.notice(
                    &format!("{whose} daily limit of {limit} tokens is used up; {paused} routines paused. Turn them back on when you want."),
                    NoticeStyle::Error,
                );
            }
        }
    }

    /// What the saver does from here, when this bot has it on and a background model to use.
    fn saver_note(&self) -> &'static str {
        let has_model = self.cfg.background_model.as_deref().is_some_and(|m| !m.trim().is_empty());
        if self.cfg.saver && has_model {
            " Saver is on: new sessions use the background model until tomorrow."
        } else {
            ""
        }
    }

    /// Pauses the routines of the bot `key`, or of every bot for the fleet; says how many it paused.
    fn pause_routines(&self, key: &str) -> usize {
        let bots: Vec<String> = if key == FLEET {
            self.hub.store.bots().unwrap_or_default().into_iter().map(|r| r.config.id).collect()
        } else {
            vec![key.to_owned()]
        };
        bots.iter()
            .map(|bot| {
                self.hub.routines.pause_all(&self.hub.store, bot).unwrap_or_else(|error| {
                    tracing::warn!(bot = %bot, error = format!("{error:#}"), "couldn't pause routines at the token limit");
                    0
                })
            })
            .sum()
    }
}
