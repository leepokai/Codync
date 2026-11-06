//! What the voice of a call hears before it speaks: its rules, then the background as named sections, each capped,
//! empty ones left out, each saying where it came from. It goes to the voice provider, on the user's own key.

use super::speech::{call_so_far, spoken};
use super::tools::{runtime, waiting};
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, EntryKind, Lane};
use crate::voice::Provider;
use codync_mem::files::{self as mf, MAIN};
use std::path::Path;
use std::sync::Arc;

/// Each section's cap (characters) and the whole background's: about 16k on Gemini, and up to 3k more of the call so
/// far for a session that replaces another. `OpenAI` Realtime keeps a smaller context, so it gets [`OPENAI_TENTHS`]
/// tenths of each.
const CAPS: Caps = Caps {
    now: 300,
    user: 3_000,
    bot: 2_000,
    remembers: 3_000,
    project: 1_000,
    work: 1_500,
    conversation: 3_500,
    others: 1_000,
    shared: 700,
    so_far: 3_000,
};
/// Of each cap, in tenths.
const OPENAI_TENTHS: usize = 6;
/// The chat messages the voice reads, and how much of each.
const MESSAGES: i64 = 20;
const MESSAGE_CHARS: usize = 600;
/// Entries looked through for the bot's current work, past calls and shared files.
pub(super) const RECENT: i64 = 300;

struct Caps {
    now: usize,
    user: usize,
    bot: usize,
    remembers: usize,
    project: usize,
    work: usize,
    conversation: usize,
    others: usize,
    shared: usize,
    so_far: usize,
}

/// The voice's role first: a relay between the user and the bot (or group), never the bot itself.
fn rules(hub: &Hub, cfg: &BotConfig) -> String {
    let name = if cfg.name.trim().is_empty() { "the bot" } else { cfg.name.trim() };
    let who = if cfg.is_group() {
        let members: Vec<String> = cfg.members.iter().map(|id| hub.bot_name(id)).collect();
        format!(
            "{name}, a group chat of coding agents on the user's computer ({}), who answer in turn",
            members.join(", ")
        )
    } else {
        format!("{name}, a coding agent on the user's computer")
    };
    format!(
        "You are the voice relay for {name}: the switchboard operator on a phone call between the user and {who}. You \
are not {name}. {name} does all the actual work.
- Your job: pass the user's requests to {name} with send_to_bot, tell the user what {name} says, and ask the user \
for anything missing. Small talk, clarifying questions and reading results aloud are yours.
- When asked who you are, say you're the voice line to {name}. Speak of {name} in the third person (\"I'll ask {name}\", \
\"{name} says …\"); never claim you did, checked or saw something yourself.
- Anything about the user's code, files, computer or project goes to {name} with send_to_bot, even if you think you \
know the answer, in the user's own words (keep names and code terms exactly). The host attaches what the user said \
word for word. Never do the work or invent results.
- bot_status and recent_messages answer \"what is {name} doing?\" or \"what did it say?\"; search_memory finds what \
{name} remembers; stop_bot stops {name} when the user asks.
- Call answer_approval only after the user has clearly said which option to pick.
- Messages starting with [{name} replied] or [Notice] come from the computer: tell the user what they say, shortened \
for speech (no code blocks, no long lists, no URLs).
- Messages starting with [Progress] come from the computer while {name} works: the latest one answers \"what is \
{name} doing?\"; mention it briefly if the user is waiting, never make up progress or results.
- Answer in the language the user is speaking right now, the language of their last words, and switch when they \
switch, whatever the phone's language, the background or {name}'s replies are in. For Chinese keep their variety and \
script: Taiwanese Mandarin gets Traditional characters and Taiwan wording, Mainland Mandarin gets Simplified, \
Cantonese gets Cantonese. Read out what {name} says in the user's language too. Keep spoken answers short and natural.
- The background below is for context: use it, don't read it out."
    )
}

