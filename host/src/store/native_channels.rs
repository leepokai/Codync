//! Durable receipts for host-owned Hermes messaging transports.

use super::Store;
use crate::LockExt;
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

pub(super) fn open(db: &Connection) -> Result<()> {
    db.execute_batch(include_str!("native_channels.sql"))?;
    // A request written to the transport before a crash may already have been delivered.
    db.execute("UPDATE native_channel_outbox SET state='unknown' WHERE state='sending'", [])?;
    db.execute("UPDATE native_channel_webhooks SET state='queued' WHERE state='claimed'", [])?;
    Ok(())
}

impl Store {
    pub(crate) fn native_enqueue(
        &self,
        connection: &str,
        generation: &str,
        event: &str,
        bot: &str,
        payload: &Value,
    ) -> Result<bool> {
        let mut db = self.db.locked();
        let tx = db.transaction()?;
        if tx
            .query_row(
                "SELECT 1 FROM native_channel_inbox WHERE connection=?1 AND generation=?2 AND event=?3",
                params![connection, generation, event],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(false);
        }
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM native_channel_inbox WHERE connection=?1 AND state='queued'",
            [connection],
            |r| r.get(0),
        )?;
        ensure!(count < 128, "This channel's incoming queue is full; retry the message later");
        tx.execute("INSERT INTO native_channel_inbox(connection,generation,event,bot,payload,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![connection,generation,event,bot,payload.to_string(),super::now_ms()])?;
        tx.commit()?;
        Ok(true)
    }

