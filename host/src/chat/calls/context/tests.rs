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
    let data = json!({"text": "Release plan", "callSeconds": 30, "transcript": [{"from": "user", "text": "ship it"}]});
    let call = hub.store.insert_entry(&Lane::main(&cfg.id), EntryKind::Notice, 1, &data).unwrap();
    let recent = hub.store.history(&cfg.id, i64::MAX, RECENT).unwrap();
    let local = chrono::DateTime::from_timestamp_millis(call.created_at).unwrap().with_timezone(&chrono::Local);
    let section = conversation(&hub, &cfg, &recent);
    assert!(
        section.contains(&format!("- Release plan ({}): \"ship it\"", local.format("%Y-%m-%d %H:%M"))),
        "{section}"
    );
}