/// The voice's instructions for a call with `cfg`'s bot; `so_far`: what was said on the call before this provider
/// session (one that replaces a dropped session picks up from it).
pub async fn instructions(
    hub: &Arc<Hub>,
    cfg: &BotConfig,
    provider: Option<Provider>,
    language: Option<&str>,
    so_far: &[super::CallLine],
) -> String {
    let tenths = if provider == Some(Provider::OpenAi) { OPENAI_TENTHS } else { 10 };
    let cap = |n: usize| n * tenths / 10;
    let project = super::project::now(&cfg.cwd).await;
    let (h, c, l) = (hub.clone(), cfg.clone(), language.map(str::to_owned));
    let sections = tokio::task::spawn_blocking(move || background(&h, &c, l.as_deref())).await.unwrap_or_default();
    let mut out = vec![rules(hub, cfg)];
    let caps = &CAPS;
    let named = [
        ("Now", "this computer's clock and the user's phone", sections.now, caps.now),
        ("About the user", "the memory the user's bots share", sections.user, caps.user),
        ("This bot", "its settings in Codync", sections.bot, caps.bot),
        ("What the bot remembers", "its own memory", sections.remembers, caps.remembers),
        ("The project now", "git, in the bot's folder", project, caps.project),
        ("Current work", "the bot's status and its chat", sections.work, caps.work),
        ("The conversation", "the bot's chat and earlier calls", sections.conversation, caps.conversation),
        ("Other bots on this computer", "Codync", sections.others, caps.others),
        ("Shared in this chat", "files and artifacts in the bot's chat", sections.shared, caps.shared),
        ("This call so far", "this call's transcript, newest last", call_so_far(so_far, cap(caps.so_far)), caps.so_far),
    ];
    for (title, from, body, n) in named {
        let body = body.trim();
        if !body.is_empty() {
            out.push(format!("## {title} (from {from})\n{}", crate::agent::acp::truncate(body, cap(n))));
        }
    }
    out.join("\n\n")
}

#[derive(Default)]
struct Sections {
    now: String,
    user: String,
    bot: String,
    remembers: String,
    work: String,
    conversation: String,
    others: String,
    shared: String,
}

fn background(hub: &Hub, cfg: &BotConfig, language: Option<&str>) -> Sections {
    let recent = hub.store.history(&cfg.id, i64::MAX, RECENT).unwrap_or_default();
    Sections {
        now: now(language),
        user: user(hub, cfg),
        bot: bot(hub, cfg),
        remembers: remembers(hub, cfg),
        work: work(hub, cfg, &recent),
        conversation: conversation(hub, cfg, &recent),
        others: others(hub, cfg),
        shared: shared(&recent),
    }
}

fn now(language: Option<&str>) -> String {
    let now = chrono::Local::now();
    let mut lines = vec![format!("{} (UTC{})", now.format("%A %Y-%m-%d %H:%M"), now.format("%:z"))];
    if let Some(language) = language.filter(|l| !l.trim().is_empty()) {
        lines.push(format!(
            "The phone's language setting: {} (only for speaking before the user has; then follow the language they speak)",
            crate::agent::acp::truncate(language.trim(), 40)
        ));
    }
    lines.join("\n")
}

/// A layer's `MEMORY.md` and its notes' names and descriptions (a description the checks refuse stays out).
fn layer(hub: &Hub, bot: Option<&str>, owner: &str, shared: bool) -> String {
    let main = hub.store.memory_read(|c| mf::current(c, bot, MAIN)).ok().flatten().unwrap_or_default();
    let index = hub.store.memory_read(|c| mf::index(c, owner, shared)).ok();
    let notes = index.map(|i| if bot.is_some() { i.bot } else { i.yours.unwrap_or_default() }).unwrap_or_default();
    let mut out = main.trim().to_owned();
    let listed: Vec<String> = notes
        .iter()
        .map(|f| {
            let about = f.description.as_deref().filter(|d| codync_mem::check::content(d).is_ok()).unwrap_or("");
            if about.is_empty() { format!("- {}", f.name) } else { format!("- {}: {about}", f.name) }
        })
        .collect();
    if !listed.is_empty() {
        out = format!("{out}\nNotes (ask the bot to read one):\n{}", listed.join("\n"));
    }
    out
}

fn user(hub: &Hub, cfg: &BotConfig) -> String {
    if cfg.is_group() || !crate::chat::memory::shared(cfg) {
        return String::new();
    }
    layer(hub, None, &cfg.id, true)
}

