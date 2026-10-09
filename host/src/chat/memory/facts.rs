//! Read-only projection of Engram's pinned schema for Codync's UI and prompt.
//! These blocking methods run on `spawn_blocking`; every write uses native MCP.

use super::{MAX_FACT_CHARS, PROFILE_PROMPT_LIMIT, engram};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub use super::dates::ymd;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Profile,
    Log,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub id: String,
    pub content: String,
    pub created_at: i64,
    pub kind: Kind,
    pub title: String,
    pub memory_type: String,
    pub scope: String,
    pub project: String,
    pub topic_key: Option<String>,
    pub session_id: String,
    pub pinned: bool,
    pub review_after: Option<String>,
    pub revision_count: i64,
    pub source: Option<String>,
}

pub struct Recall {
    pub profile: Vec<Fact>,
    pub recent: Vec<Fact>,
}

pub struct Memory {
    bot: String,
    dir: PathBuf,
    session: String,
}

impl Memory {
    pub fn for_bot(bot: &str) -> Result<Self> {
        let dir = engram::directory(bot)?;
        tokio::runtime::Handle::try_current()
            .context("memory access requires a host runtime")?
            .block_on(engram::prepare(bot))?;
        Ok(Self { bot: bot.to_owned(), dir, session: format!("{}:codync-background", engram::project(bot)) })
    }

    pub fn for_session(bot: &str, session: &str) -> Result<Self> {
        let mut memory = Self::for_bot(bot)?;
        if !session.is_empty() {
            session.clone_into(&mut memory.session);
        }
        Ok(memory)
    }

    pub fn location(&self) -> &Path {
        &self.dir
    }

    pub fn project(&self) -> String {
        engram::project(&self.bot)
    }

    fn database(&self) -> Result<Connection> {
        let db = Connection::open_with_flags(self.dir.join("engram.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(db)
    }

    pub fn relevant(&self, query: &str, limit: usize) -> Result<Vec<Fact>> {
        super::read::relevant(&self.database()?, query, limit)
    }

    pub fn browse(&self, query: &str, filter: &str, limit: usize, offset: usize) -> Result<super::read::Page> {
        super::read::browse(&mut self.database()?, query, filter, limit, offset)
    }

    pub fn find(&self, id: &str) -> Result<Fact> {
        super::read::find(&self.database()?, id.parse()?)?.context("memory not found")
    }

    pub fn recall(&self, recent_limit: usize) -> Result<Recall> {
        let db = self.database()?;
        Ok(Recall {
            profile: super::read::list(&db, "profile", PROFILE_PROMPT_LIMIT)?,
            recent: super::read::list(&db, "log", recent_limit)?,
        })
    }

    pub fn profile_facts(&self) -> Result<Vec<Fact>> {
        super::read::list(&self.database()?, "profile", usize::MAX)
    }

    pub fn add(&self, content: &str, kind: Kind, at_ms: i64) -> Result<bool> {
        let content = normalize(content);
        if content.is_empty() {
            return Ok(false);
        }
        if super::read::same_content(&self.database()?, &content)?.is_some() {
            return Ok(false);
        }
        self.tool(
            "mem_save",
            &json!({
                "title": content.chars().take(100).collect::<String>(), "content": content,
                "type": if kind == Kind::Profile { "learning" } else { "discovery" },
                "scope": if kind == Kind::Profile { "personal" } else { "project" },
                "project": engram::project(&self.bot), "capture_prompt": false,
                "session_id": self.session,
                "topic_key": format!("codync/keeper/{at_ms}/{}", super::read::content_hash(&content)),
            }),
        )?;
        Ok(true)
    }

    pub fn summary(&self, content: &str) -> Result<()> {
        self.tool(
            "mem_session_summary",
            &json!({"session_id":self.session, "project":self.project(), "content":content}),
        )?;
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<bool> {
        let Some(fact) = super::read::find(&self.database()?, id.parse()?)? else {
            return Ok(false);
        };
        self.tool("mem_delete", &json!({"id":id.parse::<i64>()?, "expected_project":fact.project}))?;
        Ok(true)
    }

    pub fn remove_by_content(&self, content: &str) -> Result<bool> {
        let Some(fact) = super::read::same_content(&self.database()?, content)? else {
            return Ok(false);
        };
        self.remove(&fact.id)
    }

    pub fn clear(&self) -> Result<()> {
        tokio::runtime::Handle::try_current()?.block_on(engram::clear(&self.bot))
    }

    /// Consolidation changes the scope of old facts; their IDs and history remain intact.
    pub fn rewrite_profile(&self, keep: &[(String, i64)], demote: &[(String, i64)]) -> Result<()> {
        for (content, _) in demote {
            if let Some(fact) = super::read::same_content(&self.database()?, content)? {
                self.tool(
                    "mem_update",
                    &json!({"id":fact.id.parse::<i64>()?, "expected_project":fact.project, "scope":"project"}),
                )?;
            }
        }
        for (content, at) in keep {
            if let Some(fact) = super::read::same_content(&self.database()?, content)? {
                if fact.scope != "personal" {
                    self.tool(
                        "mem_update",
                        &json!({"id":fact.id.parse::<i64>()?, "expected_project":fact.project, "scope":"personal"}),
                    )?;
                }
            } else {
                self.add(content, Kind::Profile, *at)?;
            }
        }
        Ok(())
    }

    pub fn tool(&self, name: &str, args: &Value) -> Result<Value> {
        let result = tokio::runtime::Handle::try_current()?.block_on(engram::call(&self.bot, name, args))?;
        if result["isError"] == true {
            bail!("Engram: {}", result["content"]);
        }
        Ok(result)
    }
}

pub(super) fn parse_timestamp(raw: &str) -> Result<i64> {
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Ok(date.timestamp_millis());
    }
    Ok(chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f")?.and_utc().timestamp_millis())
}

pub fn normalize(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_FACT_CHARS).collect()
}

pub(super) fn dedupe_key(content: &str) -> String {
    normalize(content).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_native_timestamp_formats_without_losing_date() {
        assert_eq!(parse_timestamp("2026-01-01 00:00:00").unwrap(), parse_timestamp("2026-01-01T00:00:00Z").unwrap());
        assert!(parse_timestamp("bad date").is_err());
    }
}
