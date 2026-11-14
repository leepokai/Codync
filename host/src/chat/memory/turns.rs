//! Which bot's turn a change the scan finds in a memory folder is credited to: the turns running now, and the
//! memory files their tool calls touched. The scan itself is `files`.

use super::files::{self, SETTLE_MS};
use crate::LockExt;
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::Result;
use codync_mem::files as mf;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A bot's turn, from its prompt until the scan after it: a change found meanwhile in a folder the bot sees is the
/// bot's, even one no tool call of it named (a shell command, say), once the turn ran a tool that can write files.
#[derive(Clone, Debug)]
pub(super) struct Turn {
    bot: String,
    /// When it began, Unix ms; also its id.
    start: i64,
    /// The session it runs in: what of its changes was put back is told there.
    pub(super) session: String,
    /// As in `Runtime::message`: only then may the bot change or take out what memory says.
    pub(super) message: Option<String>,
    /// When it ended, Unix ms; `None` while it runs.
    ended: Option<i64>,
    /// It ran a tool that can write files (an edit, delete, move or command).
    wrote: bool,
}

// ponytail: in memory, so a host restart forgets turns; a change made just before one is then the computer's.
static TURNS: Mutex<Vec<Turn>> = Mutex::new(Vec::new());

/// A bot is about to prompt its agent. A turn of it that never ended (a fallback took over) is this one now.
pub fn begin_turn(bot: &str, session: &str, message: Option<String>) {
    let mut turns = TURNS.locked();
    turns.retain(|t| t.bot != bot || t.ended.is_some());
    turns.push(Turn {
        bot: bot.to_owned(),
        start: crate::store::now_ms(),
        session: session.to_owned(),
        message,
        ended: None,
        wrote: false,
    });
}

/// A bot's turn didn't start after all: nothing of it is left to scan.
pub fn drop_turn(bot: &str) {
    TURNS.locked().retain(|t| t.bot != bot || t.ended.is_some());
}

/// Notes that the bot's running turn ran a tool that can write files; the turn's start.
fn wrote(bot: &str) -> Option<i64> {
    let mut turns = TURNS.locked();
    let turn = turns.iter_mut().find(|t| t.bot == bot && t.ended.is_none())?;
    turn.wrote = true;
    Some(turn.start)
}

/// The tool call of a bot that last touched a memory file, and in which turn, so the scan credits the change to it.
#[derive(Serialize, Deserialize)]
struct Touch {
    bot: String,
    /// The [`Turn::start`] of the turn it was made in.
    #[serde(default)]
    turn: i64,
}

/// Notes the memory files a tool call names (its `locations` and diffs), so the scan knows which bot changed them,
/// and that its turn ran a tool that can write files. Only calls that change files count: a read left behind
/// would credit the next outside edit to the bot.
pub fn touched(hub: &Hub, cfg: &BotConfig, tool: &serde_json::Value) {
    let kind = tool["toolKind"].as_str();
    let changes =
        matches!(kind, Some("edit" | "delete" | "move")) || tool["diffs"].as_array().is_some_and(|d| !d.is_empty());
    if !changes && kind != Some("execute") {
        return;
    }
    let Some(turn) = wrote(&cfg.id).filter(|_| changes) else { return };
    let paths = tool["locations"]
        .as_array()
        .into_iter()
        .chain(tool["diffs"].as_array())
        .flatten()
        .filter_map(|item| item["path"].as_str());
    let folders: Vec<(PathBuf, Option<String>)> =
        files::layers(cfg).into_iter().map(|layer| (files::folder(layer.as_deref()), layer)).collect();
    for path in paths {
        // The name as the folders have it (`health/allergies.md`), whichever separator the agent's path used.
        let named = folders.iter().find_map(|(dir, layer)| {
            let parts: Option<Vec<&str>> = Path::new(path).strip_prefix(dir).ok()?.iter().map(|p| p.to_str()).collect();
            Some((layer, parts?.join("/"))).filter(|(_, name)| mf::valid_name(name))
        });
        let Some((layer, name)) = named else { continue };
        let touch = Touch { bot: cfg.id.clone(), turn };
        let value = serde_json::to_string(&touch).expect("a touch is plain data");
        if let Err(error) = hub.store.kv_set(&touch_key(layer.as_deref(), &name), &value) {
            tracing::warn!(error = format!("{error:#}"), "couldn't note a memory file edit");
        }
    }
}

fn touch_key(layer: Option<&str>, name: &str) -> String {
    format!("memory.touched.{}/{name}", files::folder(layer).display())
}

/// At the end of a turn: the bot's edits usually come right before its reply, so a file still settling is taken
/// once it has. The turn stays the bot's until then.
pub fn scan_after_turn(hub: Arc<Hub>, cfg: BotConfig) {
    let mut start = None;
    for turn in TURNS.locked().iter_mut().filter(|t| t.bot == cfg.id && t.ended.is_none()) {
        turn.ended = Some(crate::store::now_ms());
        start = Some(turn.start);
    }
    tokio::spawn(async move {
        let (h, c) = (hub.clone(), cfg.clone());
        if tokio::task::spawn_blocking(move || files::scan(&h, &c)).await.unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(u64::try_from(SETTLE_MS).unwrap_or(2_000) + 100)).await;
            let c = cfg.clone();
            let _ = tokio::task::spawn_blocking(move || files::scan(&hub, &c)).await;
        }
        TURNS.locked().retain(|t| t.bot != cfg.id || Some(t.start) != start);
    });
}

/// The bot's turn a change found in a layer's folder is credited to: the one whose tool call named the file, else
/// the turn of a bot that sees the folder and ran a tool that can write files, one the user didn't start first, so
/// the stricter rules apply. A turn that ended before the file was last modified didn't make it, unless that time
/// is in the future. Any other change is the computer's: the user's own, in an editor say. Clears the file's touch.
pub(super) fn turn_of(
    hub: &Hub,
    layer: Option<&str>,
    name: &str,
    mtime: Option<i64>,
) -> Result<Option<(BotConfig, Turn)>> {
    let key = touch_key(layer, name);
    let touch: Option<Touch> = hub.store.kv_get(&key).and_then(|v| serde_json::from_str(&v).ok());
    hub.store.kv_delete(&key)?;
    let now = crate::store::now_ms();
    let turns: Vec<Turn> = TURNS
        .locked()
        .iter()
        .filter(|t| match (t.ended, mtime) {
            (Some(ended), Some(at)) => at <= ended || at > now,
            _ => true,
        })
        .cloned()
        .collect();
    let config = |bot: &str| hub.store.bot(bot).ok().flatten().filter(|row| !row.deleted).map(|row| row.config);
    let named = touch.and_then(|touch| turns.iter().find(|t| t.bot == touch.bot && t.start == touch.turn).cloned());
    if let Some(turn) = named
        && let Some(cfg) = config(&turn.bot)
    {
        return Ok(Some((cfg, turn)));
    }
    Ok(turns
        .into_iter()
        .filter(|turn| turn.wrote)
        .filter_map(|turn| Some((config(&turn.bot)?, turn)))
        .filter(|(cfg, _)| files::layers(cfg).iter().any(|l| l.as_deref() == layer))
        .min_by_key(|(_, turn)| turn.message.is_some()))
}