fn remembers(hub: &Hub, cfg: &BotConfig) -> String {
    if cfg.is_group() {
        return String::new();
    }
    layer(hub, Some(&cfg.id), &cfg.id, false)
}

fn bot(hub: &Hub, cfg: &BotConfig) -> String {
    let mut lines = vec![format!("Name: {}", cfg.name)];
    if !cfg.description.trim().is_empty() {
        lines.push(format!("Description: {}", cfg.description.trim()));
    }
    if cfg.is_group() {
        lines.push("A group chat; its members answer in turn:".into());
        for id in &cfg.members {
            if let Ok(Some(m)) = hub.store.bot(id) {
                lines.push(format!("- {}: {}", m.config.name, m.config.description.trim()));
            }
        }
        return lines.join("\n");
    }
    let model = cfg.model.as_deref().map(|m| format!(", model {m}")).unwrap_or_default();
    lines.push(format!("Runs on: {}{model}", crate::agent::fallback::name(&cfg.backend)));
    let project = Path::new(&cfg.cwd).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    lines.push(format!("Folder: {} (project {project})", cfg.cwd));
    lines.push(format!(
        "Permission mode: {}",
        serde_json::to_value(cfg.permission).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
    ));
    if !cfg.connectors.is_empty() {
        lines.push(format!("Connectors: {}", cfg.connectors.join(", ")));
    }
    if !cfg.skills.is_empty() {
        lines.push(format!("Skills: {}", cfg.skills.join(", ")));
    }
    lines.push(format!("Computer use: {}", if cfg.computer { "on" } else { "off" }));
    let routines = hub.routines.list(&cfg.id);
    for r in routines["routines"].as_array().into_iter().flatten() {
        let when: Vec<&str> =
            r["triggerDescriptions"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).collect();
        let off = if r["enabled"] == true { "" } else { " (off)" };
        lines.push(format!("Routine: {} — {}{off}", r["name"].as_str().unwrap_or_default(), when.join("; ")));
    }
    lines.join("\n")
}

fn work(hub: &Hub, cfg: &BotConfig, recent: &[Entry]) -> String {
    let rt = runtime(hub, cfg);
    let status = match rt.status {
        BotStatus::Idle => "idle",
        BotStatus::Working => "working",
        BotStatus::NeedsInput => "waiting for the user",
        BotStatus::Error => "stopped on an error",
    };
    let mut lines = vec![format!("Status: {status}")];
    if !rt.activity.trim().is_empty() {
        lines.push(format!("Doing: {}", rt.activity.trim()));
    }
    if let Some(w) = waiting(recent) {
        lines.push(format!("Waiting for approval: {} (options: {})", w.request, w.options.join(", ")));
    }
    let queued = recent.iter().filter(|e| e.kind == EntryKind::User.as_str() && e.data["status"] == "queued").count();
    if queued > 0 {
        lines.push(format!("Messages waiting for it: {queued}"));
    }
    if let Some(failure) =
        recent.iter().rev().find(|e| e.kind == EntryKind::Notice.as_str() && e.data["style"] == "error")
    {
        lines.push(format!("Last failure: {}", spoken(failure.data["text"].as_str().unwrap_or_default(), 300)));
    }
    lines.join("\n")
}

/// The chat's last `count` messages as the voice reads them: who wrote each, and its text shortened for speech.
pub(super) fn said(hub: &Hub, cfg: &BotConfig, count: i64, chars: usize) -> Vec<(String, String)> {
    let messages = hub.store.messages_after(&Lane::main(&cfg.id), 0, count).unwrap_or_default();
    messages
        .iter()
        .map(|e| {
            let who = super::writer(e)
                .map_or_else(|| hub.bot_name(e.data["author"].as_str().unwrap_or(&cfg.id)), str::to_owned);
            (who, spoken(e.data["text"].as_str().unwrap_or_default(), chars))
        })
        .collect()
}