    pub(crate) fn native_pending(&self, connection: &str) -> Result<Vec<(String, String, String, Value)>> {
        let db = self.db.locked();
        let mut q = db.prepare("SELECT generation,event,bot,payload FROM native_channel_inbox WHERE connection=?1 AND state='queued' ORDER BY rowid LIMIT 8")?;
        let rows = q.query_map([connection], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        rows.map(|row| {
            let (generation, event, bot, payload) = row?;
            Ok((generation, event, bot, serde_json::from_str(&payload)?))
        })
        .collect()
    }

    pub(crate) fn native_reject(&self, connection: &str, generation: &str, event: &str) -> Result<()> {
        self.db.locked().execute("UPDATE native_channel_inbox SET state='rejected' WHERE connection=?1 AND generation=?2 AND event=?3 AND state='queued'",params![connection,generation,event])?;
        Ok(())
    }

    pub(crate) fn native_claim_prompt(
        &self,
        connection: &str,
        generation: &str,
        event: &str,
        entry: &str,
    ) -> Result<bool> {
        Ok(self.db.locked().execute("UPDATE native_channel_inbox SET state='started',entry=?4 WHERE connection=?1 AND generation=?2 AND event=?3 AND state='queued'",params![connection,generation,event,entry])? == 1)
    }

    #[allow(clippy::too_many_arguments)] // one column per argument of the route row
    pub(crate) fn native_route(
        &self,
        root: &str,
        connection: &str,
        generation: &str,
        bot: &str,
        chat: &str,
        user: &str,
        reply: &str,
    ) -> Result<()> {
        self.db.locked().execute("INSERT INTO native_channel_routes(root,connection,generation,bot,chat,user,reply_to) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(root) DO UPDATE SET user=excluded.user,reply_to=excluded.reply_to",params![root,connection,generation,bot,chat,user,reply])?;
        Ok(())
    }

    pub(crate) fn native_candidates(&self, connection: &str, generation: &str) -> Result<Vec<Value>> {
        let db = self.db.locked();
        let mut q = db.prepare("SELECT e.id,e.bot_id,e.thread_id,e.kind,e.data,r.chat,r.user,r.reply_to FROM entries e JOIN native_channel_routes r ON r.root=e.thread_id AND r.bot=e.bot_id WHERE r.connection=?1 AND r.generation=?2 AND ((e.kind='agent' AND json_extract(e.data,'$.final')=1) OR (e.kind='permission' AND json_extract(e.data,'$.status')='pending')) AND NOT EXISTS(SELECT 1 FROM native_channel_outbox o WHERE o.entry=e.id) ORDER BY e.seq LIMIT 24")?;
        let rows = q.query_map(params![connection, generation], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        rows.map(|row| { let (id,bot,root,kind,data,chat,user,reply) = row?; Ok(json!({"id":id,"bot":bot,"root":root,"kind":kind,"data":serde_json::from_str::<Value>(&data)?,"chat":chat,"user":user,"replyTo":reply})) }).collect()
    }

    pub(crate) fn native_reserve_send(
        &self,
        entry: &str,
        connection: &str,
        generation: &str,
        payload: &Value,
        code: Option<&str>,
        skipped: bool,
    ) -> Result<bool> {
        Ok(self.db.locked().execute("INSERT INTO native_channel_outbox(entry,connection,generation,payload,code,state,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(entry) DO NOTHING",params![entry,connection,generation,payload.to_string(),code,if skipped {"app_only"} else {"sending"},super::now_ms()])? == 1)
    }

    pub(crate) fn native_finish_send(&self, entry: &str, success: bool, external: Option<&str>) -> Result<()> {
        self.db.locked().execute(
            "UPDATE native_channel_outbox SET state=?2,external_id=?3,updated_at=?4 WHERE entry=?1 AND state='sending'",
            params![entry, if success { "sent" } else { "unknown" }, external, super::now_ms()],
        )?;
        Ok(())
    }

    pub(crate) fn native_card(
        &self,
        connection: &str,
        generation: &str,
        code: &str,
    ) -> Result<Option<(String, Value)>> {
        self.db.locked().query_row("SELECT entry,payload FROM native_channel_outbox WHERE connection=?1 AND generation=?2 AND code=?3 AND state IN ('sending','sent','unknown')",params![connection,generation,code], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?.map(|(entry,payload)|Ok((entry,serde_json::from_str(&payload)?))).transpose()
    }

    pub(crate) fn native_users(&self, connection: &str, generation: &str) -> Result<Vec<String>> {
        let db = self.db.locked();
        let mut q =
            db.prepare("SELECT user FROM native_channel_users WHERE connection=?1 AND generation=?2 AND allowed=1")?;
        Ok(q.query_map(params![connection, generation], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn native_user_decision(&self, connection: &str, generation: &str, user: &str) -> Result<Option<bool>> {
        Ok(self
            .db
            .locked()
            .query_row(
                "SELECT allowed FROM native_channel_users WHERE connection=?1 AND generation=?2 AND user=?3",
                params![connection, generation, user],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub(crate) fn native_set_user(&self, connection: &str, generation: &str, user: &str, allowed: bool) -> Result<()> {
        self.db.locked().execute("INSERT INTO native_channel_users(connection,generation,user,allowed) VALUES(?1,?2,?3,?4) ON CONFLICT(connection,generation,user) DO UPDATE SET allowed=excluded.allowed",params![connection,generation,user,allowed])?;
        Ok(())
    }

    pub(crate) fn native_pairing(&self, connection: &str, generation: &str, hash: &str, expires: i64) -> Result<()> {
        self.db.locked().execute("INSERT INTO native_channel_pairings(connection,generation,hash,expires,attempts) VALUES(?1,?2,?3,?4,0) ON CONFLICT(connection) DO UPDATE SET generation=excluded.generation,hash=excluded.hash,expires=excluded.expires,attempts=0",params![connection,generation,hash,expires])?;
        Ok(())
    }

    pub(crate) fn native_seed_pairing(
        &self,
        connection: &str,
        generation: &str,
        hash: &str,
        expires: i64,
    ) -> Result<()> {
        self.db.locked().execute("INSERT INTO native_channel_pairings(connection,generation,hash,expires,attempts) VALUES(?1,?2,?3,?4,0) ON CONFLICT(connection) DO UPDATE SET generation=excluded.generation,hash=excluded.hash,expires=excluded.expires,attempts=0 WHERE native_channel_pairings.generation<>excluded.generation",params![connection,generation,hash,expires])?;
        Ok(())
    }

    pub(crate) fn native_pairing_active(&self, connection: &str, generation: &str) -> Result<bool> {
        Ok(self.db.locked().query_row("SELECT EXISTS(SELECT 1 FROM native_channel_pairings WHERE connection=?1 AND generation=?2 AND expires>?3 AND attempts<32)",params![connection,generation,super::now_ms()], |r| r.get(0))?)
    }

    pub(crate) fn native_consume_pairing(
        &self,
        connection: &str,
        generation: &str,
        user: &str,
        hash: &str,
    ) -> Result<bool> {
        let mut db = self.db.locked();
        let tx = db.transaction()?;
        let expected: Option<String> = tx.query_row("SELECT hash FROM native_channel_pairings WHERE connection=?1 AND generation=?2 AND expires>?3 AND attempts<32",params![connection,generation,super::now_ms()],|r|r.get(0)).optional()?;
        let Some(expected) = expected else { return Ok(false) };
        tx.execute("UPDATE native_channel_pairings SET attempts=attempts+1 WHERE connection=?1", [connection])?;
        let valid = crate::remote::crypto::ct_eq(expected.as_bytes(), hash.as_bytes());
        if valid {
            tx.execute("UPDATE native_channel_pairings SET expires=0,attempts=32 WHERE connection=?1", [connection])?;
            tx.execute("INSERT INTO native_channel_users(connection,generation,user,allowed) VALUES(?1,?2,?3,1) ON CONFLICT(connection,generation,user) DO UPDATE SET allowed=1",params![connection,generation,user])?;
        }
        tx.commit()?;
        Ok(valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn incoming_receipts_claim_once_and_cannot_cross_accounts() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert!(store.native_enqueue("email", "account-a", "message", "bot", &json!({"text":"hello"})).unwrap());
        assert!(!store.native_enqueue("email", "account-a", "message", "bot", &json!({"text":"replay"})).unwrap());
        assert!(!store.native_claim_prompt("email", "account-b", "message", "entry").unwrap());
        assert!(store.native_claim_prompt("email", "account-a", "message", "entry").unwrap());
        assert!(!store.native_claim_prompt("email", "account-a", "message", "entry").unwrap());
        assert!(store.native_enqueue("email", "account-b", "message", "bot", &json!({"text":"new identity"})).unwrap());
    }

    #[test]
    fn consumed_pairing_never_reopens_on_reconnect_and_sender_denial_overrides_it() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let expiry = super::super::now_ms() + 60000;
        store.native_seed_pairing("signal", "a", "hash", expiry).unwrap();
        assert!(store.native_consume_pairing("signal", "a", "alice", "hash").unwrap());
        store.native_seed_pairing("signal", "a", "hash", expiry).unwrap();
        assert!(!store.native_consume_pairing("signal", "a", "mallory", "hash").unwrap());
        assert_eq!(store.native_user_decision("signal", "a", "alice").unwrap(), Some(true));
        store.native_set_user("signal", "a", "alice", false).unwrap();
        assert_eq!(store.native_user_decision("signal", "a", "alice").unwrap(), Some(false));
        assert_eq!(store.native_user_decision("signal", "b", "alice").unwrap(), None);
    }

    #[test]
    fn ambiguous_send_reservations_survive_restart_without_becoming_retryable() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert!(store.native_reserve_send("entry", "email", "a", &json!({"text":"report"}), None, false).unwrap());
        store.with_connection(open).unwrap();
        assert!(!store.native_reserve_send("entry", "email", "a", &json!({"text":"report"}), None, false).unwrap());
        let state: String = store
            .with_connection(|db| {
                Ok(db.query_row("SELECT state FROM native_channel_outbox WHERE entry='entry'", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(state, "unknown");
    }
}
