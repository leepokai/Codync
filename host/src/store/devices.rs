//! Authorized devices and their push and Live Activity tickets.

use super::{Store, logged, now_ms};
use crate::LockExt;
use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where an authorized device came from (wire values `local` / `account`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceSource {
    /// Paired with the QR code shown on this computer.
    Local,
    /// Approved here for a signed-in account; kept alive by a lease the cloud renews.
    Account,
}

/// What a device may do (wire values `control` / `screen`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Control,
    Screen,
}

/// One row of the authorized-device table (spec §4.3).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub key: String,
    pub name: String,
    pub platform: String,
    pub source: DeviceSource,
    #[serde(skip)]
    pub grant_id: Option<String>,
    pub scopes: Vec<Scope>,
    pub lease_until: Option<i64>,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
}

/// A push ticket with the device's push key (§6.7) and account context.
pub struct PushTicket {
    pub ticket: String,
    pub push_key: Option<String>,
    pub ctx: Option<String>,
}

const DEVICE_COLS: &str = "key, name, platform, source, grant_id, scopes, lease_until, created_at, last_seen_at";

fn row_device(r: &rusqlite::Row) -> rusqlite::Result<Device> {
    let source: String = r.get(3)?;
    let scopes: String = r.get(5)?;
    Ok(Device {
        key: r.get(0)?,
        name: r.get(1)?,
        platform: r.get(2)?,
        // Unknown values grant nothing: an unreadable source is an account device (leased),
        // unreadable scopes are none.
        source: serde_json::from_value(Value::String(source)).unwrap_or(DeviceSource::Account),
        grant_id: r.get(4)?,
        scopes: serde_json::from_str(&scopes).unwrap_or_default(),
        lease_until: r.get(6)?,
        created_at: r.get(7)?,
        last_seen_at: r.get(8)?,
    })
}

