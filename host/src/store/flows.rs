//! Routine flows: the links that hand a routine's reply from bot to bot, the node positions of
//! each bot's canvas and the bots placed on it (`routines::flow` holds the rules, `flow_steps`
//! the steps of each run).
//!
//! Routines live in the kv blob (`routines.v1`), not in a table, so the cascade on deleting a
//! routine or a bot is explicit and runs in one transaction. Every change stamps a global `rev`
//! and records it for each bot whose canvas it touches (kv `routineflow.rev.<bot>`), which is
//! what a client's catch-up compares against.

use super::{Store, next_rev};
use anyhow::{Result, bail};
use rusqlite::{Connection, Transaction, params};
use serde::Serialize;
use std::collections::BTreeSet;

/// The coordinate limit of a node; far beyond any canvas a person would build.
pub const MAX_COORDINATE: f64 = 1_000_000.0;
/// kv prefix of the per-bot canvas revision.
pub const REV_PREFIX: &str = "routineflow.rev.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowLink {
    pub id: String,
    pub routine_id: String,
    pub from_bot_id: String,
    pub to_bot_id: String,
    pub instruction: String,
    pub created_at: i64,
    /// The bot that owns the routine, whose run starts the flow.
    #[serde(skip)]
    pub owner_bot_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPosition {
    pub node_key: String,
    pub x: f64,
    pub y: f64,
}

/// What a mutation touched: the rev it stamped and the bots whose canvas changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowChange {
    pub rev: i64,
    pub bots: Vec<String>,
}

pub(super) fn open(c: &Connection) -> Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS routine_flow_links(
            id TEXT PRIMARY KEY, routine_id TEXT NOT NULL, owner_bot_id TEXT NOT NULL,
            from_bot_id TEXT NOT NULL, to_bot_id TEXT NOT NULL, instruction TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL, UNIQUE(routine_id, from_bot_id, to_bot_id));
         CREATE INDEX IF NOT EXISTS routine_flow_links_routine ON routine_flow_links(routine_id);
         CREATE INDEX IF NOT EXISTS routine_flow_links_owner ON routine_flow_links(owner_bot_id);
         CREATE INDEX IF NOT EXISTS routine_flow_links_to ON routine_flow_links(to_bot_id);
         CREATE TABLE IF NOT EXISTS routine_flow_steps(
            id TEXT PRIMARY KEY, run_id TEXT NOT NULL, routine_id TEXT NOT NULL, owner_bot_id TEXT NOT NULL,
            bot_id TEXT NOT NULL, delivery_id TEXT, input TEXT NOT NULL, output TEXT,
            status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','skipped','cancelled')),
            error TEXT, run_at INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
            UNIQUE(run_id, bot_id));
         CREATE INDEX IF NOT EXISTS routine_flow_steps_routine ON routine_flow_steps(routine_id, run_at DESC);
         CREATE INDEX IF NOT EXISTS routine_flow_steps_status ON routine_flow_steps(status);
         CREATE INDEX IF NOT EXISTS routine_flow_steps_bot ON routine_flow_steps(bot_id);
         CREATE TABLE IF NOT EXISTS routine_flow_positions(
            canvas_bot_id TEXT NOT NULL, node_key TEXT NOT NULL, x REAL NOT NULL, y REAL NOT NULL,
            PRIMARY KEY(canvas_bot_id, node_key));
         CREATE INDEX IF NOT EXISTS routine_flow_positions_node ON routine_flow_positions(node_key);
         CREATE TABLE IF NOT EXISTS routine_flow_placed(
            canvas_bot_id TEXT NOT NULL, bot_id TEXT NOT NULL, PRIMARY KEY(canvas_bot_id, bot_id));
         CREATE INDEX IF NOT EXISTS routine_flow_placed_bot ON routine_flow_placed(bot_id);",
    )?;
    Ok(())
}

fn row_link(r: &rusqlite::Row) -> rusqlite::Result<FlowLink> {
    Ok(FlowLink {
        id: r.get(0)?,
        routine_id: r.get(1)?,
        owner_bot_id: r.get(2)?,
        from_bot_id: r.get(3)?,
        to_bot_id: r.get(4)?,
        instruction: r.get(5)?,
        created_at: r.get(6)?,
    })
}

