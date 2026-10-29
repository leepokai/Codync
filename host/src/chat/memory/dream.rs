//! Dreaming in the host: when a bot's run starts, what it reads, and running its model on the bot's own harness.
//! What a run may change is the crate's decision (`codync_mem::dream`).

use crate::chat::{naming, oneshot};
use crate::hub::Hub;
use crate::store::{BotConfig, Entry, EntryKind, Store};
use anyhow::{Result, anyhow};
use codync_mem::dream::{self, Edits, Message};
use codync_mem::files::{self as mf, Author, Cause, Fact, MAIN};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

/// Quiet time after the last turn before a run.
const IDLE: Duration = Duration::from_mins(5);
/// User messages that start a run without waiting, and the most one run reads.
const BATCH: usize = 8;
/// Entries (the user's messages and final replies) one run looks at.
const READ_LIMIT: i64 = 40;
const MESSAGE_CHARS: usize = 4_000;
/// How often an idle bot checks whether its daily time review is due.
const REVIEW_CHECK: Duration = Duration::from_hours(1);

pub enum DreamEvent {
    /// A turn the user started has finished.
    Turn(naming::Turn),
    /// Run soon: a new session started or the context was compacted.
    Flush,
}

enum Ran {
    /// Read up to here; more may wait.
    Done { more: bool },
    /// Try again later without counting a failure: usage is high, or memory changed during the run.
    Later,
}

fn bookmark_key(bot: &str) -> String {
    format!("memory.dream.{bot}")
}

fn reviewed_key(bot: &str) -> String {
    format!("memory.dream.reviewed.{bot}")
}

/// A layer's MEMORY.md (`None`: the shared one) as a run last saw it nearly full, by its hash: it is tidied again
/// once it changed.
fn tidied_key(layer: Option<&str>) -> String {
    format!("memory.dream.tidied.{}", layer.unwrap_or("shared"))
}

/// Whether a layer's MEMORY.md (`content`) is nearly full and changed since a run last saw it so.
fn untidied(store: &Store, layer: Option<&str>, content: &str) -> bool {
    dream::full(content, layer) && store.kv_get(&tidied_key(layer)) != Some(mf::hash(content))
}

/// A run saw a layer's MEMORY.md (`content`): when it is nearly full, it isn't tidied again until it changes.
fn mark_tidied(store: &Store, layer: Option<&str>, content: &str) -> Result<()> {
    if dream::full(content, layer) {
        store.kv_set(&tidied_key(layer), &mf::hash(content))?;
    }
    Ok(())
}

/// Runs of bots that see the shared layer take turns, from what they read to what they write, so two never tidy or
/// change the shared MEMORY.md from the same copy.
// ponytail: one lock for every such run; a lock per layer if runs ever queue for long.
static SHARED: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Dreaming starts reading after `seq`.
pub(super) fn set_bookmark(store: &Store, bot: &str, seq: i64) -> Result<()> {
    store.kv_set(&bookmark_key(bot), &seq.to_string())
}