impl Store {
    pub fn device(&self, key: &str) -> Option<Device> {
        let c = self.db.locked();
        let row = c.query_row(&format!("SELECT {DEVICE_COLS} FROM devices WHERE key = ?1"), [key], row_device);
        logged("device", row.optional()).flatten()
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let c = self.db.locked();
        let mut st = c.prepare(&format!("SELECT {DEVICE_COLS} FROM devices ORDER BY created_at"))?;
        Ok(st.query_map([], row_device)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Adds (or, for a key paired again, refreshes) an authorized device.
    pub fn put_device(&self, d: &Device) -> Result<()> {
        let c = self.db.locked();
        c.execute(
            "INSERT INTO devices(key, name, platform, source, grant_id, scopes, lease_until, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(key) DO UPDATE SET name = ?2, platform = ?3, source = ?4, grant_id = ?5,
               scopes = ?6, lease_until = ?7",
            params![
                d.key,
                d.name,
                d.platform,
                serde_json::to_value(d.source)?.as_str(),
                d.grant_id,
                serde_json::to_string(&d.scopes)?,
                d.lease_until,
                d.created_at
            ],
        )?;
        Ok(())
    }

    /// Removes a device with its push and Live Activity tickets; false when it wasn't there.
    pub fn remove_device(&self, key: &str) -> Result<bool> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        let n = tx.execute("DELETE FROM devices WHERE key = ?1", [key])?;
        tx.execute("DELETE FROM push_tickets WHERE device_key = ?1", [key])?;
        tx.execute("DELETE FROM activity_tickets WHERE device_key = ?1", [key])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Extends an account device's lease (the cloud still lists its grant).
    pub fn set_lease(&self, key: &str, until: i64) -> Result<()> {
        let c = self.db.locked();
        c.execute("UPDATE devices SET lease_until = ?2 WHERE key = ?1", params![key, until])?;
        Ok(())
    }

    pub fn touch_device(&self, key: &str) -> Result<()> {
        let c = self.db.locked();
        c.execute("UPDATE devices SET last_seen_at = ?2 WHERE key = ?1", params![key, now_ms()])?;
        Ok(())
    }

    pub fn add_push_ticket(
        &self,
        ticket: &str,
        device_key: &str,
        push_key: Option<&str>,
        ctx: Option<&str>,
        name: &str,
    ) -> Result<()> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        // Relay tickets use a random nonce; registering again must replace this device's old ticket.
        tx.execute("DELETE FROM push_tickets WHERE device_key = ?1", [device_key])?;
        tx.execute(
            "INSERT INTO push_tickets(ticket, device_key, push_key, ctx, name, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(ticket) DO UPDATE SET device_key = ?2, push_key = ?3, ctx = ?4, name = ?5",
            params![ticket, device_key, push_key, ctx, name, now_ms()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Stops alerts to a device (its notification switch is off); Live Activity tickets stay.
    pub fn remove_push_tickets(&self, device_key: &str) -> Result<()> {
        let c = self.db.locked();
        c.execute("DELETE FROM push_tickets WHERE device_key = ?1", [device_key])?;
        Ok(())
    }

    pub fn remove_push_ticket(&self, ticket: &str) -> Result<()> {
        let c = self.db.locked();
        // If the newest ticket for an identity is dead or superseded, do not fall back
        // to its older rows on the next delivery. A concurrently replaced ticket is untouched.
        c.execute(
            "DELETE FROM push_tickets WHERE device_key = (SELECT device_key FROM push_tickets WHERE ticket = ?1)
             AND rowid <= (SELECT rowid FROM push_tickets WHERE ticket = ?1)",
            [ticket],
        )?;
        c.execute("DELETE FROM activity_tickets WHERE ticket = ?1", [ticket])?;
        Ok(())
    }

    pub fn push_tickets(&self) -> Vec<PushTicket> {
        let c = self.db.locked();
        let rows = c.prepare("SELECT ticket, push_key, ctx FROM push_tickets WHERE rowid IN (SELECT MAX(rowid) FROM push_tickets GROUP BY device_key) ORDER BY rowid").and_then(|mut st| {
            st.query_map([], |r| Ok(PushTicket { ticket: r.get(0)?, push_key: r.get(1)?, ctx: r.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()
        });
        logged("push tickets", rows).unwrap_or_default()
    }

    pub fn add_activity_ticket(&self, ticket: &str, bot_id: &str, device_key: &str) -> Result<()> {
        let mut c = self.db.locked();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM activity_tickets WHERE device_key = ?1 AND bot_id = ?2", params![device_key, bot_id])?;
        tx.execute(
            "INSERT INTO activity_tickets(ticket, bot_id, device_key, created_at) VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(ticket) DO UPDATE SET bot_id = ?2, device_key = ?3",
            params![ticket, bot_id, device_key, now_ms()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn activity_tickets(&self, bot_id: &str) -> Vec<String> {
        let c = self.db.locked();
        let rows = c
            .prepare("SELECT ticket FROM activity_tickets WHERE bot_id = ?1 AND rowid IN (SELECT MAX(rowid) FROM activity_tickets GROUP BY bot_id, device_key)")
            .and_then(|mut st| st.query_map([bot_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>());
        logged("activity tickets", rows).unwrap_or_default()
    }

    pub fn remove_activity_ticket(&self, bot_id: &str, device_key: &str) -> Result<()> {
        self.db.locked().execute(
            "DELETE FROM activity_tickets WHERE bot_id = ?1 AND device_key = ?2",
            params![bot_id, device_key],
        )?;
        Ok(())
    }

    /// Forgets a bot's Live Activity tickets (its activity ended); returns them.
    pub fn take_activity_tickets(&self, bot_id: &str) -> Vec<String> {
        let tickets = self.activity_tickets(bot_id);
        if let Err(error) = self.db.locked().execute("DELETE FROM activity_tickets WHERE bot_id = ?1", [bot_id]) {
            tracing::warn!(%error, bot = bot_id, "couldn't forget Live Activity tickets");
        }
        tickets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::temp_store;

    #[test]
    fn removing_a_device_drops_its_tickets() {
        let s = temp_store();
        let d = Device {
            key: "dk1".into(),
            name: "Phone".into(),
            platform: "ios".into(),
            source: DeviceSource::Local,
            grant_id: None,
            scopes: vec![Scope::Control, Scope::Screen],
            lease_until: None,
            created_at: 1,
            last_seen_at: None,
        };
        s.put_device(&d).unwrap();
        s.put_device(&Device { key: "dk2".into(), ..d.clone() }).unwrap();
        s.add_push_ticket("t1", "dk1", Some("pk"), Some("local"), "Phone").unwrap();
        s.add_push_ticket("t2", "dk2", None, None, "Phone").unwrap();
        s.add_activity_ticket("a1", "b1", "dk1").unwrap();
        s.add_activity_ticket("a2", "b1", "dk2").unwrap();
        let got = s.device("dk1").unwrap();
        assert_eq!((got.source, got.scopes), (DeviceSource::Local, vec![Scope::Control, Scope::Screen]));

        assert!(s.remove_device("dk1").unwrap());
        assert!(!s.remove_device("dk1").unwrap());
        assert!(s.device("dk1").is_none());
        assert_eq!(s.push_tickets().iter().map(|t| t.ticket.as_str()).collect::<Vec<_>>(), ["t2"]);
        assert_eq!(s.activity_tickets("b1"), ["a2"]);
        assert_eq!(s.take_activity_tickets("b1"), ["a2"]);
        assert_eq!(s.activity_tickets("b1").len(), 0);
    }

    #[test]
    fn only_latest_ticket_is_used_before_an_existing_phone_reregisters() {
        let s = temp_store();
        s.db.locked().execute_batch(
            "INSERT INTO push_tickets(ticket, device_key, created_at) VALUES('old', 'phone', 1), ('new', 'phone', 2), ('other', 'phone2', 3);
             INSERT INTO activity_tickets(ticket, bot_id, device_key, created_at) VALUES('old-a', 'bot', 'phone', 1), ('new-a', 'bot', 'phone', 2);"
        ).unwrap();
        let tickets = s.push_tickets();
        assert_eq!(tickets.len(), 2);
        assert!(!tickets.iter().any(|ticket| ticket.ticket == "old"));
        assert_eq!(s.activity_tickets("bot"), ["new-a"]);
        s.remove_push_ticket("new").unwrap();
        assert_eq!(s.push_tickets().iter().map(|ticket| ticket.ticket.as_str()).collect::<Vec<_>>(), ["other"]);
    }

    #[test]
    fn ending_one_phone_activity_preserves_other_bots_and_phones() {
        let s = temp_store();
        s.add_activity_ticket("one", "bot", "phone").unwrap();
        s.add_activity_ticket("two", "bot", "other-phone").unwrap();
        s.add_activity_ticket("three", "other-bot", "phone").unwrap();
        s.remove_activity_ticket("bot", "phone").unwrap();
        assert_eq!(s.activity_tickets("bot"), ["two"]);
        assert_eq!(s.activity_tickets("other-bot"), ["three"]);
        s.remove_activity_ticket("bot", "phone").unwrap();
        assert_eq!(s.activity_tickets("bot"), ["two"]);
    }

    #[test]
    fn registration_replaces_old_keys_and_activity_tokens_for_one_device_only() {
        let s = temp_store();
        s.add_push_ticket("old", "phone", None, Some("local"), "Phone").unwrap();
        s.add_push_ticket("other", "other-phone", Some("other-key"), Some("local"), "Other").unwrap();
        s.add_push_ticket("new", "phone", Some("new-key"), Some("local"), "Phone").unwrap();
        let tickets = s.push_tickets();
        assert_eq!(tickets.len(), 2);
        assert!(!tickets.iter().any(|t| t.ticket == "old"));
        assert_eq!(tickets.iter().find(|t| t.ticket == "new").unwrap().push_key.as_deref(), Some("new-key"));
        s.add_activity_ticket("old-activity", "bot", "phone").unwrap();
        s.add_activity_ticket("new-activity", "bot", "phone").unwrap();
        s.add_activity_ticket("other-bot", "bot2", "phone").unwrap();
        assert_eq!(s.activity_tickets("bot"), ["new-activity"]);
        s.remove_push_ticket("new-activity").unwrap();
        assert_eq!(s.activity_tickets("bot").len(), 0);
        assert_eq!(s.activity_tickets("bot2"), ["other-bot"]);
    }
}