const LINK_COLUMNS: &str = "id, routine_id, owner_bot_id, from_bot_id, to_bot_id, instruction, created_at";

fn links_of(c: &Connection, routine_id: &str) -> Result<Vec<FlowLink>> {
    let mut st = c.prepare(&format!(
        "SELECT {LINK_COLUMNS} FROM routine_flow_links WHERE routine_id = ?1 ORDER BY created_at, id"
    ))?;
    Ok(st.query_map([routine_id], row_link)?.collect::<rusqlite::Result<_>>()?)
}

/// Stamps one rev for `bots` and records it as each one's canvas revision.
pub(super) fn touch(tx: &Transaction, bots: impl IntoIterator<Item = String>) -> Result<Option<FlowChange>> {
    let bots: Vec<String> = bots.into_iter().collect::<BTreeSet<_>>().into_iter().collect();
    if bots.is_empty() {
        return Ok(None);
    }
    let rev = next_rev(tx)?;
    for bot in &bots {
        tx.execute(
            "INSERT INTO kv(k, v) VALUES(?1, ?2) ON CONFLICT(k) DO UPDATE SET v = ?2",
            params![format!("{REV_PREFIX}{bot}"), rev.to_string()],
        )?;
    }
    Ok(Some(FlowChange { rev, bots }))
}

/// The owner and every bot a link of `routine_id` names: whoever's canvas shows the routine.
pub(super) fn audience(c: &Connection, routine_id: &str, owner: &str) -> Result<BTreeSet<String>> {
    let mut bots = BTreeSet::from([owner.to_owned()]);
    for link in links_of(c, routine_id)? {
        bots.extend([link.from_bot_id, link.to_bot_id]);
    }
    Ok(bots)
}

fn check_node_key(key: &str) -> Result<()> {
    let ok = key
        .split_once(':')
        .is_some_and(|(kind, id)| matches!(kind, "routine" | "bot") && !id.is_empty() && !id.contains(':'));
    if !ok || key.len() > 200 {
        bail!("nodeKey must be routine:<id> or bot:<id>");
    }
    Ok(())
}

fn check_coordinate(v: f64) -> Result<()> {
    if !v.is_finite() || v.abs() > MAX_COORDINATE {
        bail!("coordinates must be numbers within ±{MAX_COORDINATE}");
    }
    Ok(())
}

impl Store {
    pub fn flow_links(&self, routine_id: &str) -> Result<Vec<FlowLink>> {
        self.with_connection(|c| links_of(c, routine_id))
    }

    pub fn flow_link(&self, id: &str) -> Result<Option<FlowLink>> {
        self.with_connection(|c| {
            let mut st = c.prepare(&format!("SELECT {LINK_COLUMNS} FROM routine_flow_links WHERE id = ?1"))?;
            Ok(st.query_map([id], row_link)?.next().transpose()?)
        })
    }

