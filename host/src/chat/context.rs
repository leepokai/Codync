//! What a bot is told about itself, frozen per context window: Grok Bot's
//! "prompt snapshots".
//!
//! The bot's instructions (name, description, skills, memory) are rendered once
//! and kept as a [`Snapshot`] keyed by the ACP session and its *compaction
//! epoch*. How it reaches the agent depends on the harness (`agent::bot::Instructions`).
//! The snapshot stays byte-identical, so the prompt cache stays warm, until the
//! agent compacts its context (a `compaction_update`) or a new session starts.
//! Profile edits in between are announced once as an `<agent_profile_update>`
//! block on the next message and folded into the snapshot after the next
//! compaction. Memory changes in between are told on the next message the same way
//! (see `memory::change_notice`).

use crate::store::{BotConfig, Store};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub description: String,
    /// The skills block the agent is told (`market::skills_brief`), so a skill renamed or redescribed counts too.
    #[serde(default)]
    pub skills_brief: String,
}

impl Identity {
    pub fn of(store: &Store, cfg: &BotConfig) -> Self {
        Self {
            name: cfg.name.trim().to_owned(),
            description: cfg.description.trim().to_owned(),
            skills_brief: crate::market::skills_brief(store, &cfg.skills).unwrap_or_default(),
        }
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
    /// The memory revision `system` holds (`memory::current_revision` before it was rendered).
    #[serde(default)]
    pub memory: String,
    /// True from the session's creation until a prompt that carried its instructions has reached the agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unprompted: bool,
}

// A bot talks in several sessions (its chat and groups, routine runs), each with its own: snapshots
// and epochs are kept per session.
fn key(bot_id: &str, session: &str) -> String {
    format!("context.{bot_id}.{session}")
}

fn epoch_key(bot_id: &str, session: &str) -> String {
    format!("context.epoch.{bot_id}.{session}")
}

/// `(epoch, last compaction id)` stored as `session \t epoch \t id`.
fn epoch_state(store: &Store, bot_id: &str, session: &str) -> (u32, String) {
    store
        .kv_get(&epoch_key(bot_id, session))
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
    store.kv_set(&epoch_key(bot_id, session), &format!("{session}\t{}\t{compaction_id}", n + 1))?;
    Ok(true)
}

fn load(store: &Store, bot_id: &str, session: &str) -> Option<Snapshot> {
    store.kv_get(&key(bot_id, session)).and_then(|v| serde_json::from_str(&v).ok())
}

fn save(store: &Store, bot_id: &str, s: &Snapshot) -> Result<()> {
    store.kv_set(&key(bot_id, &s.session), &serde_json::to_string(s)?)
}

/// The snapshot for `session` at its current epoch, rendering a fresh one when
/// the stored one belongs to another session or an earlier epoch.
pub fn resolve(store: &Store, cfg: &BotConfig, session: &str) -> Result<Snapshot> {
    let epoch = epoch(store, &cfg.id, session);
    let stored = load(store, &cfg.id, session);
    if let Some(s) = stored.as_ref().filter(|s| s.epoch == epoch && !s.system.is_empty()) {
        return Ok(s.clone());
    }
    let identity = Identity::of(store, cfg);
    let memory = crate::chat::memory::current_revision(store, &cfg.id);
    let unprompted = stored.as_ref().is_some_and(|old| old.unprompted);
    let s = Snapshot {
        session: session.to_owned(),
        epoch,
        system: render(store, cfg),
        system_identity: identity.clone(),
        unprompted,
        // A re-render reaches the agent only as Claude's system prompt or a re-sent profile (see `mark_announced`):
        // until then it knows the identity it last heard, so an update it missed is still announced.
        announced: stored.map_or(identity, |old| old.announced),
        memory,
    };
    save(store, &cfg.id, &s)?;
    Ok(s)
}

/// One session's instructions are rendered again on its next turn: before a compaction the host starts, so the
/// compaction keeps them as they are now.
pub fn refresh(store: &Store, bot_id: &str, session: &str) -> Result<()> {
    match load(store, bot_id, session) {
        Some(mut s) => {
            s.system.clear();
            save(store, bot_id, &s)
        }
        None => Ok(()),
    }
}

/// Every session of the bot renders its instructions again on its next turn (after the move from the old memory).
pub fn invalidate(store: &Store, bot_id: &str) -> Result<()> {
    for k in store.kv_prefix(&format!("context.{bot_id}.")) {
        match store.kv_get(&k).and_then(|v| serde_json::from_str::<Snapshot>(&v).ok()) {
            Some(mut s) => {
                s.system.clear();
                save(store, bot_id, &s)?;
            }
            None => store.kv_set(&k, "")?,
        }
    }
    Ok(())
}

/// Records `system` (already handed to a new session), rendered at memory revision `memory`, as that session's
/// snapshot.
pub fn adopt(store: &Store, cfg: &BotConfig, session: &str, system: String, memory: String) -> Result<Snapshot> {
    let identity = Identity::of(store, cfg);
    let s = Snapshot {
        session: session.to_owned(),
        epoch: epoch(store, &cfg.id, session),
        system,
        system_identity: identity.clone(),
        announced: identity,
        memory,
        unprompted: true,
    };
    save(store, &cfg.id, &s)?;
    Ok(s)
}

/// After the first prompt of a session reached the agent: its instructions are no longer owed, unless the
/// snapshot moved on meanwhile.
pub fn mark_prompted(store: &Store, bot_id: &str, sent: &Snapshot) -> Result<()> {
    update_if_current(store, bot_id, sent, |current| std::mem::take(&mut current.unprompted))
}

/// The bot's instructions, rendered live.
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
    // General on purpose: how to talk and where to work. Each tool's own instructions (MCP
    // servers, tool descriptions) explain the tool, so they aren't repeated here.
    lines.push(String::new());
    lines.push(
        "You are a persistent agent the user delegates work to through Codync, from their phone or computer."
            .to_owned(),
    );
    if cfg.lead {
        lines.push(crate::chat::lead::BRIEF.to_owned());
    }
    // Claude Code's own system prompt, which this is appended to, says the user is in its CLI.
    let engine = if cfg.backend == "custom" {
        "the coding agent".to_owned()
    } else {
        crate::agent::fallback::name(&cfg.backend)
    };
    lines.push(format!(
        "Environment: you run inside Codync. The user talks to you through Codync (its phone, desktop or terminal app), not in {engine}'s own CLI, IDE or app. The engine Codync runs you on is {engine}: where your base instructions describe its CLI, terminal, slash commands or keyboard shortcuts, they describe that engine, not where the user is, so don't point the user to them. To start over, the user starts a New session for this chat in Codync (the chat's menu in the apps, S in the terminal UI)."
    ));
    lines.push(String::new());
    lines.push(format!("Messages: {}", crate::chat::outbox::INSTRUCTIONS));
    lines.push(String::new());
    lines.push(
        "Other conversations: your history is shared across them. A group chat turn starts with [Group chat: ...]; a request from another bot or a routine run says so in its first line. In group chats, routines and requests from other bots, send_message isn't available: your final reply is what gets delivered (to the room, the run's result or the bot that asked), so write it for its reader; group chats and requests from other bots can't use message_bot either. Replies to your own message_bot requests arrive as a turn of their own in this chat, starting with [Codync: ...]: carry on with the user's task from there, and your final reply goes to the user. These rules apply to the current turn, not later turns in this session. Only @-mention in group chats, and don't assume a group sees your private chat."
            .to_owned(),
    );
    lines.push(String::new());
    lines.push(format!("Workspace: your default working directory is {}. {} Keep your own files and outputs there. For another project, use explicit paths or change directory for that command; don't change your bot configuration to work elsewhere. This directory is not a sandbox or a grant of access: follow the user's instructions and your permissions, and don't assume other bots' files are yours.", cfg.cwd, if crate::agent::workspace::is_managed(cfg) { "Codync allocated it for you alone." } else { "The user chose this project folder." }));
    if let Some(skills) = crate::market::skills_brief(store, &cfg.skills) {
        lines.push(String::new());
        lines.push(skills);
    }
    lines.push(String::new());
    lines.push(codync_mem::compaction::KEEP.to_owned());
    let mut out = lines.join("\n");
    let memory = crate::chat::memory::opening(store, cfg);
    if !memory.is_empty() {
        out.push_str("\n\n");
        out.push_str(&memory);
    }
    out
}

