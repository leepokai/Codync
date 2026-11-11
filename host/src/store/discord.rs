use super::{Store, now_ms};
use crate::LockExt;
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

pub(super) fn open(db: &rusqlite::Connection) -> Result<()> {
    db.execute_batch(include_str!("discord.sql"))?;
    let audio: bool =
        db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('discord_inbox') WHERE name='audio')", [], |r| {
            r.get(0)
        })?;
    if !audio {
        db.execute("ALTER TABLE discord_inbox ADD COLUMN audio TEXT NOT NULL DEFAULT 'null'", [])?;
    }
    Ok(())
}

pub struct DiscordIncoming {
    pub event: String,
    pub generation: String,
    pub bot: String,
    pub channel: String,
    pub entry: String,
    pub text: String,
    pub user: String,
    pub message: String,
    pub(crate) audio: Option<crate::channels::voice_notes::Audio>,
}
pub struct DiscordOutgoing {
    pub entry: String,
    pub channel: String,
    pub reply_to: String,
    pub text: String,
    pub bot: String,
    pub files: Vec<serde_json::Value>,
}
#[derive(Serialize)]
pub struct DiscordDelivery {
    pub id: String,
    pub state: String,
}

impl Store {
    pub fn resume_discord_sign_in(&self, entry: &str) -> Result<bool> {
        Ok(self.db.locked().execute(
            "UPDATE discord_inbox SET state='queued' WHERE entry_id=?1 AND state='started'
            AND EXISTS(SELECT 1 FROM entries e WHERE e.id=?1 AND json_extract(e.data,'$.source')='discord'
                AND json_extract(e.data,'$.turnResult.state')='waitingForSignIn')",
            [entry],
        )? == 1)
    }
    pub fn reject_stale_discord(&self, generation: Option<&str>) -> Result<Vec<String>> {
        let mut db = self.db.locked();
        let tx = db.transaction()?;
        let ids = {
            let mut q = tx.prepare(
                "SELECT entry_id FROM discord_inbox WHERE state='queued' AND (?1 IS NULL OR generation<>?1)",
            )?;
            q.query_map([generation], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?
        };
        tx.execute(
            "UPDATE discord_inbox SET state='rejected' WHERE state='queued' AND (?1 IS NULL OR generation<>?1)",
            [generation],
        )?;
        tx.commit()?;
        Ok(ids)
    }
    pub fn enqueue_discord(&self, event: &DiscordIncoming) -> Result<()> {
        self.db.locked().execute(
            "INSERT INTO discord_inbox(event_id,generation,bot_id,channel_id,entry_id,text,user_id,created_at,audio)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT DO NOTHING",
            params![
                event.event,
                event.generation,
                event.bot,
                event.channel,
                event.entry,
                event.text,
                event.user,
                now_ms(),
                serde_json::to_string(&event.audio)?
            ],
        )?;
        Ok(())
    }
    pub fn pending_discord(&self) -> Result<Vec<DiscordIncoming>> {
        let db = self.db.locked();
        let mut q = db.prepare(
            "SELECT event_id,generation,bot_id,channel_id,entry_id,text,user_id,audio
            FROM discord_inbox WHERE state='queued' ORDER BY created_at LIMIT 20",
        )?;
        Ok(q.query_map([], |r| {
            Ok(DiscordIncoming {
                event: r.get(0)?,
                generation: r.get(1)?,
                bot: r.get(2)?,
                channel: r.get(3)?,
                entry: r.get(4)?,
                text: r.get(5)?,
                user: r.get(6)?,
                message: String::new(),
                audio: serde_json::from_str(&r.get::<_, String>(7)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
                })?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn discord_thread_exists(&self, generation: &str, channel: &str) -> Result<bool> {
        Ok(self.db.locked().query_row(
            "SELECT EXISTS(SELECT 1 FROM discord_threads WHERE generation=?1 AND channel_id=?2)",
            params![generation, channel],
            |r| r.get(0),
        )?)
    }
    pub fn map_discord_thread(&self, event: &DiscordIncoming, root: &str) -> Result<()> {
        self.db.locked().execute(
            "INSERT INTO discord_threads VALUES(?1,?2,?3,?4,?5)
            ON CONFLICT(generation,channel_id) DO UPDATE SET message_id=excluded.message_id",
            params![event.generation, event.channel, event.bot, root, event.message],
        )?;
        Ok(())
    }
    pub fn claim_discord_prompt(&self, entry: &str, generation: &str) -> Result<bool> {
        Ok(self.db.locked().execute(
            "UPDATE discord_inbox SET state='started' WHERE entry_id=?1 AND generation=?2 AND state='queued'",
            params![entry, generation],
        )? == 1)
    }
    pub fn reject_discord(&self, event: &str) -> Result<()> {
        self.db
            .locked()
            .execute("UPDATE discord_inbox SET state='rejected' WHERE event_id=?1 AND state='queued'", [event])?;
        Ok(())
    }
    pub fn pending_discord_replies(&self, generation: &str) -> Result<Vec<DiscordOutgoing>> {
        let db = self.db.locked();
        let mut q = db.prepare(
            "SELECT e.id,t.channel_id,t.message_id,json_extract(e.data,'$.text'),t.bot_id,COALESCE(json_extract(e.data,'$.files'),'[]')
            FROM entries e JOIN discord_threads t ON t.bot_id=e.bot_id AND t.root_id=e.thread_id
            WHERE t.generation=?1 AND e.kind='agent' AND json_extract(e.data,'$.final')=1
            AND (COALESCE(json_extract(e.data,'$.text'),'')<>'' OR json_extract(e.data,'$.files') IS NOT NULL)
            AND NOT EXISTS(SELECT 1 FROM discord_outbox o WHERE o.entry_id=e.id) ORDER BY e.seq LIMIT 20",
        )?;
        Ok(q.query_map([generation], |r| {
            Ok(DiscordOutgoing {
                entry: r.get(0)?,
                channel: r.get(1)?,
                reply_to: r.get(2)?,
                text: r.get(3)?,
                bot: r.get(4)?,
                files: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn claim_discord_reply(&self, entry: &str) -> Result<bool> {
        Ok(self.db.locked().execute(
            "INSERT INTO discord_outbox VALUES(?1,'sending',?2) ON CONFLICT DO NOTHING",
            params![entry, now_ms()],
        )? == 1)
    }
    pub fn finish_discord_reply(&self, entry: &str, state: &str) -> Result<()> {
        self.db.locked().execute(
            "UPDATE discord_outbox SET state=?2,updated_at=?3 WHERE entry_id=?1 AND state='sending'",
            params![entry, state, now_ms()],
        )?;
        Ok(())
    }
    pub fn discord_deliveries(&self) -> Result<Vec<DiscordDelivery>> {
        let db = self.db.locked();
        let mut q = db.prepare(
            "SELECT entry_id,CASE WHEN state='sending' THEN 'unknown' ELSE state END AS state
            FROM discord_outbox ORDER BY updated_at DESC LIMIT 50",
        )?;
        Ok(q.query_map([], |r| Ok(DiscordDelivery { id: r.get(0)?, state: r.get(1)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod audio_migration_tests {
    #[test]
    fn old_pending_messages_survive_repeatable_audio_migration() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("discord.sql")).unwrap();
        db.execute("INSERT INTO discord_inbox(event_id,generation,bot_id,channel_id,entry_id,text,user_id,created_at) VALUES('event','g','bot','channel','entry','original text','user',1)", []).unwrap();
        super::open(&db).unwrap();
        super::open(&db).unwrap();
        let row: (String, String, String) = db
            .query_row("SELECT text,state,audio FROM discord_inbox", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(row, ("original text".into(), "queued".into(), "null".into()));
    }
}
