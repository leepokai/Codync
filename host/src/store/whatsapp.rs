use super::{Store, now_ms};
use crate::LockExt;
use anyhow::Result;
use rusqlite::params;

pub(super) fn open(db: &rusqlite::Connection) -> Result<()> {
    db.execute_batch(include_str!("whatsapp.sql"))?;
    Ok(())
}

pub struct WhatsAppOutgoing {
    pub entry: String,
    pub chat: String,
    pub text: String,
    /// The files the reply carries: path, name and type, as the entry stores them.
    pub files: Vec<serde_json::Value>,
}

impl Store {
    /// The root entry of a `WhatsApp` conversation, if it has one.
    pub fn whatsapp_root(&self, chat: &str) -> Option<String> {
        self.db.locked().query_row("SELECT root_id FROM whatsapp_threads WHERE chat=?1", [chat], |r| r.get(0)).ok()
    }
    pub fn map_whatsapp_thread(&self, chat: &str, root: &str) -> Result<()> {
        self.db
            .locked()
            .execute("INSERT INTO whatsapp_threads VALUES(?1,?2) ON CONFLICT(chat) DO NOTHING", params![chat, root])?;
        Ok(())
    }
    /// Agent replies that are final and not in the outbox yet, with their conversation.
    pub fn pending_whatsapp_replies(&self) -> Result<Vec<WhatsAppOutgoing>> {
        let db = self.db.locked();
        let mut q = db.prepare(
            "SELECT e.id,t.chat,COALESCE(json_extract(e.data,'$.text'),''),COALESCE(json_extract(e.data,'$.files'),'[]')
            FROM entries e JOIN whatsapp_threads t ON t.root_id=e.thread_id
            WHERE e.kind='agent' AND json_extract(e.data,'$.final')=1
            AND (COALESCE(json_extract(e.data,'$.text'),'')<>'' OR json_extract(e.data,'$.files') IS NOT NULL)
            AND NOT EXISTS(SELECT 1 FROM whatsapp_outbox o WHERE o.entry_id=e.id) ORDER BY e.seq LIMIT 20",
        )?;
        Ok(q.query_map([], |r| {
            let files: String = r.get(3)?;
            Ok(WhatsAppOutgoing {
                entry: r.get(0)?,
                chat: r.get(1)?,
                text: r.get(2)?,
                files: serde_json::from_str(&files).unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn claim_whatsapp_reply(&self, entry: &str) -> Result<bool> {
        Ok(self.db.locked().execute(
            "INSERT INTO whatsapp_outbox VALUES(?1,'sending',?2) ON CONFLICT DO NOTHING",
            params![entry, now_ms()],
        )? == 1)
    }
    pub fn finish_whatsapp_reply(&self, entry: &str, state: &str) -> Result<()> {
        self.db.locked().execute(
            "UPDATE whatsapp_outbox SET state=?2,updated_at=?3 WHERE entry_id=?1 AND state='sending'",
            params![entry, state, now_ms()],
        )?;
        Ok(())
    }
}