/// The `<agent_profile_update>` block to append to the next message when the
/// bot's identity changed since the agent last heard it.
pub fn profile_update(store: &Store, snapshot: &Snapshot, cfg: &BotConfig) -> Option<(String, Identity)> {
    let identity = Identity::of(store, cfg);
    if identity == snapshot.announced {
        return None;
    }
    let or_none = |s: &str, none: &str| if s.is_empty() { none.to_owned() } else { s.to_owned() };
    let skills = if identity.skills_brief.is_empty() { "Skills: none." } else { &identity.skills_brief };
    let text = [
        "<agent_profile_update>".to_owned(),
        "Your agent profile changed. This full update is authoritative and supersedes the Agent profile section in your instructions and every earlier profile update in this conversation.".to_owned(),
        format!("Current name: {}", or_none(&identity.name, "(no name)")),
        format!("Current description: {}", or_none(&identity.description, "(no description)")),
        skills.to_owned(),
        "Use this identity until a future conversation summary folds it into the Agent profile section.".to_owned(),
        "</agent_profile_update>".to_owned(),
    ]
    .join("\n");
    Some((text, identity))
}

/// After the turn that carried an update reached the agent: remember it heard
/// `identity`, unless the snapshot moved on meanwhile (a compaction re-rendered it).
pub fn mark_announced(store: &Store, bot_id: &str, turn_snapshot: &Snapshot, identity: Identity) -> Result<()> {
    update_if_current(store, bot_id, turn_snapshot, |current| {
        let changed = current.announced != identity;
        current.announced = identity;
        changed
    })
}

