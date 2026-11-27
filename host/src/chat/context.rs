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

use crate::chat::work::Source;
use crate::store::{BotConfig, Store};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub description: String,
    /// The skills block the agent is told (`market::skills_brief`), so a skill renamed or redescribed counts too.
    #[serde(default)]
    pub skills_brief: String,
    /// Whether it's the user's lead bot (`chat::lead`), a role the user moves. None in a snapshot from before it
    /// could move (`parse` reads it from the instructions).
    #[serde(default)]
    pub lead: Option<bool>,
    /// Its projects (`BotConfig::projects`), the first its default: where its tasks work.
    #[serde(default)]
    pub projects: Vec<String>,
}

impl Identity {
    pub fn of(store: &Store, cfg: &BotConfig) -> Self {
        Self {
            name: cfg.name.trim().to_owned(),
            description: cfg.description.trim().to_owned(),
            skills_brief: current_skills(store, cfg),
            lead: Some(cfg.lead),
            projects: cfg.projects.clone(),
        }
    }

    fn skills_or_none(&self) -> &str {
        if self.skills_brief.is_empty() { "Skills: none." } else { &self.skills_brief }
    }
}

fn current_skills(store: &Store, cfg: &BotConfig) -> String {
    crate::market::skills_brief(store, &enabled_skills(store, cfg)).unwrap_or_default()
}

/// Standalone sessions use an empty skill list. Bot-backed sessions use their owner's saved list or `cfg.skills`.
pub fn enabled_skills<'a>(store: &Store, cfg: &'a BotConfig) -> Cow<'a, [String]> {
    if cfg.plain {
        return Cow::Borrowed(&[]);
    }
    store
        .bot(cfg.owner())
        .ok()
        .flatten()
        .filter(|row| !row.deleted)
        .map_or(Cow::Borrowed(&cfg.skills), |row| Cow::Owned(row.config.skills))
}

/// The start of `lead::BRIEF` in every version of it: instructions that hold it gave the bot the lead's role.
const LEAD_ROLE: &str = "\nLead: you are the user's lead bot";

/// What a bot that led hears once it no longer does.
const NOT_LEAD: &str = "Lead: you are no longer the user's lead bot; ignore the earlier Lead instructions.";

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
    /// The epoch of the delivered opening or update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    announced_epoch: Option<u32>,
    /// The skill block from the last opening accounted for in the task's context.
    #[serde(default, alias = "loadedTaskSkills", skip_serializing_if = "Option::is_none")]
    confirmed_task_skills: Option<String>,
    /// The skill block sent on a standard reopen that still needs confirmation in a prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_task_skills: Option<String>,
    /// A task prompt may have replaced the last acknowledged skill list.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    task_skills_uncertain: bool,
    /// The memory revision `system` holds (`memory::current_revision` before it was rendered).
    #[serde(default)]
    pub memory: String,
    /// True from the session's creation until a prompt that carried its instructions has reached the agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unprompted: bool,
}

impl Snapshot {
    pub fn needs_resend(&self) -> bool {
        self.announced_epoch.is_some_and(|announced| announced < self.epoch)
    }
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
    store.kv_get(&key(bot_id, session)).and_then(|v| parse(&v))
}

/// A stored snapshot. One from before the lead could move (2.33.0) told the agent its role in `system` alone.
fn parse(raw: &str) -> Option<Snapshot> {
    let mut s: Snapshot = serde_json::from_str(raw).ok()?;
    let led = Some(s.system.contains(LEAD_ROLE));
    s.system_identity.lead = s.system_identity.lead.or(led);
    s.announced.lead = s.announced.lead.or(led);
    Some(s)
}

fn save(store: &Store, bot_id: &str, s: &Snapshot) -> Result<()> {
    store.kv_set(&key(bot_id, &s.session), &serde_json::to_string(s)?)
}

