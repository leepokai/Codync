//! What a bot is told about itself, frozen per context window: Grok Bot's
//! "prompt snapshots".
//!
//! The bot's instructions (name, description, skills, memory) are rendered once
//! and kept as a [`Snapshot`] keyed by the ACP session and its *compaction
//! epoch*. Harnesses that take a system prompt (Claude, via `_meta.systemPrompt`)
//! get it as one; others get it in the first message of the session. The
//! snapshot stays byte-identical, so the prompt cache stays warm, until the
//! agent compacts its context (a `compaction_update`) or a new session starts.
//! Profile edits in between are announced once as an `<agent_profile_update>`
//! block on the next message and folded into the snapshot after the next
//! compaction. Memory learned in between reaches the prompt the same way.

use crate::memory::{self, Memory};
use crate::store::{BotConfig, Store};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub description: String,
    pub skills: Vec<String>,
}

impl Identity {
    pub fn of(cfg: &BotConfig) -> Self {
        let mut skills = cfg.skills.clone();
        skills.sort();
        Self { name: cfg.name.trim().to_owned(), description: cfg.description.trim().to_owned(), skills }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub session: String,
    pub epoch: u32,
    /// The rendered instructions, exactly as the agent got them.
    pub system: String,
    /// The identity rendered into `system`.
    pub system_identity: Identity,
    /// The identity the agent last heard of (`system` or a later update block).
    pub announced: Identity,
}

fn key(bot_id: &str) -> String {
    format!("context.{bot_id}")
}

fn epoch_key(bot_id: &str) -> String {
    format!("context.epoch.{bot_id}")
}

/// `(epoch, last compaction id)` stored as `session \t epoch \t id`.
fn epoch_state(store: &Store, bot_id: &str, session: &str) -> (u32, String) {
    store
        .kv_get(&epoch_key(bot_id))
        .and_then(|v| {
            let mut parts = v.splitn(3, '\t');
            (parts.next()? == session).then(|| {
                (parts.next().and_then(|n| n.parse().ok()).unwrap_or(0), parts.next().unwrap_or("").to_owned())
            })
        })
        .unwrap_or_default()
}

/// How many times the agent has compacted `session`.
pub fn epoch(store: &Store, bot_id: &str, session: &str) -> u32 {
    epoch_state(store, bot_id, session).0
}

/// Counts a finished compaction once (adapters may repeat its terminal update).
/// Returns whether it was new.
pub fn bump_epoch(store: &Store, bot_id: &str, session: &str, compaction_id: &str) -> Result<bool> {
    let (n, last) = epoch_state(store, bot_id, session);
    if !compaction_id.is_empty() && compaction_id == last {
        return Ok(false);
    }
    store.kv_set(&epoch_key(bot_id), &format!("{session}\t{}\t{compaction_id}", n + 1))?;
    Ok(true)
}

fn load(store: &Store, bot_id: &str) -> Option<Snapshot> {
    store.kv_get(&key(bot_id)).and_then(|v| serde_json::from_str(&v).ok())
}

fn save(store: &Store, bot_id: &str, s: &Snapshot) -> Result<()> {
    store.kv_set(&key(bot_id), &serde_json::to_string(s)?)
}

/// The snapshot for `session` at its current epoch, rendering a fresh one when
/// the stored one belongs to another session or an earlier epoch. Reads memory files.
pub fn resolve(store: &Store, cfg: &BotConfig, session: &str) -> Result<Snapshot> {
    let epoch = epoch(store, &cfg.id, session);
    if let Some(s) = load(store, &cfg.id).filter(|s| s.session == session && s.epoch == epoch) {
        return Ok(s);
    }
    adopt(store, cfg, session, render(store, cfg))
}

/// Records `system` (already handed to a new session) as that session's snapshot.
pub fn adopt(store: &Store, cfg: &BotConfig, session: &str, system: String) -> Result<Snapshot> {
    let identity = Identity::of(cfg);
    let s = Snapshot {
        session: session.to_owned(),
        epoch: epoch(store, &cfg.id, session),
        system,
        system_identity: identity.clone(),
        announced: identity,
    };
    save(store, &cfg.id, &s)?;
    Ok(s)
}

/// The bot's instructions, rendered live. Reads memory files.
pub fn render(store: &Store, cfg: &BotConfig) -> String {
    let name = cfg.name.trim();
    let mut lines = vec!["Agent profile:".to_owned()];
    if !name.is_empty() {
        lines.push(format!("Title: {name}"));
        lines.push(format!("Your agent name is \"{name}\". If the user asks for your name, answer with \"{name}\"."));
    }
    if !cfg.description.trim().is_empty() {
        lines.push(format!("Description: {}", cfg.description.trim()));
    }
    lines.push(
        "You are a persistent agent the user delegates work to from their phone through Codync. They see only your final reply of each turn, so keep it short and phone-friendly: say what you did and what needs the user."
            .to_owned(),
    );
    lines.push(crate::team::INSTRUCTIONS.to_owned());
    if let Some(skills) = crate::market::skills_brief(store, &cfg.skills) {
        lines.push(skills);
    }
    let mut out = lines.join("\n");
    match Memory::for_bot(&cfg.id) {
        Ok(mem) => {
            let (section, _) = memory::render(&mem.recall(memory::RECENT_PROMPT_LIMIT), mem.location());
            out.push_str("\n\n");
            out.push_str(&section);
        }
        Err(error) => tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "no memory folder"),
    }
    out
}