/// Applies `change` to the stored snapshot of `sent`'s session unless it moved on meanwhile (a re-render);
/// saves only when `change` returns true.
fn update_if_current(
    store: &Store,
    bot_id: &str,
    sent: &Snapshot,
    change: impl FnOnce(&mut Snapshot) -> bool,
) -> Result<()> {
    let Some(mut current) = load(store, bot_id, &sent.session) else { return Ok(()) };
    if current.epoch != sent.epoch || current.system != sent.system || !change(&mut current) {
        return Ok(());
    }
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
    fn instructions_say_where_bot_requests_and_their_replies_go() {
        let store = store();
        let system = render(&store, &cfg("Reporter", "Report outcomes"));
        assert!(system.contains("requests from other bots, send_message isn't available"));
        assert!(system.contains("Replies to your own message_bot requests arrive as a turn of their own"));
        assert!(system.contains("These rules apply to the current turn"));
    }

    #[test]
    fn instructions_place_the_user_in_codync_not_in_the_engine() {
        let system = render(&store(), &cfg("Reporter", ""));
        assert!(system.contains("not in Claude Code's own CLI, IDE or app"), "{system}");
        assert!(system.contains("The engine Codync runs you on is Claude Code:"), "{system}");
    }

    #[test]
    fn snapshot_is_frozen_until_compaction_and_updates_are_announced_once() {
        let store = store();
        let a = cfg("Fixer", "Fix bugs");
        let first = adopt(&store, &a, "s1", "SYSTEM v1".into(), String::new()).expect("adopt");
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

    #[test]
    fn a_rewritten_skill_is_announced() {
        let store = store();
        let skill = |description: &str| {
            let list = serde_json::json!([{"id": "release", "name": "Release", "description": description}]);
            store.kv_set("skills", &list.to_string()).expect("save skills");
        };
        skill("When shipping");
        let mut a = cfg("Fixer", "Fix bugs");
        a.skills = vec!["release".into()];
        let first = adopt(&store, &a, "s1", "SYSTEM v1".into(), String::new()).expect("adopt");
        assert!(profile_update(&store, &first, &a).is_none());
        skill("When shipping a release or a hotfix");
        let (text, _) = profile_update(&store, &first, &a).expect("the description changed");
        assert!(text.contains("- Release: When shipping a release or a hotfix"), "{text}");
    }

    #[test]
    fn a_memory_change_re_renders_but_keeps_what_the_agent_heard() {
        let store = store();
        let a = cfg("Fixer", "Fix bugs");
        adopt(&store, &a, "s1", "SYSTEM v1".into(), String::new()).expect("adopt");
        let b = cfg("Fixer", "Fix bugs and write tests");
        invalidate(&store, &a.id).expect("invalidate");
        let snap = resolve(&store, &b, "s1").expect("resolve");
        assert!(snap.system.contains("Description: Fix bugs and write tests"), "re-rendered");
        assert!(profile_update(&store, &snap, &b).is_some(), "the agent never heard the new description");
    }

    #[test]
    fn a_new_sessions_instructions_stay_owed_until_a_prompt_carried_them() {
        let store = store();
        let a = cfg("Fixer", "Fix bugs");
        let first = adopt(&store, &a, "s1", "SYSTEM v1".into(), String::new()).expect("adopt");
        assert!(resolve(&store, &a, "s1").expect("resolve").unprompted, "survives a restart");
        mark_prompted(&store, &a.id, &first).expect("mark");
        assert!(!resolve(&store, &a, "s1").expect("resolve").unprompted);
    }
}