/// The snapshot for `session` at its current epoch, rendering a fresh one when
/// the stored one belongs to another session or an earlier epoch.
pub fn resolve(store: &Store, cfg: &BotConfig, session: &str, opening_in_prompt: bool) -> Result<Snapshot> {
    // A thread's sessions are kept with its bot's, as its memory is.
    let owner = cfg.owner();
    let epoch = epoch(store, owner, session);
    let stored = load(store, owner, session);
    if let Some(s) = stored.as_ref().filter(|s| s.epoch == epoch && !s.system.is_empty()) {
        return Ok(s.clone());
    }
    let memory = crate::chat::memory::current_revision(store, owner);
    let (system, identity) = render_with_identity(store, cfg);
    let unprompted = stored.as_ref().map_or(opening_in_prompt && epoch > 0, |old| old.unprompted);
    let announced_epoch = stored.as_ref().map(|old| old.announced_epoch.unwrap_or(old.epoch));
    let (announced, confirmed_task_skills, pending_task_skills, task_skills_uncertain) = stored.map_or_else(
        || (identity.clone(), None, None, false),
        |old| (old.announced, old.confirmed_task_skills, old.pending_task_skills, old.task_skills_uncertain),
    );
    let s = Snapshot {
        session: session.to_owned(),
        epoch,
        system,
        system_identity: identity.clone(),
        unprompted,
        announced,
        announced_epoch,
        confirmed_task_skills,
        pending_task_skills,
        task_skills_uncertain,
        memory,
    };
    save(store, owner, &s)?;
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
        match store.kv_get(&k).and_then(|v| parse(&v)) {
            Some(mut s) => {
                s.system.clear();
                save(store, bot_id, &s)?;
            }
            None => store.kv_set(&k, "")?,
        }
    }
    Ok(())
}

/// Records `system` (already handed to a new session), rendered with `identity` at memory revision `memory`, as that
/// session's snapshot.
pub fn adopt(
    store: &Store,
    cfg: &BotConfig,
    session: &str,
    system: String,
    identity: Identity,
    memory: String,
) -> Result<Snapshot> {
    let bot_id = cfg.owner();
    let confirmed_task_skills =
        (!as_bot(cfg, &crate::chat::work::source_of(store, cfg))).then(|| identity.skills_brief.clone());
    let s = Snapshot {
        session: session.to_owned(),
        epoch: epoch(store, bot_id, session),
        system,
        system_identity: identity.clone(),
        announced: identity,
        announced_epoch: None,
        confirmed_task_skills,
        pending_task_skills: None,
        task_skills_uncertain: false,
        memory,
        unprompted: true,
    };
    save(store, bot_id, &s)?;
    Ok(s)
}

/// Records the epoch of the opening carried by a successful prompt.
pub fn mark_prompted(store: &Store, bot_id: &str, sent: &Snapshot) -> Result<()> {
    update_if_current(store, bot_id, sent, |current| {
        let first = std::mem::take(&mut current.unprompted);
        let previous = current.announced_epoch.replace(sent.epoch);
        first || previous != Some(sent.epoch)
    })
}

/// Whether a session of `cfg` working for `source` is told who the bot is (name, description, memory): all but a
/// thread on a task the bot handed out, which is told the task alone.
pub fn as_bot(cfg: &BotConfig, source: &Source) -> bool {
    !cfg.is_thread() || !source.is_bot()
}

/// What a thread on a handed-out task is told of itself.
const TASK: &str = "You are doing one task for the user through Codync, away from the chat where it was handed out: \
everything you know about it is in the message that gives it to you, and you don't see that chat. If something you \
need isn't there, look it up (search_memory and read_memory, the files) before you ask. Your last message is your \
report: the bot that gave you the task tells the user what matters, so say what you did, what you found, what is left \
and where (files, branch). When the user must choose before you can go on, call request_decision; to hand over a file \
or a page, send_file or publish_artifact. Save what should outlast this task with edit_memory. When the bot that gave \
you the task asks you a question, answer it in one short message, then go on with your task.";

/// What a thread the user opened with the bot is told of where it is.
const USER: &str = "Away from your chat: the user opened this thread to work with you directly. It starts without \
your chat's conversation; the user reads your answers here and may write to you again. When they must choose before \
you can go on, call request_decision; to hand over a file or a page, send_file or publish_artifact.";

/// What a thread on a routine's run is told of where it is.
const ROUTINE: &str = "Away from your chat: you are running one of your routines. Its message says which and what to \
do; your final reply is the run's result, posted in your chat, so write it for the user.";

/// What a thread on another bot's request is told of where it is.
const REQUEST: &str = "Away from your chat: another bot asked you something. Its message says who and what; your \
final reply goes back to that bot as your answer, so write it for that bot.";

const OTHER_COMPUTERS: &str = "Other computers: the user's other computers and their bots are reached through Codync's network. To see, ask or hand work to a bot or Codync on another computer, use list_bots and message_bot (ids like name@computer), not SSH, remote shells or your engine's own agent and session tools, which only see this computer. Use SSH only when the user asks for it or the request is about that computer's files or system rather than its bots.";

