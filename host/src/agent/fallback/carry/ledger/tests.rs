use super::*;
use serde_json::json;

fn entry(seq: i64, kind: &str, data: Value) -> Entry {
    Entry {
        id: format!("e{seq}"),
        seq,
        bot_id: "b".into(),
        rev: seq,
        kind: kind.into(),
        turn: seq,
        data,
        created_at: 1_790_000_000_000,
        updated_at: 0,
    }
}

fn scratch(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("codync-ledger-{}-{name}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("file.txt");
    std::fs::write(&file, body).unwrap();
    file
}

#[test]
fn the_users_rules_are_quoted_and_their_orders_are_not_kept_as_facts() {
    let entries = [
        entry(1, EntryKind::User.as_str(), json!({"text": "Check the build"})),
        entry(2, EntryKind::User.as_str(), json!({"text": "不要改 src/auth.rs，只动测试"})),
        entry(3, EntryKind::Agent.as_str(), json!({"text": "Never touch the lockfile", "final": true})),
        entry(4, EntryKind::User.as_str(), json!({"text": "Always run cargo fmt first"})),
    ];
    let rules = constraints(&entries);
    assert_eq!(rules.len(), 2, "the bot's own words and a plain order are no rules: {rules:?}");
    assert!(rules[0].contains("Always run cargo fmt first"), "newest first: {rules:?}");
    assert!(rules[1].contains("不要改 src/auth.rs"), "verbatim, in the user's own words: {rules:?}");
}

#[test]
fn paths_addresses_and_ids_the_user_named_are_kept_once_in_order() {
    let entries = [
        entry(
            1,
            EntryKind::User.as_str(),
            json!({"text": "请看一下 https://github.com/leepokai/Codync/pull/415 和 src/auth.rs，还有 src/auth.rs"}),
        ),
        entry(2, EntryKind::User.as_str(), json!({"text": "再看 docs/features/agent-fallback.md"})),
    ];
    let found = references(&entries);
    assert_eq!(
        found,
        ["docs/features/agent-fallback.md", "https://github.com/leepokai/Codync/pull/415", "src/auth.rs"],
        "newest message first, each kept once"
    );
}

#[test]
fn files_keep_their_reason_size_and_hash() {
    let changed = scratch("changed", "hello");
    let created = scratch("created", "");
    let entries = [
        entry(
            1,
            EntryKind::Tool.as_str(),
            json!({"title": "Read notes/plan.md", "toolKind": "read", "locations": [{"path": "notes/plan.md"}]}),
        ),
        entry(
            2,
            EntryKind::Tool.as_str(),
            json!({"title": "Edit a.rs", "toolKind": "edit", "diffs": [
                {"path": changed.to_string_lossy(), "isNew": false, "patch": "+x"}
            ]}),
        ),
        entry(
            3,
            EntryKind::Tool.as_str(),
            json!({"title": "Write b.rs", "toolKind": "edit", "diffs": [
                {"path": created.to_string_lossy(), "isNew": true, "patch": "+y"}
            ]}),
        ),
    ];
    let files = files(&entries);
    let reasons: Vec<(&str, &str)> = files.iter().map(|f| (f.why, f.path.as_str())).collect();
    assert_eq!(reasons[0].0, "created", "{files:?}");
    assert_eq!(reasons[1].0, "modified");
    assert_eq!(reasons[2].0, "read");
    let changed_entry = &files[1];
    let bytes = std::fs::read(&changed).unwrap();
    assert_eq!(changed_entry.bytes, Some(bytes.len() as u64));
    let hash = changed_entry.sha256.clone().expect("a hash");
    assert_eq!(hash.len(), 64, "sha256 in hex: {hash}");
    assert_eq!(files[2].bytes, None, "a file it never saw is unknown, not guessed");
    assert!(describe_file(changed_entry).contains("B, sha256 "));
}

#[test]
fn commands_keep_the_exit_status_their_output_ends_with() {
    let entries = [
        entry(
            1,
            EntryKind::Tool.as_str(),
            json!({"title": "cargo test", "toolKind": "execute", "output": "ok\n\n[exit code: 1]"}),
        ),
        entry(2, EntryKind::Tool.as_str(), json!({"title": "ls", "toolKind": "execute", "output": "a.txt"})),
        entry(3, EntryKind::Tool.as_str(), json!({"title": "cat a.txt", "toolKind": "read"})),
        entry(
            4,
            EntryKind::Tool.as_str(),
            json!({"title": "npm test", "toolKind": "terminal", "output": "Process exited with code 2"}),
        ),
    ];
    let commands = commands(&entries);
    assert_eq!(commands.len(), 3, "the shell calls; the read isn't one: {commands:?}");
    assert_eq!(commands[0].status, Some(2), "newest first");
    assert_eq!(commands[2].status, Some(1));
    assert_eq!(commands[1].status, None, "a command whose output says nothing of its status");
    assert_eq!(exit_status("ok"), None);
    assert_eq!(exit_status("exit status 0"), Some(0));
}

#[test]
fn what_waits_for_the_user_is_named() {
    let entries = [
        entry(1, EntryKind::Permission.as_str(), json!({"title": "Run npm test", "status": "pending"})),
        entry(2, EntryKind::Permission.as_str(), json!({"title": "Edit a.rs", "status": "answered"})),
    ];
    assert_eq!(waiting(&entries), ["approval: Run npm test"]);
}

#[test]
fn the_ledger_reads_as_sections_and_names_what_the_budget_left_out() {
    let file = scratch("budget", "0123456789");
    let ledger = Ledger {
        constraints: vec!["always run the tests".into()],
        references: vec!["src/auth.rs".into()],
        files: vec![Touched {
            path: file.to_string_lossy().into_owned(),
            bytes: Some(10),
            sha256: Some("a".repeat(64)),
            why: "modified",
        }],
        commands: vec![Command { line: "cargo test".into(), status: Some(1) }],
        running: vec!["dev server web on port 5173".into()],
        waiting: vec!["approval: Run npm test".into()],
    };
    let full = render(&ledger, 6_000);
    for piece in [
        "<ledger>",
        "always run the tests",
        "src/auth.rs",
        "(modified, 10 B, sha256 aaaaaaaaaaaa)",
        "`cargo test` (exit 1)",
        "dev server web on port 5173",
        "approval: Run npm test",
    ] {
        assert!(full.contains(piece), "{piece} is in the ledger: {full}");
    }
    assert!(!full.contains("Omitted due to budget"), "a generous budget drops nothing");
    // A tight one keeps what fits and names the rest: nothing is dropped quietly.
    let tight = render(&ledger, 420);
    assert!(tight.contains("Omitted due to budget:"), "{tight}");
    assert!(tight.contains("Omitted due to budget:"), "nothing is dropped quietly: {tight}");
    assert!(tight.contains("Omitted due to budget:"), "{tight}");
    assert!(tight.chars().count() < 2_000, "the tight ledger stays small: {}", tight.chars().count());
}

#[test]
fn an_empty_ledger_adds_nothing() {
    assert_eq!(render(&Ledger::default(), 1_000), "");
}
