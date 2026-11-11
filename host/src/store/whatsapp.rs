use super::{Store, now_ms};
use crate::LockExt;
use anyhow::Result;
use rusqlite::{OptionalExtension, params};

pub(super) fn open(db: &rusqlite::Connection) -> Result<()> {
    // Older hosts keyed all bots' conversations by chat alone. Keep their root and
    // approval history, while letting a different bot have its own conversation.
    let legacy = {
        let mut columns = db.prepare("PRAGMA table_info(whatsapp_threads)")?;
        let names = columns.query_map([], |row| row.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
        !names.is_empty() && !names.iter().any(|name| name == "bot_id")
    };
    if legacy {
        let tx = db.unchecked_transaction()?;
        tx.execute_batch("ALTER TABLE whatsapp_threads RENAME TO whatsapp_threads_legacy;")?;
        tx.execute_batch(include_str!("whatsapp.sql"))?;
        tx.execute_batch(
            "INSERT INTO whatsapp_threads(chat,bot_id,root_id,sender)
             SELECT t.chat,e.bot_id,t.root_id,t.sender FROM whatsapp_threads_legacy t
             JOIN entries e ON e.id=t.root_id;
             DROP TABLE whatsapp_threads_legacy;",
        )?;
        tx.commit()?;
    }
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

#[cfg(test)]
mod tests;

impl Store {
    /// The root entry of a `WhatsApp` conversation, if it has one.
    pub fn whatsapp_root(&self, chat: &str, bot: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .locked()
            .query_row("SELECT root_id FROM whatsapp_threads WHERE chat=?1 AND bot_id=?2", params![chat, bot], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn map_whatsapp_thread(&self, chat: &str, bot: &str, root: &str, sender: &str) -> Result<()> {
        self.db.locked().execute(
            "INSERT INTO whatsapp_threads(chat,bot_id,root_id,sender) VALUES(?1,?2,?3,?4)
            ON CONFLICT(chat,bot_id) DO UPDATE SET sender=excluded.sender",
            params![chat, bot, root, sender],
        )?;
        Ok(())
    }
    /// Agent replies that are final and not in the outbox yet, with their conversation.
    pub fn pending_whatsapp_replies(&self) -> Result<Vec<WhatsAppOutgoing>> {
        let db = self.db.locked();
        let mut q = db.prepare(
            "SELECT e.id,t.chat,COALESCE(json_extract(e.data,'$.text'),''),COALESCE(json_extract(e.data,'$.files'),'[]')
            FROM entries e JOIN whatsapp_threads t ON t.root_id=e.thread_id AND t.bot_id=e.bot_id
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

    /// Approval cards still pending in `WhatsApp` chats that were not shown yet.
    pub fn pending_whatsapp_cards(&self) -> Result<Vec<(String, String)>> {
        let db = self.db.locked();
        let mut q = db.prepare(
            "SELECT t.chat,e.id FROM entries e JOIN whatsapp_threads t ON t.root_id=e.thread_id AND t.bot_id=e.bot_id
            WHERE e.kind='permission' AND json_extract(e.data,'$.status')='pending'
            AND NOT EXISTS(SELECT 1 FROM whatsapp_cards c WHERE c.chat=t.chat AND c.entry_id=e.id)
            AND NOT EXISTS(SELECT 1 FROM whatsapp_card_exclusions x WHERE x.entry_id=e.id)
            ORDER BY e.seq LIMIT 10",
        )?;
        Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn mark_whatsapp_card(&self, chat: &str, entry: &str, sender: &str) -> Result<()> {
        self.db.locked().execute(
            "INSERT INTO whatsapp_cards VALUES(?1,?2,?3,?4) ON CONFLICT DO NOTHING",
            params![chat, entry, sender, now_ms()],
        )?;
        Ok(())
    }
    /// A legacy or mixed-author decision remains in the app, without blocking later cards.
    pub fn exclude_whatsapp_card(&self, entry: &str) -> Result<()> {
        self.db.locked().execute("INSERT INTO whatsapp_card_exclusions VALUES(?1) ON CONFLICT DO NOTHING", [entry])?;
        Ok(())
    }
    /// The newest approval card shown in a chat, for the person who may answer it.
    pub fn whatsapp_card(&self, chat: &str, sender: &str) -> Option<String> {
        self.db
            .locked()
            .query_row(
                "SELECT entry_id FROM whatsapp_cards WHERE chat=?1 AND sender=?2 ORDER BY sent_at DESC",
                params![chat, sender],
                |r| r.get(0),
            )
            .ok()
    }
    pub fn forget_whatsapp_card(&self, chat: &str, entry: &str) -> Result<()> {
        self.db.locked().execute("DELETE FROM whatsapp_cards WHERE chat=?1 AND entry_id=?2", params![chat, entry])?;
        Ok(())
    }
}