/// The instructions of a session of `cfg`, a bot or one of its threads, rendered live.
#[cfg(test)]
pub fn render(store: &Store, cfg: &BotConfig) -> String {
    render_with_identity(store, cfg).0
}

/// The instructions of a session of `cfg`, rendered live alongside the [`Identity`] baked into them so `adopt`
/// records what the agent actually received even if skills or profile fields change before `session/new` replies.
pub fn render_with_identity(store: &Store, cfg: &BotConfig) -> (String, Identity) {
    let identity = Identity::of(store, cfg);
    let system = if cfg.is_thread() {
        render_thread(store, cfg, &crate::chat::work::source_of(store, cfg), &identity)
    } else {
        render_bot(store, cfg, &identity)
    };
    (system, identity)
}

/// Who the bot is: its name and description.
fn identity_lines(identity: &Identity) -> Vec<String> {
    let mut lines = vec!["Agent profile:".to_owned()];
    if !identity.name.is_empty() {
        lines.push(format!("Title: {}", identity.name));
        lines.push(format!(
            "Your agent name is \"{}\". If the user asks for your name, answer with \"{}\".",
            identity.name, identity.name
        ));
    }
    if !identity.description.is_empty() {
        lines.push(format!("Description: {}", identity.description));
    }
    lines.push(String::new());
    lines.push(
        "You are a persistent agent the user delegates work to through Codync, from their phone or computer."
            .to_owned(),
    );
    lines
}

/// Where the agent runs: in Codync, not in its engine's own app.
fn environment(cfg: &BotConfig, chat: bool) -> String {
    // Claude Code's own system prompt, which this is appended to, says the user is in its CLI.
    let engine = if cfg.backend == "custom" {
        "the coding agent".to_owned()
    } else {
        crate::agent::fallback::name(&cfg.backend)
    };
    let mut line = format!(
        "Environment: you run inside Codync. The user talks to you through Codync (its phone, desktop or terminal app), not in {engine}'s own CLI, IDE or app. The engine Codync runs you on is {engine}: where your base instructions describe its CLI, terminal, slash commands or keyboard shortcuts, they describe that engine, not where the user is, so don't point the user to them."
    );
    if chat {
        line.push_str(" A chat in Codync is one conversation that goes on; where Codync offers New session in the bot's menu (^k in the terminal UI), the user can pick it to start over with a fresh context, and the chat stays.");
    }
    line
}

/// What every session ends with: the skills, the compaction keep-list and, told who the bot is, its memory.
fn closing(store: &Store, cfg: &BotConfig, mut lines: Vec<String>, memory: bool, skills_brief: &str) -> String {
    if !skills_brief.is_empty() {
        lines.push(String::new());
        lines.push(skills_brief.to_owned());
    }
    lines.push(String::new());
    lines.push(codync_mem::compaction::KEEP.to_owned());
    let mut out = lines.join("\n");
    let memory = if memory { crate::chat::memory::opening(store, &cfg.as_owner()) } else { String::new() };
    if !memory.is_empty() {
        out.push_str("\n\n");
        out.push_str(&memory);
    }
    out
}

/// The bot's own instructions.
fn render_bot(store: &Store, cfg: &BotConfig, identity: &Identity) -> String {
    let mut lines = identity_lines(identity);
    if cfg.lead {
        lines.push(crate::chat::lead::BRIEF.to_owned());
    }
    // General on purpose: how to talk and where to work. Each tool's own instructions (MCP
    // servers, tool descriptions) explain the tool, so they aren't repeated here.
    lines.push(environment(cfg, true));
    lines.push(String::new());
    lines.push(format!("Messages: {}", crate::chat::outbox::instructions(false)));
    lines.push(String::new());
    lines.push(
        "Other conversations: your history is shared across them. A group chat turn starts with [Group chat: ...]; a request from another bot or a routine run says so in its first line; in your own chat, each user message starts with [Codync: message <id>], the id send_message and send_file take as reply_to. In group chats, routines and requests from other bots, send_message isn't available: your final reply is what gets delivered (to the room, the run's result or the bot that asked), so write it for its reader; group chats and requests from other bots can't use message_bot either. Replies to your own message_bot requests arrive as a turn of their own in this chat, starting with [Codync: <bot> replied to your message_bot request ...] (or couldn't finish it): carry on with the user's request from there, and your final reply goes to the user. These rules apply to the current turn, not later turns in this session. Only @-mention in group chats, and don't assume a group sees your private chat."
            .to_owned(),
    );
    lines.push(String::new());
    lines.push(OTHER_COMPUTERS.to_owned());
    lines.push(String::new());
    lines.push(workspace(cfg));
    lines.push(String::new());
    lines.push(crate::dev_servers::BRIEF.to_owned());
    lines.push(String::new());
    lines.push(crate::routines::BRIEF.to_owned());
    lines.push(String::new());
    lines.push(crate::chat::work::BRIEF.to_owned());
    closing(store, cfg, lines, true, &identity.skills_brief)
}