fn bookmark(store: &Store, bot: &str) -> i64 {
    store.kv_get(&bookmark_key(bot)).and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Starts the bot's dreamer. It runs once the bot has been quiet for [`IDLE`] after the user's turns, or right
/// away when [`BATCH`] of them are waiting, once a day for the time review, and when a MEMORY.md it sees is nearly
/// full, until it has room again or a run moves nothing out. It also names an unnamed bot. It stops when the bot
/// actor drops the sender; what it hasn't read waits for the next one.
pub fn spawn_dreamer(hub: Arc<Hub>, bot_id: String) -> mpsc::UnboundedSender<DreamEvent> {
    let (tx, mut rx) = mpsc::unbounded_channel::<DreamEvent>();
    tokio::spawn(async move {
        let mut waiting = waiting(&hub.store, &bot_id);
        let idle = tokio::time::sleep(IDLE);
        tokio::pin!(idle);
        let mut failures = 0u32;
        let mut retry_at: Option<Instant> = None;
        let quiet_until = |retry_at: Option<Instant>| {
            let after = Instant::now() + IDLE;
            retry_at.map_or(after, |at| at.max(after))
        };
        loop {
            let due = waiting > 0 || review_due(&hub.store, &bot_id) || tidy_due(&hub.store, &bot_id);
            let run = tokio::select! {
                event = rx.recv() => match event {
                    None => break,
                    Some(DreamEvent::Turn(turn)) => {
                        if let Err(e) = naming::observe(&hub, &bot_id, turn).await {
                            tracing::warn!(bot = %bot_id, error = format!("{e:#}"), "naming the bot failed");
                        }
                        waiting += 1;
                        idle.as_mut().reset(quiet_until(retry_at));
                        waiting >= BATCH && retry_at.is_none()
                    }
                    Some(DreamEvent::Flush) => {
                        // Recount: what waits may not have come as a turn (a call's spoken lines, a stopped turn).
                        waiting = self::waiting(&hub.store, &bot_id);
                        idle.as_mut().reset(retry_at.unwrap_or_else(Instant::now));
                        false
                    }
                },
                () = &mut idle, if due => true,
                () = tokio::time::sleep(REVIEW_CHECK), if !due => false,
            };
            if !run {
                continue;
            }
            let next = match self::run(&hub, &bot_id).await {
                Ok(Ran::Done { more }) => {
                    failures = 0;
                    retry_at = None;
                    waiting = if more { waiting.max(1) } else { self::waiting(&hub.store, &bot_id) };
                    if more { Instant::now() } else { quiet_until(None) }
                }
                Ok(Ran::Later) => quiet_until(retry_at),
                Err(e) => {
                    failures += 1;
                    retry_at = Some(Instant::now() + IDLE * 2u32.pow(failures.min(5)));
                    tracing::warn!(bot = %bot_id, failures, error = format!("{e:#}"), "dreaming failed; it retries later");
                    quiet_until(retry_at)
                }
            };
            idle.as_mut().reset(next);
        }
    });
    tx
}

/// The user's messages since the bookmark, a call's spoken lines each counting as one.
fn waiting(store: &Store, bot: &str) -> usize {
    store.conversation_since(bot, bookmark(store, bot), READ_LIMIT).map_or(0, |entries| {
        entries.iter().map(|e| if e.kind == EntryKind::User.as_str() { 1 } else { spoken(e).count() }).sum()
    })
}

/// What the user said on a call (`chat::calls`), from the voice's transcript.
fn spoken(e: &Entry) -> impl Iterator<Item = &str> {
    e.data["transcript"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["from"] == "user")
        .filter_map(|l| l["text"].as_str())
}

/// Once a day, when the bot's MEMORY.md holds a dated line it may rewrite (the time review).
fn review_due(store: &Store, bot: &str) -> bool {
    if store.kv_get(&reviewed_key(bot)).as_deref() == Some(today().as_str()) {
        return false;
    }
    store
        .memory_read(|c| mf::facts_of(c, Some(bot)))
        .is_ok_and(|facts| facts.iter().any(|f| !f.author.is_users() && dated(&f.text)))
}

/// When a MEMORY.md the bot sees is nearly full, and changed since a run last saw it so.
fn tidy_due(store: &Store, bot: &str) -> bool {
    let Some(cfg) = store.bot(bot).ok().flatten().filter(|b| !b.deleted).map(|b| b.config) else { return false };
    super::files::layers(&cfg).iter().any(|layer| {
        let layer = layer.as_deref();
        let content = store.memory_read(|c| mf::current(c, layer, MAIN)).ok().flatten().unwrap_or_default();
        untidied(store, layer, &content)
    })
}

/// Whether a memory names a date the time review may have to turn into the past: a year, a month or day in
/// Chinese, or an English month next to a number. Other numbers (prices, counts) don't count.
fn dated(text: &str) -> bool {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let chars: Vec<char> = text.chars().collect();
    let year =
        chars.windows(4).any(|w| matches!(w[..2], ['1', '9'] | ['2', '0']) && w[2..].iter().all(char::is_ascii_digit));
    let cjk = chars.windows(2).any(|w| w[0].is_ascii_digit() && "年月日號号".contains(w[1]));
    let lower = text.to_lowercase();
    let english = MONTHS.iter().any(|m| lower.contains(m)) && chars.iter().any(char::is_ascii_digit);
    year || cjk || english
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

async fn run(hub: &Arc<Hub>, bot_id: &str) -> Result<Ran> {
    let Some(cfg) = hub.store.bot(bot_id)?.filter(|b| !b.deleted).map(|b| b.config) else {
        return Ok(Ran::Done { more: false });
    };
    // Dreaming never spends the user's last headroom.
    if crate::usage::near_limit(hub, &cfg.backend) {
        return Ok(Ran::Later);
    }
    let shared = super::shared(&cfg);
    let _turn = if shared { Some(SHARED.lock().await) } else { None };
    let after = bookmark(&hub.store, bot_id);
    let entries = hub.store.conversation_since(bot_id, after, READ_LIMIT)?;
    let read = within_batch(&entries);
    let more = read.len() < entries.len() || i64::try_from(entries.len()).is_ok_and(|n| n == READ_LIMIT);
    let last_seq = read.last().map_or(after, |e| e.seq);
    let messages: Vec<Message> = read.iter().flat_map(messages).collect();
    let today = today();
    let review = review_due(&hub.store, bot_id);
    let tidy = tidy_due(&hub.store, bot_id);
    let worth = messages.iter().any(|m| m.from_user && m.text.iter().any(|run| codync_mem::evidence::substantive(run)));
    if !worth && !review && !tidy {
        set_bookmark(&hub.store, bot_id, last_seq)?;
        return Ok(Ran::Done { more });
    }

    let (h, c) = (hub.clone(), cfg.clone());
    tokio::task::spawn_blocking(move || super::files::scan(&h, &c)).await?;
    let snapshot = hub.store.memory_read(|c| dream::snapshot(c, bot_id, shared, &today))?;
    let (system, material) = dream::prompt(&snapshot, &messages);
    let raw = oneshot::one_shot(hub, &cfg, &system, &material).await?;
    let plan = dream::parse(&raw).ok_or_else(|| anyhow!("dreaming answered without a plan"))?;
    let (kept, mut rejected) = dream::audit(&snapshot, &messages, plan);
    let (edits, dropped) = hub.store.memory_read(|c| dream::edits(c, &snapshot, &kept, &messages))?;
    rejected.extend(dropped);
    if !rejected.is_empty() {
        tracing::info!(bot = %bot_id, dropped = rejected.len(), reasons = ?rejected, "dreaming dropped changes");
    }
    if !edits.files.is_empty() {
        let (h, c) = (hub.clone(), cfg.clone());
        if !tokio::task::spawn_blocking(move || write(&h, &c, &edits)).await?? {
            return Ok(Ran::Later);
        }
    }
    set_bookmark(&hub.store, bot_id, last_seq)?;
    if review {
        hub.store.kv_set(&reviewed_key(bot_id), &today)?;
    }
    // A run that moved lines out changed MEMORY.md, so the next one tidies on; one that moved none waits for a change.
    for layer in &snapshot.layers {
        mark_tidied(&hub.store, snapshot.layer_bot(layer.shared), layer.content.as_deref().unwrap_or_default())?;
    }
    Ok(Ran::Done { more })
}

/// Writes a run's files over what it saw, and logs it. False when a file changed meanwhile: what was written stays,
/// and the rest waits for the next run.
fn write(hub: &Hub, cfg: &BotConfig, edits: &Edits) -> Result<bool> {
    let dream = hub.store.memory_write(|c, now, _| dream::begin(c, &cfg.id, now))?;
    let writer = super::files::Writer { author: Author::Dream, bot: Some(cfg), cause: Cause::Dream, message: None };
    let mut whole = true;
    for edit in &edits.files {
        let layer = edit.bot.as_deref();
        let expected = edit.before.as_deref().map(mf::hash);
        let version = match super::files::write(hub, layer, &edit.name, Some(&edit.after), expected.as_deref(), writer)
        {
            Ok(version) => version,
            Err(error) => {
                tracing::info!(bot = %cfg.id, file = %edit.name, error = format!("{error:#}"), "dreaming's change wasn't written");
                whole = false;
                break;
            }
        };
        hub.store.memory_write(|c, now, _| {
            dream::record_file(c, dream, version.id)?;
            if edit.name != MAIN {
                return Ok(());
            }
            for line in edits.removed.iter().filter(|l| l.bot == edit.bot) {
                dream::record_line(c, dream, false, &line.text)?;
            }
            for line in edits.added.iter().filter(|l| l.bot == edit.bot) {
                dream::record_line(c, dream, true, &line.text)?;
                // A review's rewrite stays its writer's, and isn't reviewed again; a new line is dreaming's.
                let (author, author_bot) = match &line.rewrites {
                    Some(fact) => {
                        dream::mark_reviewed(c, layer, &line.text)?;
                        (fact.author, fact.author_bot.clone())
                    }
                    None => (Author::Dream, Some(cfg.id.clone())),
                };
                let fact = Fact { text: line.text.clone(), author, author_bot, message: line.message.clone(), at: now };
                mf::set_origin(c, layer, &fact)?;
            }
            Ok(())
        })?;
    }
    hub.store.memory_write(|c, _, _| dream::discard_if_empty(c, dream))?;
    tracing::info!(bot = %cfg.id, added = edits.added.len(), removed = edits.removed.len(), files = edits.files.len(), "memory dreamed");
    Ok(whole)
}

/// The entries up to the [`BATCH`]th user message and the replies after it.
fn within_batch(entries: &[Entry]) -> &[Entry] {
    let mut users = 0;
    for (i, e) in entries.iter().enumerate() {
        if e.kind == EntryKind::User.as_str() {
            if users == BATCH {
                return &entries[..i];
            }
            users += 1;
        }
    }
    entries
}

/// What a run sees of an entry. A user message counts only for what the user typed themselves; one the host wrote
/// in the user's place (`from: host`) isn't the user's words, nor one a call's voice model wrote (`from: voice`, it
/// may rephrase or translate: the call's transcript holds what the user said). A call's line gives its transcript,
/// line by line: the user's lines are their own words, the voice's are context.
fn messages(e: &Entry) -> Vec<Message> {
    let clip = |text: &str| crate::agent::acp::truncate(text, MESSAGE_CHARS);
    let said: Vec<(bool, Vec<String>)> = if e.kind == EntryKind::Notice.as_str() {
        e.data["transcript"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| (l["from"] == "user", vec![clip(l["text"].as_str().unwrap_or_default())]))
            .collect()
    } else if e.kind == EntryKind::User.as_str() {
        match e.data["from"].as_str() {
            Some("host") => Vec::new(),
            Some("voice") => vec![(false, vec![clip(e.data["text"].as_str().unwrap_or_default())])],
            _ => vec![(true, typed(&e.data).iter().map(|run| clip(run)).collect())],
        }
    } else {
        vec![(false, vec![clip(e.data["text"].as_str().unwrap_or_default())])]
    };
    said.into_iter()
        .filter_map(|(from_user, text)| {
            let text: Vec<String> = text.into_iter().filter(|run| !run.trim().is_empty()).collect();
            (!text.is_empty()).then(|| Message {
                id: e.id.clone(),
                from_user,
                text,
                date: super::dates::ymd(e.created_at),
            })
        })
        .collect()
}

/// The runs of a user message the user typed themselves: not pasted, quoted or forwarded text.
fn typed(data: &serde_json::Value) -> Vec<String> {
    let pasted: Vec<String> =
        data["pasted"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(str::to_owned)).collect();
    codync_mem::evidence::typed(data["text"].as_str().unwrap_or_default(), &pasted)
}

#[cfg(test)]
mod tests {
    use super::{dated, messages};
    use crate::store::{Entry, EntryKind};
    use serde_json::json;

    fn entry(kind: EntryKind, data: serde_json::Value) -> Entry {
        Entry {
            id: "e".into(),
            seq: 1,
            bot_id: "b".into(),
            rev: 1,
            kind: kind.as_str().into(),
            turn: 1,
            data,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn a_call_counts_what_the_user_said_not_what_the_voice_wrote() {
        let call = entry(
            EntryKind::Notice,
            json!({"text": "Voice chat", "call": "c", "transcript": [
                {"from": "user", "text": "我每週日早上去游泳"},
                {"from": "voice", "text": "好，記下了。"},
            ]}),
        );
        let seen: Vec<(bool, Vec<String>)> = messages(&call).into_iter().map(|m| (m.from_user, m.text)).collect();
        assert_eq!(seen, vec![(true, vec!["我每週日早上去游泳".to_owned()]), (false, vec!["好，記下了。".to_owned()])]);
        let relayed = entry(EntryKind::User, json!({"text": "Note: swims on Sundays", "call": "c", "from": "voice"}));
        assert!(messages(&relayed).iter().all(|m| !m.from_user));
        let spoken = entry(EntryKind::User, json!({"text": "swims on Sundays", "call": "c"}));
        assert!(messages(&spoken).iter().all(|m| m.from_user));
        assert!(messages(&entry(EntryKind::User, json!({"text": "x", "from": "host"}))).is_empty());
    }

    #[test]
    fn only_dates_trigger_the_time_review() {
        assert!(dated("Going to Singapore in July 2026"));
        assert!(dated("下個月 3號 要搬家"));
        assert!(dated("Dentist on Oct 21"));
        assert!(!dated("Prompt is too long: Provided 10,389 tokens"));
        assert!(!dated("Has 2 cats"));
    }
}