fn conversation(hub: &Hub, cfg: &BotConfig, recent: &[Entry]) -> String {
    let mut lines: Vec<String> =
        said(hub, cfg, MESSAGES, MESSAGE_CHARS).into_iter().map(|(who, text)| format!("{who}: {text}")).collect();
    let calls: Vec<String> = recent
        .iter()
        .rev()
        .filter(|e| e.kind == EntryKind::Notice.as_str() && e.data["callSeconds"].is_number())
        .take(3)
        .map(|e| {
            let gist = e.data["transcript"].as_array().into_iter().flatten().find(|l| l["from"] == "user");
            let gist =
                gist.and_then(|l| l["text"].as_str()).map(|t| format!(": \"{}\"", spoken(t, 120))).unwrap_or_default();
            format!("- {} ({}){gist}", e.data["text"].as_str().unwrap_or(super::UNTITLED), local_time(e.created_at))
        })
        .collect();
    if !calls.is_empty() {
        lines.push(format!("Earlier calls with this bot:\n{}", calls.join("\n")));
    }
    lines.join("\n")
}

/// When something happened, on this computer's clock (as "Now" reads).
fn local_time(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map_or_else(|| "unknown date".into(), |t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
}

fn others(hub: &Hub, cfg: &BotConfig) -> String {
    let rows = hub.store.bots().unwrap_or_default();
    rows.iter()
        .filter(|r| !r.deleted && r.config.id != cfg.id)
        .map(|r| {
            let about = r.config.description.trim();
            if about.is_empty() { format!("- {}", r.config.name) } else { format!("- {}: {about}", r.config.name) }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn shared(recent: &[Entry]) -> String {
    let mut titles: Vec<String> = Vec::new();
    for e in recent.iter().rev() {
        if let Some(title) = e.data["artifact"]["title"].as_str() {
            titles.push(format!("- {title} (artifact)"));
        }
        for f in e.data["files"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(e.data["attachments"].as_array().into_iter().flatten())
        {
            if let Some(name) = f["name"].as_str() {
                titles.push(format!("- {name}"));
            }
        }
        if titles.len() >= 10 {
            break;
        }
    }
    titles.join("\n")
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::store::{BotKind, Permission, Store};
    use codync_mem::files::{Author, Cause, Change};
    use serde_json::json;

    pub(in crate::chat::calls) fn hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-call-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(Path::new(":memory:")).unwrap();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        Hub::new(store, "h".into(), identity, "t".into(), 0)
    }

    pub(in crate::chat::calls) fn bot(id: &str, name: &str, cwd: &str) -> BotConfig {
        BotConfig {
            id: id.into(),
            kind: BotKind::Agent,
            members: vec![],
            name: name.into(),
            description: "Keeps the shop's site running".into(),
            avatar_color: "blue".into(),
            avatar_shape: "blob".into(),
            backend: "claude".into(),
            command: None,
            cwd: cwd.into(),
            permission: Permission::Ask,
            model: Some("opus".into()),
            model_options: std::collections::BTreeMap::new(),
            fast_mode: false,
            fallbacks: vec![],
            self_fallbacks: true,
            pinned: false,
            position: 0,
            hidden: false,
            section: None,
            notify: None,
            connectors: vec!["github".into()],
            skills: vec![],
            computer: false,
            manage_team: false,
            offered_to_network: true,
            reachable_by_assistants: true,
            created_by: None,
            shared_memory: Some(true),
            harness_memory: false,
            auto_name: false,
            lead: false,
            created_at: 0,
        }
    }

    /// `MEMORY.md` straight into the store, as a scan would record it.
    fn remember(hub: &Hub, layer: Option<&str>, content: &str) {
        let change = Change {
            bot: layer,
            name: MAIN,
            content: Some(content),
            author: Author::User,
            author_bot: None,
            cause: Cause::Edit,
            message: None,
            mtime: 0,
            size: content.len() as u64,
        };
        hub.store.memory_write(|c, now, rev| mf::record(c, &change, now, rev)).unwrap();
    }

    /// git in `dir` alone: a pre-commit hook running these tests sets `GIT_DIR` and `GIT_INDEX_FILE` for the commit.
    fn git(dir: &Path, args: &[&str]) -> bool {
        let mut cmd = std::process::Command::new("git");
        for (key, _) in std::env::vars_os().filter(|(k, _)| k.to_string_lossy().starts_with("GIT_")) {
            cmd.env_remove(key);
        }
        cmd.args(args).current_dir(dir).output().is_ok_and(|o| o.status.success())
    }

    #[tokio::test]
    async fn the_voice_hears_its_role_then_the_background_within_caps() {
        let hub = hub();
        let repo = std::env::temp_dir().join(format!("codync-call-repo-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("cart.ts"), "x").unwrap();
        let repo_ok = git(&repo, &["init", "-q", "-b", "main"])
            && git(&repo, &["add", "cart.ts"])
            && git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "Add the cart"]);
        // Unstaged, so its porcelain line starts with a space.
        std::fs::write(repo.join("cart.ts"), "y").unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let cfg = bot(&id, "Mika", &repo.to_string_lossy());
        hub.store.save_bot(&cfg).unwrap();
        hub.store.save_bot(&bot(&uuid::Uuid::new_v4().to_string(), "Owen", "/tmp")).unwrap();
        remember(&hub, Some(&id), "- The shop deploys from main\n");
        remember(&hub, None, "- Lives in Taipei\n");
        let chat = Lane::main(&id);
        for turn in 0..30 {
            let long = format!("message {turn} {}", "x".repeat(2_000));
            hub.store.insert_entry(&chat, EntryKind::User, turn, &json!({"text": long})).unwrap();
            hub.store
                .insert_entry(&chat, EntryKind::Agent, turn, &json!({"text": "Done.", "final": true, "author": id}))
                .unwrap();
        }

        let gemini = instructions(&hub, &cfg, Some(Provider::Gemini), Some("zh-TW"), &[]).await;
        assert!(gemini.starts_with("You are the voice relay for Mika"), "the role comes first: {}", &gemini[..80]);
        assert!(gemini.contains("You are not Mika") && gemini.contains("send_to_bot"));
        assert!(
            gemini.contains("the language the user is speaking right now")
                && gemini.contains("follow the language they speak"),
            "the voice answers in the language spoken, the phone's setting is only a fallback"
        );
        for heading in [
            "## Now",
            "## About the user",
            "## This bot",
            "## What the bot remembers",
            "## The conversation",
            "## Other bots",
        ] {
            assert!(gemini.contains(heading), "{heading} is there");
        }
        for fact in ["Lives in Taipei", "The shop deploys from main", "zh-TW", "github", "Owen"] {
            assert!(gemini.contains(fact), "{fact} reaches the voice");
        }
        if repo_ok {
            assert!(
                gemini.contains("## The project now")
                    && gemini.contains("Branch: main")
                    && gemini.contains("Changed files (1): cart.ts")
            );
            assert!(gemini.contains("Add the cart"));
        }
        assert!(!gemini.contains("## Shared in this chat"), "empty sections stay out");
        let conversation = gemini.split("## The conversation").nth(1).unwrap().split("\n## ").next().unwrap();
        assert!(conversation.chars().count() <= CAPS.conversation + 80, "the conversation keeps to its cap");
        let background = gemini.chars().count() - rules(&hub, &cfg).chars().count();
        assert!(background <= 16_000 + 1_000, "the background keeps to about 16k: {background}");
        let openai = instructions(&hub, &cfg, Some(Provider::OpenAi), None, &[]).await;
        assert!(openai.chars().count() < gemini.chars().count(), "OpenAI's smaller context gets less");
    }

    #[test]
    fn earlier_calls_are_dated_on_this_computers_clock() {
        let hub = hub();
        let cfg = bot("b-clock", "Mika", "/tmp");
        hub.store.save_bot(&cfg).unwrap();
        let data =
            json!({"text": "Release plan", "callSeconds": 30, "transcript": [{"from": "user", "text": "ship it"}]});
        let call = hub.store.insert_entry(&Lane::main(&cfg.id), EntryKind::Notice, 1, &data).unwrap();
        let recent = hub.store.history(&cfg.id, i64::MAX, RECENT).unwrap();
        let local = chrono::DateTime::from_timestamp_millis(call.created_at).unwrap().with_timezone(&chrono::Local);
        let section = conversation(&hub, &cfg, &recent);
        assert!(
            section.contains(&format!("- Release plan ({}): \"ship it\"", local.format("%Y-%m-%d %H:%M"))),
            "{section}"
        );
    }
}