/// A thread's instructions: what its task needs, never the chat's own paragraphs (messages, rooms, the event box).
/// On a handed-out task it is told the task alone; otherwise who the bot is too, as the bot away from its chat.
fn render_thread(store: &Store, cfg: &BotConfig, source: &Source, identity: &Identity) -> String {
    let as_bot = as_bot(cfg, source);
    let mut lines = if as_bot { identity_lines(identity) } else { Vec::new() };
    lines.push(
        match source {
            Source::Bot => TASK,
            Source::User => USER,
            Source::Routine { .. } => ROUTINE,
            Source::Request { .. } => REQUEST,
        }
        .to_owned(),
    );
    lines.push(String::new());
    lines.push(environment(cfg, false));
    lines.push(String::new());
    lines.push(format!(
        "Workspace: you work in {}. {} Keep to it: don't change Codync settings to work elsewhere.",
        cfg.cwd,
        thread_origin(cfg)
    ));
    lines.push(String::new());
    lines.push(crate::dev_servers::BRIEF.to_owned());
    // The servers a run and a request keep (`Actor::mcp_servers`).
    if !source.own() {
        lines.push(String::new());
        lines.push(crate::routines::BRIEF.to_owned());
    }
    if matches!(source, Source::Routine { .. }) {
        lines.push(String::new());
        lines.push(OTHER_COMPUTERS.to_owned());
    }
    closing(store, cfg, lines, as_bot, &identity.skills_brief)
}

/// Whose a thread's folder is.
fn thread_origin(cfg: &BotConfig) -> &'static str {
    if crate::agent::workspace::own_folder(cfg).is_some() {
        "It is the bot's own folder, which Codync keeps versions of in git."
    } else if crate::chat::work::is_worktree(&cfg.cwd) {
        "It is a git worktree Codync made for this task, on a branch of its own: the user's uncommitted changes are not in it."
    } else {
        "It is a folder the user chose."
    }
}

/// The bot's Workspace paragraph: its own folder, and its projects.
fn workspace(cfg: &BotConfig) -> String {
    format!(
        "Workspace: your own folder is {}, your working directory. {} Keep your own files and outputs there, and never change your own folder or your bot configuration to work elsewhere. {} This directory is not a sandbox or a grant of access: follow the user's instructions and your permissions, and don't assume other bots' files are yours.",
        cfg.cwd,
        workspace_origin(cfg),
        projects_line(&cfg.projects)
    )
}

/// What the bot is told of its projects, in its instructions and a profile update.
fn projects_line(projects: &[String]) -> String {
    match projects {
        [] => "You have no projects: code work in a folder the user names goes to a task with that folder (add_task folder). To read a folder's files yourself, use explicit paths.".to_owned(),
        [default, rest @ ..] => {
            let others = if rest.is_empty() { String::new() } else { format!("; also {}", rest.join(", ")) };
            format!(
                "Your projects, the user's folders: {default} (your default){others}. Code work in a project goes to a task with that folder (add_task folder; left out, your default project), not into your own folder. To read a project's files yourself, use explicit paths."
            )
        }
    }
}

/// Whose the bot's working directory is, and for its own folder, the versions Codync keeps there.
fn workspace_origin(cfg: &BotConfig) -> &'static str {
    match crate::agent::workspace::own_folder(cfg) {
        Some(dir) if crate::agent::versions::kept(&dir) => {
            "Codync allocated it for you alone and keeps its versions in git: it commits the folder before (\"Before: ...\") and after each turn, so git log and git diff show what each turn changed, and you can bring back an earlier version of a file when the user asks."
        }
        Some(_) => "Codync allocated it for you alone.",
        None => "The user chose this project folder.",
    }
}