    /// The ids of the routines a bot's canvas shows besides its own: those a link of which names it.
    pub fn flow_routines_touching(&self, bot: &str) -> Result<Vec<String>> {
        self.with_connection(|c| {
            let mut st = c.prepare(
                "SELECT DISTINCT routine_id FROM routine_flow_links
                 WHERE owner_bot_id = ?1 OR from_bot_id = ?1 OR to_bot_id = ?1 ORDER BY routine_id",
            )?;
            Ok(st.query_map([bot], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn flow_links_for(&self, routine_ids: &[String]) -> Result<Vec<FlowLink>> {
        let mut all = vec![];
        for id in routine_ids {
            all.extend(self.flow_links(id)?);
        }
        Ok(all)
    }

    /// Adds a link after `validate` accepts it against the routine's current links, in one
    /// transaction so two connects can't together make a cycle.
    pub fn flow_add_link(
        &self,
        link: &FlowLink,
        validate: impl FnOnce(&[FlowLink]) -> Result<()>,
    ) -> Result<FlowChange> {
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            validate(&links_of(&tx, &link.routine_id)?)?;
            tx.execute(
                &format!("INSERT INTO routine_flow_links({LINK_COLUMNS}) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)"),
                params![
                    link.id,
                    link.routine_id,
                    link.owner_bot_id,
                    link.from_bot_id,
                    link.to_bot_id,
                    link.instruction,
                    link.created_at
                ],
            )?;
            let bots = audience(&tx, &link.routine_id, &link.owner_bot_id)?;
            let change = touch(&tx, bots)?.expect("the owner is always in the audience");
            tx.commit()?;
            Ok(change)
        })
    }

    pub fn flow_update_link(&self, id: &str, instruction: &str) -> Result<Option<(FlowLink, FlowChange)>> {
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            if tx.execute("UPDATE routine_flow_links SET instruction = ?2 WHERE id = ?1", params![id, instruction])?
                == 0
            {
                return Ok(None);
            }
            let mut st = tx.prepare(&format!("SELECT {LINK_COLUMNS} FROM routine_flow_links WHERE id = ?1"))?;
            let link = st.query_row([id], row_link)?;
            drop(st);
            let bots = audience(&tx, &link.routine_id, &link.owner_bot_id)?;
            let change = touch(&tx, bots)?.expect("the owner is always in the audience");
            tx.commit()?;
            Ok(Some((link, change)))
        })
    }

    pub fn flow_remove_link(&self, id: &str) -> Result<Option<FlowChange>> {
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            let mut st = tx.prepare(&format!("SELECT {LINK_COLUMNS} FROM routine_flow_links WHERE id = ?1"))?;
            let Some(link) = st.query_map([id], row_link)?.next().transpose()? else { return Ok(None) };
            drop(st);
            // The audience is read first: the removed link's bot may leave the canvas.
            let bots = audience(&tx, &link.routine_id, &link.owner_bot_id)?;
            tx.execute("DELETE FROM routine_flow_links WHERE id = ?1", [id])?;
            let change = touch(&tx, bots)?;
            tx.commit()?;
            Ok(change)
        })
    }

    pub fn flow_positions(&self, canvas_bot: &str) -> Result<Vec<FlowPosition>> {
        self.with_connection(|c| {
            let mut st = c.prepare(
                "SELECT node_key, x, y FROM routine_flow_positions WHERE canvas_bot_id = ?1 ORDER BY node_key",
            )?;
            let rows =
                st.query_map([canvas_bot], |r| Ok(FlowPosition { node_key: r.get(0)?, x: r.get(1)?, y: r.get(2)? }))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn flow_placed_bots(&self, canvas_bot: &str) -> Result<Vec<String>> {
        self.with_connection(|c| {
            let mut st =
                c.prepare("SELECT bot_id FROM routine_flow_placed WHERE canvas_bot_id = ?1 ORDER BY bot_id")?;
            Ok(st.query_map([canvas_bot], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn flow_save_position(&self, canvas_bot: &str, key: &str, x: f64, y: f64) -> Result<FlowChange> {
        check_node_key(key)?;
        check_coordinate(x)?;
        check_coordinate(y)?;
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO routine_flow_positions(canvas_bot_id, node_key, x, y) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(canvas_bot_id, node_key) DO UPDATE SET x = ?3, y = ?4",
                params![canvas_bot, key, x, y],
            )?;
            let change = touch(&tx, [canvas_bot.to_owned()])?.expect("one bot is given");
            tx.commit()?;
            Ok(change)
        })
    }

    /// Puts a bot on the canvas before any link reaches it, at `x`, `y`.
    pub fn flow_place_bot(&self, canvas_bot: &str, bot: &str, x: f64, y: f64) -> Result<FlowChange> {
        check_coordinate(x)?;
        check_coordinate(y)?;
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "INSERT OR IGNORE INTO routine_flow_placed(canvas_bot_id, bot_id) VALUES(?1, ?2)",
                params![canvas_bot, bot],
            )?;
            tx.execute(
                "INSERT INTO routine_flow_positions(canvas_bot_id, node_key, x, y) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(canvas_bot_id, node_key) DO UPDATE SET x = ?3, y = ?4",
                params![canvas_bot, format!("bot:{bot}"), x, y],
            )?;
            let change = touch(&tx, [canvas_bot.to_owned()])?.expect("one bot is given");
            tx.commit()?;
            Ok(change)
        })
    }

    /// Takes a node off the canvas; a `bot:` node also stops being placed. Nothing there is no change.
    pub fn flow_remove_position(&self, canvas_bot: &str, key: &str) -> Result<Option<FlowChange>> {
        check_node_key(key)?;
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            let mut gone = tx.execute(
                "DELETE FROM routine_flow_positions WHERE canvas_bot_id = ?1 AND node_key = ?2",
                params![canvas_bot, key],
            )?;
            if let Some(bot) = key.strip_prefix("bot:") {
                gone += tx.execute(
                    "DELETE FROM routine_flow_placed WHERE canvas_bot_id = ?1 AND bot_id = ?2",
                    params![canvas_bot, bot],
                )?;
            }
            let change = if gone == 0 { None } else { touch(&tx, [canvas_bot.to_owned()])? };
            tx.commit()?;
            Ok(change)
        })
    }

    /// A deleted routine leaves its links, steps and node positions.
    pub fn flow_forget_routine(&self, routine_id: &str, owner: &str) -> Result<Option<FlowChange>> {
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            let bots = audience(&tx, routine_id, owner)?;
            let mut gone = tx.execute("DELETE FROM routine_flow_links WHERE routine_id = ?1", [routine_id])?;
            gone += tx.execute("DELETE FROM routine_flow_steps WHERE routine_id = ?1", [routine_id])?;
            gone += tx
                .execute("DELETE FROM routine_flow_positions WHERE node_key = ?1", [format!("routine:{routine_id}")])?;
            let change = if gone == 0 { None } else { touch(&tx, bots)? };
            tx.commit()?;
            Ok(change)
        })
    }

    /// A deleted bot leaves every flow: its own routines' flows, the links that name it, its steps,
    /// its canvas and its place on others.
    pub fn flow_forget_bot(&self, bot: &str) -> Result<Option<FlowChange>> {
        self.with_connection(|c| {
            let tx = c.unchecked_transaction()?;
            let routines: Vec<(String, String)> = {
                let mut st = tx.prepare(
                    "SELECT DISTINCT routine_id, owner_bot_id FROM routine_flow_links
                     WHERE owner_bot_id = ?1 OR from_bot_id = ?1 OR to_bot_id = ?1",
                )?;
                st.query_map([bot], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
            };
            let mut bots = BTreeSet::new();
            for (routine, owner) in &routines {
                bots.extend(audience(&tx, routine, owner)?);
            }
            let node = format!("bot:{bot}");
            let mut gone = tx.execute(
                "DELETE FROM routine_flow_links WHERE owner_bot_id = ?1 OR from_bot_id = ?1 OR to_bot_id = ?1",
                [bot],
            )?;
            gone += tx.execute("DELETE FROM routine_flow_steps WHERE owner_bot_id = ?1 OR bot_id = ?1", [bot])?;
            gone += tx.execute(
                "DELETE FROM routine_flow_positions WHERE canvas_bot_id = ?1 OR node_key = ?2",
                params![bot, node],
            )?;
            gone += tx.execute("DELETE FROM routine_flow_placed WHERE canvas_bot_id = ?1 OR bot_id = ?1", [bot])?;
            tx.execute("DELETE FROM kv WHERE k = ?1", [format!("{REV_PREFIX}{bot}")])?;
            bots.remove(bot);
            let change = if gone == 0 { None } else { touch(&tx, bots)? };
            tx.commit()?;
            Ok(change)
        })
    }

    /// Canvas revisions after `since`: each bot whose flow changed since, with the rev.
    pub fn flow_revs_since(&self, since: i64) -> Vec<(String, i64)> {
        self.kv_prefix(REV_PREFIX)
            .into_iter()
            .filter_map(|key| {
                let rev = self.kv_get(&key)?.parse::<i64>().ok()?;
                let bot = key.strip_prefix(REV_PREFIX)?.to_owned();
                (rev > since).then_some((bot, rev))
            })
            .collect()
    }
}