/// The `<agent_profile_update>` block to append to the next message when the
/// bot's identity changed since the agent last heard it.
pub fn profile_update(store: &Store, snapshot: &Snapshot, cfg: &BotConfig) -> Option<(String, Identity)> {
    let identity = Identity::of(cfg);
    if identity == snapshot.announced {
        return None;
    }
    let or_none = |s: &str, none: &str| if s.is_empty() { none.to_owned() } else { s.to_owned() };
    let skills = crate::market::skills_brief(store, &cfg.skills).unwrap_or_else(|| "Skills: none.".into());
    let text = [
        "<agent_profile_update>".to_owned(),
        "Your agent profile changed. This full update is authoritative and supersedes the Agent profile section in your instructions and every earlier profile update in this conversation.".to_owned(),
        format!("Current name: {}", or_none(&identity.name, "(no name)")),
        format!("Current description: {}", or_none(&identity.description, "(no description)")),
        skills,
        "Use this identity until a future conversation summary folds it into the Agent profile section.".to_owned(),
        "</agent_profile_update>".to_owned(),
    ]
    .join("\n");
    Some((text, identity))
}

/// After the turn that carried an update reached the agent: remember it heard
/// `identity`, unless the snapshot moved on meanwhile (a compaction re-rendered it).
pub fn mark_announced(store: &Store, bot_id: &str, turn_snapshot: &Snapshot, identity: Identity) -> Result<()> {
    let Some(mut current) = load(store, bot_id) else { return Ok(()) };
    if current.session != turn_snapshot.session
        || current.epoch != turn_snapshot.epoch
        || current.system != turn_snapshot.system
        || current.announced == identity
    {
        return Ok(());
    }
    current.announced = identity;
    save(store, bot_id, &current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str, description: &str) -> BotConfig {
        serde_json::from_value(serde_json::json!({
            "id": "5f0c7a52-0000-4000-8000-000000000001", "name": name, "description": description,
            "backend": "claude", "cwd": "/tmp",
        }))
        .expect("valid bot")
    }

    fn store() -> Store {
        let path = std::env::temp_dir().join(format!("codync-context-{}.db", uuid::Uuid::new_v4()));
        Store::open(&path).expect("temp store")
    }

    #[test]
    fn snapshot_is_frozen_until_compaction_and_updates_are_announced_once() {
        let store = store();
        let a = cfg("Fixer", "Fix bugs");
        let first = adopt(&store, &a, "s1", "SYSTEM v1".into()).expect("adopt");
        // Frozen: resolving again returns the same text, even though a live render differs.
        assert_eq!(resolve(&store, &a, "s1").expect("resolve").system, "SYSTEM v1");
        assert!(profile_update(&store, &first, &a).is_none());

        let b = cfg("Fixer", "Fix bugs and write tests");
        let (text, identity) = profile_update(&store, &first, &b).expect("changed");
        assert!(text.contains("Current description: Fix bugs and write tests"));
        mark_announced(&store, &a.id, &first, identity).expect("mark");
        let snap = resolve(&store, &b, "s1").expect("resolve");
        assert_eq!(snap.system, "SYSTEM v1");
        assert!(profile_update(&store, &snap, &b).is_none());

        // A compaction folds the new identity into a fresh render.
        assert!(bump_epoch(&store, &a.id, "s1", "c1").expect("bump"));
        assert!(!bump_epoch(&store, &a.id, "s1", "c1").expect("bump"));
        let folded = resolve(&store, &b, "s1").expect("resolve");
        assert_eq!(folded.epoch, 1);
        assert!(folded.system.contains("Description: Fix bugs and write tests"));
        // A new session starts over at epoch 0.
        assert_eq!(epoch(&store, &a.id, "s2"), 0);
    }
}