/// The `<agent_profile_update>` block to append to the next message when the
/// bot's identity changed since the agent last heard it.
pub fn profile_update(store: &Store, snapshot: &Snapshot, cfg: &BotConfig) -> Option<(String, Identity)> {
    let identity = Identity::of(store, cfg);
    if identity == snapshot.announced {
        return None;
    }
    let or_none = |s: &str, none: &str| if s.is_empty() { none.to_owned() } else { s.to_owned() };
    // Its role, when it leads or its instructions or an earlier update said it did.
    let role = if identity.lead == Some(true) {
        crate::chat::lead::BRIEF
    } else if snapshot.announced.lead == Some(true) || snapshot.system_identity.lead == Some(true) {
        NOT_LEAD
    } else {
        ""
    };
    let text = [
        "<agent_profile_update>".to_owned(),
        "Your agent profile changed. This full update is authoritative and supersedes the Agent profile section in your instructions and every earlier profile update in this conversation.".to_owned(),
        format!("Current name: {}", or_none(&identity.name, "(no name)")),
        format!("Current description: {}", or_none(&identity.description, "(no description)")),
        identity.skills_or_none().to_owned(),
        // A thread has none: the bot's projects are the bot's to send work to.
        if cfg.is_thread() { String::new() } else { projects_line(&identity.projects) },
        role.to_owned(),
        "Use this identity until a future conversation summary folds it into the Agent profile section.".to_owned(),
        "</agent_profile_update>".to_owned(),
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    Some((text, identity))
}

/// Replaces the skill list a handed-out task last heard.
pub fn task_skills_update(
    store: &Store,
    snapshot: &Snapshot,
    cfg: &BotConfig,
    resend: bool,
) -> Option<(String, Identity)> {
    let skills_brief = current_skills(store, cfg);
    if !resend
        && !snapshot.task_skills_uncertain
        && snapshot.pending_task_skills.is_none()
        && skills_brief == snapshot.announced.skills_brief
    {
        return None;
    }
    let identity = Identity { skills_brief, ..snapshot.announced.clone() };
    let text = format!(
        "<skills_update>\nUse this list in place of all earlier skill lists for this task.\n{}\n</skills_update>",
        identity.skills_or_none()
    );
    Some((text, identity))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TaskSystemDelivery {
    Immediate,
    MayDefer,
}

/// Accounts for the skills in a successfully reopened task's system prompt.
pub fn task_system_reopened(store: &Store, bot_id: &str, sent: &Snapshot, delivery: TaskSystemDelivery) -> Result<()> {
    let skills = &sent.system_identity.skills_brief;
    update_if_current(store, bot_id, sent, |current| {
        let differs_from_announced = &current.announced.skills_brief != skills;
        let changed = current.confirmed_task_skills.as_ref().map_or(differs_from_announced, |last| last != skills);
        if delivery == TaskSystemDelivery::MayDefer
            && (current.confirmed_task_skills.is_none()
                || (changed && differs_from_announced)
                || current.pending_task_skills.is_some())
        {
            if current.pending_task_skills.as_ref() == Some(skills) {
                return false;
            }
            current.pending_task_skills = Some(skills.clone());
            return true;
        }
        let cleared = current.pending_task_skills.take().is_some();
        if current.confirmed_task_skills.as_ref() == Some(skills) {
            return cleared;
        }
        current.confirmed_task_skills = Some(skills.clone());
        if changed && delivery == TaskSystemDelivery::Immediate {
            current.announced.skills_brief.clone_from(skills);
        }
        true
    })
}

pub(crate) fn mark_task_skills_uncertain(store: &Store, bot_id: &str, sent: &Snapshot) -> Result<()> {
    update_if_current(store, bot_id, sent, |current| !std::mem::replace(&mut current.task_skills_uncertain, true))
}

/// After the turn that carried an update reached the agent: remember it heard
/// `identity`, unless the snapshot moved on meanwhile (a compaction re-rendered it).
pub fn mark_announced(store: &Store, bot_id: &str, turn_snapshot: &Snapshot, identity: Identity) -> Result<()> {
    update_if_current(store, bot_id, turn_snapshot, |current| {
        let changed = current.announced != identity;
        current.announced = identity;
        let previous = current.announced_epoch.replace(turn_snapshot.epoch);
        let reasserted = turn_snapshot.task_skills_uncertain && std::mem::take(&mut current.task_skills_uncertain);
        if turn_snapshot.pending_task_skills.is_some()
            && current.pending_task_skills == turn_snapshot.pending_task_skills
        {
            current.confirmed_task_skills = current.pending_task_skills.take();
            return true;
        }
        changed || reasserted || previous != Some(turn_snapshot.epoch)
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

    fn thread_of(owner: &BotConfig) -> BotConfig {
        BotConfig { id: "thread-1".into(), thread_of: Some(owner.id.clone()), ..owner.clone() }
    }

    #[test]
    fn a_thread_on_a_handed_out_task_is_told_the_task_alone() {
        let store = store();
        let thread = thread_of(&cfg("Fixer", "Fix bugs"));
        let text = render_thread(&store, &thread, &Source::Bot, &Identity::of(&store, &thread));
        assert!(text.starts_with(TASK), "{text}");
        for absent in ["Fixer", "Description:", "add_task", "Other conversations", "Messages:", "Other computers"] {
            assert!(!text.contains(absent), "{absent} in: {text}");
        }
        assert!(text.contains("Workspace: you work in /tmp."), "{text}");
        assert!(text.contains(crate::dev_servers::BRIEF), "it may start dev servers");
    }

    #[test]
    fn a_thread_the_user_opened_is_the_bot_away_from_its_chat() {
        let store = store();
        let thread = thread_of(&cfg("Fixer", "Fix bugs"));
        let text = render_thread(&store, &thread, &Source::User, &Identity::of(&store, &thread));
        assert!(text.contains("Title: Fixer") && text.contains("Description: Fix bugs"), "{text}");
        assert!(text.contains(USER), "{text}");
        for absent in ["add_task", "Other conversations", "Messages:", crate::routines::BRIEF] {
            assert!(!text.contains(absent), "{absent} in: {text}");
        }
    }

    #[test]
    fn a_routines_run_and_a_request_keep_the_routines_they_can_use() {
        let store = store();
        let thread = thread_of(&cfg("Fixer", "Fix bugs"));
        let identity = Identity::of(&store, &thread);
        let run = render_thread(&store, &thread, &Source::Routine { id: "r1".into(), run: "run-1".into() }, &identity);
        assert!(run.contains(crate::routines::BRIEF) && run.contains(OTHER_COMPUTERS), "{run}");
        let asked = render_thread(&store, &thread, &Source::Request { id: None }, &identity);
        assert!(asked.contains(crate::routines::BRIEF) && !asked.contains(OTHER_COMPUTERS), "{asked}");
        assert!(!run.contains("add_task") && !asked.contains("add_task"));
    }

    #[test]
    fn instructions_say_where_bot_requests_and_their_replies_go() {
        let store = store();
        let system = render(&store, &cfg("Reporter", "Report outcomes"));
        assert!(system.contains("requests from other bots, send_message isn't available"));
        assert!(system.contains("Replies to your own message_bot requests arrive as a turn of their own"));
        assert!(system.contains("These rules apply to the current turn"));
        assert!(system.contains("use list_bots and message_bot (ids like name@computer), not SSH"));
    }

    #[test]
    fn a_bot_is_told_its_projects_and_hears_when_they_change() {
        let store = store();
        let original = cfg("Fixer", "");
        let mut bot = original.clone();
        let alone = render(&store, &bot);
        assert!(
            alone.contains("Workspace: your own folder is /tmp") && alone.contains("You have no projects"),
            "{alone}"
        );
        bot.projects = vec!["/code/app".into(), "/code/site".into()];
        let system = render(&store, &bot);
        assert!(
            system.contains("Your projects, the user's folders: /code/app (your default); also /code/site."),
            "{system}"
        );
        assert!(system.contains("add_task folder") && system.contains("never change your own folder"), "{system}");
        let thread = thread_of(&original);
        let thread_text = render_thread(&store, &thread, &Source::User, &Identity::of(&store, &thread));
        assert!(!thread_text.contains("Your projects"));

        let identity = Identity::of(&store, &original);
        let first = adopt(&store, &original, "s1", "SYSTEM".into(), identity, String::new()).expect("adopt");
        let (text, _) = profile_update(&store, &first, &bot).expect("its projects changed");
        assert!(text.contains("/code/app (your default)"), "{text}");
        let renamed = thread_of(&cfg("Fixer 2", ""));
        let (text, _) = profile_update(&store, &first, &renamed).expect("its name changed");
        assert!(!text.contains("projects") && !text.contains("add_task"), "a thread runs no tasks: {text}");
    }

    #[test]
    fn instructions_place_the_user_in_codync_not_in_the_engine() {
        let system = render(&store(), &cfg("Reporter", ""));
        assert!(system.contains("not in Claude Code's own CLI, IDE or app"), "{system}");
        assert!(system.contains("The engine Codync runs you on is Claude Code:"), "{system}");
    }

    #[test]
    fn instructions_send_a_later_look_to_follow_ups_not_the_engines_timers() {
        let system = render(&store(), &cfg("Reporter", ""));
        assert!(system.contains("set a follow-up with follow_up"), "{system}");
        assert!(system.contains("Don't use your engine's own wake-ups, timers, loops, sleeps or cron"), "{system}");
    }

    #[test]
    fn snapshot_is_frozen_until_compaction_and_updates_are_announced_once() {
        let store = store();
        let a = cfg("Fixer", "Fix bugs");
        let first =
            adopt(&store, &a, "s1", "SYSTEM v1".into(), Identity::of(&store, &a), String::new()).expect("adopt");
        // Frozen: resolving again returns the same text, even though a live render differs.
        assert_eq!(resolve(&store, &a, "s1", true).expect("resolve").system, "SYSTEM v1");
        assert!(profile_update(&store, &first, &a).is_none());

        let b = cfg("Fixer", "Fix bugs and write tests");
        let (text, identity) = profile_update(&store, &first, &b).expect("changed");
        assert!(text.contains("Current description: Fix bugs and write tests"));
        mark_announced(&store, &a.id, &first, identity).expect("mark");
        let snap = resolve(&store, &b, "s1", true).expect("resolve");
        assert_eq!(snap.system, "SYSTEM v1");
        assert!(profile_update(&store, &snap, &b).is_none());

        // A compaction folds the new identity into a fresh render.
        assert!(bump_epoch(&store, &a.id, "s1", "c1").expect("bump"));
        assert!(!bump_epoch(&store, &a.id, "s1", "c1").expect("bump"));
        let folded = resolve(&store, &b, "s1", true).expect("resolve");
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
        let first =
            adopt(&store, &a, "s1", "SYSTEM v1".into(), Identity::of(&store, &a), String::new()).expect("adopt");
        assert!(profile_update(&store, &first, &a).is_none());
        skill("When shipping a release or a hotfix");
        let (text, _) = profile_update(&store, &first, &a).expect("the description changed");
        assert!(text.contains("- Release: When shipping a release or a hotfix"), "{text}");
    }

    #[test]
    fn a_memory_change_re_renders_but_keeps_what_the_agent_heard() {
        let store = store();
        let a = cfg("Fixer", "Fix bugs");
        adopt(&store, &a, "s1", "SYSTEM v1".into(), Identity::of(&store, &a), String::new()).expect("adopt");
        let b = cfg("Fixer", "Fix bugs and write tests");
        invalidate(&store, &a.id).expect("invalidate");
        let snap = resolve(&store, &b, "s1", true).expect("resolve");
        assert!(snap.system.contains("Description: Fix bugs and write tests"), "re-rendered");
        assert!(profile_update(&store, &snap, &b).is_some(), "the agent never heard the new description");
    }

    #[test]
    fn the_lead_role_is_announced_as_it_moves() {
        let store = store();
        let mut lead = cfg("Fixer", "Fix bugs");
        lead.lead = true;
        let (system, identity) = render_with_identity(&store, &lead);
        adopt(&store, &lead, "s1", system, identity, String::new()).expect("adopt");
        // A snapshot from before the lead could move: the role was in its instructions alone.
        let k = key(&lead.id, "s1");
        let mut old: serde_json::Value = serde_json::from_str(&store.kv_get(&k).expect("saved")).expect("json");
        for identity in ["systemIdentity", "announced"] {
            old[identity].as_object_mut().expect("an identity").remove("lead");
        }
        store.kv_set(&k, &old.to_string()).expect("save");
        let snap = resolve(&store, &lead, "s1", true).expect("resolve");
        assert!(profile_update(&store, &snap, &lead).is_none(), "it heard its role in its instructions");

        let mut other = lead.clone();
        other.lead = false;
        let (text, identity) = profile_update(&store, &snap, &other).expect("the role moved");
        assert!(text.contains(NOT_LEAD) && !text.contains(crate::chat::lead::BRIEF), "{text}");
        mark_announced(&store, &lead.id, &snap, identity).expect("mark");
        let snap = resolve(&store, &other, "s1", true).expect("resolve");
        assert!(profile_update(&store, &snap, &other).is_none(), "told once");
        let (text, _) = profile_update(&store, &snap, &lead).expect("it leads again");
        assert!(text.contains(crate::chat::lead::BRIEF), "{text}");
    }

    #[test]
    fn a_new_sessions_instructions_stay_owed_until_a_prompt_carried_them() {
        let store = store();
        let a = cfg("Fixer", "Fix bugs");
        let first =
            adopt(&store, &a, "s1", "SYSTEM v1".into(), Identity::of(&store, &a), String::new()).expect("adopt");
        assert!(resolve(&store, &a, "s1", true).expect("resolve").unprompted, "survives a restart");
        mark_prompted(&store, &a.id, &first).expect("mark");
        assert!(!resolve(&store, &a, "s1", true).expect("resolve").unprompted);
    }

    #[test]
    fn a_missing_snapshot_after_compaction_only_owes_an_opening_in_the_prompt() {
        let store = store();
        let bot = cfg("Fixer", "Fix bugs");
        for (sid, opening_in_prompt) in [("first-message", true), ("system-prompt", false)] {
            bump_epoch(&store, &bot.id, sid, "c1").expect("compaction");
            let snapshot = resolve(&store, &bot, sid, opening_in_prompt).expect("rebuild missing snapshot");
            assert_eq!(snapshot.unprompted, opening_in_prompt);
        }
    }

    #[test]
    fn a_late_announcement_does_not_settle_a_newer_compaction() {
        let store = store();
        let bot = cfg("Fixer", "Fix bugs");
        let sid = "s1";
        let first = resolve(&store, &bot, sid, true).expect("initial snapshot");
        assert!(!first.needs_resend());

        bump_epoch(&store, &bot.id, sid, "c1").expect("first compaction");
        let sent = resolve(&store, &bot, sid, true).expect("sent snapshot");
        assert!(sent.needs_resend());

        bump_epoch(&store, &bot.id, sid, "c2").expect("second compaction");
        let latest = resolve(&store, &bot, sid, true).expect("latest snapshot");
        mark_announced(&store, &bot.id, &sent, sent.system_identity.clone()).expect("late acknowledgement");
        assert!(resolve(&store, &bot, sid, true).expect("resolve after late acknowledgement").needs_resend());

        mark_announced(&store, &bot.id, &latest, latest.system_identity.clone()).expect("current acknowledgement");
        assert!(!resolve(&store, &bot, sid, true).expect("resolve after current acknowledgement").needs_resend());
    }

    #[test]
    fn a_thread_profile_reads_its_owners_current_skills() {
        let store = store();
        let mut owner = cfg("Fixer", "Fix bugs");
        store.save_bot(&owner).expect("save owner");
        let mut thread = owner.clone();
        thread.id = "thread-1".into();
        thread.thread_of = Some(owner.id.clone());
        let (system, identity) = render_with_identity(&store, &thread.as_owner());
        let snap = adopt(&store, &thread, "s-thread", system, identity, String::new()).expect("adopt");
        assert!(profile_update(&store, &snap, &thread.as_owner()).is_none());

        let list = serde_json::json!([{"id": "deploy", "name": "Deploy", "description": "Ship releases"}]);
        store.kv_set("skills", &list.to_string()).expect("save skills");
        owner.skills = vec!["deploy".into()];
        store.save_bot(&owner).expect("save owner with skill");

        let (update, _) = profile_update(&store, &snap, &thread.as_owner()).expect("thread hears parent's new skill");
        assert!(update.contains("- Deploy: Ship releases"), "{update}");
        assert!(render(&store, &thread.as_owner()).contains("- Deploy: Ship releases"));
    }

    #[test]
    fn a_skill_saved_while_opening_a_session_is_still_announced_after_adopt() {
        let store = store();
        let mut owner = cfg("Fixer", "Fix bugs");
        store.save_bot(&owner).expect("save owner");

        // Rendered before `session/new` (no skills yet); another thread saves a skill before `adopt` runs.
        let (system, identity) = render_with_identity(&store, &owner);
        assert!(!system.contains("Deploy"));
        let list = serde_json::json!([{"id": "deploy", "name": "Deploy", "description": "Ship releases"}]);
        store.kv_set("skills", &list.to_string()).expect("save skills");
        owner.skills = vec!["deploy".into()];
        store.save_bot(&owner).expect("save owner with skill");

        let snap = adopt(&store, &owner, "s-new", system, identity, String::new()).expect("adopt");
        let (update, _) = profile_update(&store, &snap, &owner).expect("new skill must still be announced");
        assert!(update.contains("- Deploy: Ship releases"), "{update}");
    }
}
