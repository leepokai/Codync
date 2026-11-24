use super::*;
use serde_json::json;

struct Temp(std::path::PathBuf);

impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("codync-ledger-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // Only the unique directory this helper created; cleanup must not hide a test failure.
        if self.0.parent() == Some(std::env::temp_dir().as_path()) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn entry(seq: i64, turn: i64, kind: EntryKind, data: Value) -> Entry {
    Entry {
        id: format!("e{seq}"),
        seq,
        bot_id: "b".into(),
        rev: seq,
        kind: kind.as_str().into(),
        turn,
        data,
        created_at: 0,
        updated_at: 0,
    }
}

fn read(seq: i64, turn: i64, path: &str) -> Entry {
    entry(
        seq,
        turn,
        EntryKind::Tool,
        json!({"title": format!("Read {path}"), "toolKind": "read", "status": "completed", "locations": [{"path": path}]}),
    )
}

#[test]
fn reset_excludes_old_rules_references_files_and_commands() {
    let entries = vec![
        entry(1, 1, EntryKind::User, json!({"text": "Never change old/secret.txt"})),
        read(2, 1, "old/secret.txt"),
        entry(3, 1, EntryKind::Tool, json!({"title":"old-command", "toolKind":"execute"})),
        entry(4, 2, EntryKind::User, json!({"text": "Only change new/current.txt"})),
        read(5, 2, "new/current.txt"),
        entry(6, 3, EntryKind::User, json!({"text": "Continue"})),
    ];
    let (history, work) = scoped(entries, 3, &["e6".into()]);
    let mut omitted = Vec::new();
    assert_eq!(constraints(&history, &mut omitted), ["Only change new/current.txt"]);
    assert_eq!(references(&history, &mut omitted), ["new/current.txt"]);
    assert_eq!(commands(&work, &mut omitted).len(), 0);
    assert!(work.iter().all(|e| e.turn == 2));
}

#[test]
fn a_turn_without_tools_does_not_claim_an_older_turns_files() {
    let entries = vec![
        read(1, 1, "old.txt"),
        entry(2, 2, EntryKind::Agent, json!({"text":"answered","final":true})),
        entry(3, 3, EntryKind::User, json!({"text":"continue"})),
    ];
    let (_, work) = scoped(entries, 0, &["e3".into()]);
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].turn, 2);
}

#[test]
fn a_queued_followup_does_not_hide_the_current_turns_work() {
    let entries = vec![
        entry(1, 1, EntryKind::User, json!({"text": "Fix tests", "status": "sent"})),
        read(2, 1, "src/lib.rs"),
        entry(3, 1, EntryKind::Tool, json!({"title": "cargo test", "toolKind": "execute", "status": "failed"})),
        entry(4, 2, EntryKind::User, json!({"text": "Also check logs", "status": "queued"})),
    ];
    let (_, work) = scoped(entries, 0, &[]);
    assert!(!work.is_empty() && work.iter().all(|e| e.turn == 1), "queued follow-up turn must not win: {work:?}");
    assert_eq!(commands(&work, &mut Vec::new()).len(), 1);
    let dir = Temp::new();
    assert_eq!(files::collect(&work, dir.path(), &[], &mut Vec::new()).len(), 1);
}

#[test]
fn read_and_search_outputs_mentioning_exit_codes_are_not_commands() {
    let tool = |seq, data| entry(seq, 1, EntryKind::Tool, data);
    let entries = [
        tool(
            1,
            json!({
                "title": "Read app.log", "toolKind": "read", "status": "completed", "output": "last line: exit code: 1"
            }),
        ),
        tool(
            2,
            json!({
                "title": "Search exit status",
                "toolKind": "search",
                "status": "completed",
                "output": "Process exited with code 2",
            }),
        ),
        tool(
            3,
            json!({
                "title": "Inspect output", "toolKind": "other", "status": "completed", "output": "exit status 1"
            }),
        ),
        tool(4, json!({"title": "cargo check", "toolKind": "execute", "status": "failed", "output": "exit code: 3"})),
        tool(
            5,
            json!({
                "title": "npm test",
                "toolKind": "other",
                "command": "npm test",
                "status": "failed",
                "output": "exit code: 4",
            }),
        ),
    ];
    let found = commands(&entries, &mut Vec::new());
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].line, "npm test");
    assert_eq!(found[0].status, Some(4));
    assert_eq!(found[1].line, "cargo check");
    assert_eq!(found[1].status, Some(3));
}

#[test]
fn only_the_user_supplies_constraints_and_references() {
    let entries = [
        entry(1, 1, EntryKind::User, json!({"text": "不要改 src/a.rs https://example.com #409"})),
        entry(2, 1, EntryKind::Agent, json!({"text": "Never read agent/guess.txt"})),
    ];
    let mut omitted = Vec::new();
    assert_eq!(constraints(&entries, &mut omitted).len(), 1);
    assert_eq!(references(&entries, &mut omitted), ["src/a.rs", "https://example.com", "#409"]);
}

#[test]
fn collection_overflow_and_clipped_quotes_are_explicit() {
    let entries: Vec<_> = (0..20)
        .map(|i| {
            entry(i, 1, EntryKind::User, json!({"text":format!("Never change path/{i}.txt {}", "界".repeat(400))}))
        })
        .collect();
    let mut omitted = Vec::new();
    let rules = constraints(&entries, &mut omitted);
    let refs = references(&entries, &mut omitted);
    assert_eq!(rules.len(), QUOTES);
    assert_eq!(refs.len(), REFS);
    assert_eq!(omitted.len(), 28);
    assert!(rules.iter().all(|r| r.contains("[truncated;")));
    assert!(omitted.iter().all(|r| !r.contains("Never") && !r.contains('界')));
}

#[test]
fn files_have_complete_hashes_resolved_from_the_bots_working_folder() {
    let dir = Temp::new();
    std::fs::write(dir.path().join("same.txt"), "hello").unwrap();
    let files = files::collect(&[read(1, 1, "same.txt")], dir.path(), &[], &mut Vec::new());
    assert_eq!(files[0].bytes, Some(5));
    assert_eq!(files[0].sha256.as_deref(), Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"));
    assert_eq!(files[0].unavailable, None);
}

#[test]
fn denied_pending_failed_and_unknown_edits_are_not_completed_work() {
    let dir = Temp::new();
    let entries = [
        entry(1, 1, EntryKind::Tool, json!({"status":"completed","diffs":[{"path":"done.txt","isNew":true}]})),
        entry(2, 1, EntryKind::Permission, json!({"status":"denied","diffs":[{"path":"denied.txt"}]})),
        entry(3, 1, EntryKind::Tool, json!({"status":"pending","diffs":[{"path":"pending.txt"}]})),
        entry(4, 1, EntryKind::Tool, json!({"status":"failed","diffs":[{"path":"failed.txt"}]})),
        entry(5, 1, EntryKind::Tool, json!({"diffs":[{"path":"unknown.txt"}]})),
        entry(6, 1, EntryKind::Agent, json!({"status":"completed","diffs":[{"path":"invented.txt"}]})),
    ];
    let files = files::collect(&entries, dir.path(), &[], &mut Vec::new());
    assert_eq!(
        files.iter().map(|f| f.why).collect::<Vec<_>>(),
        ["unconfirmed proposal", "failed change", "pending proposal", "denied proposal", "created"]
    );
    assert!(!files.iter().any(|f| f.path == "invented.txt"));
}

#[test]
fn outside_paths_directories_and_oversized_files_are_not_read() {
    let dir = Temp::new();
    let outside = Temp::new();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let large = std::fs::File::create(dir.path().join("large.bin")).unwrap();
    large.set_len(65 * 1024 * 1024).unwrap();
    let entries =
        [read(1, 1, outside.path().join("secret.txt").to_str().unwrap()), read(2, 1, "."), read(3, 1, "large.bin")];
    let files = files::collect(&entries, dir.path(), &[], &mut Vec::new());
    assert_eq!(files[0].bytes, Some(65 * 1024 * 1024));
    assert!(files.iter().all(|f| f.sha256.is_none() && f.unavailable.is_some()));
}

#[test]
fn explicitly_allowed_memory_folders_can_be_inspected() {
    let dir = Temp::new();
    let memory = Temp::new();
    let path = memory.path().join("note.md");
    std::fs::write(&path, "note").unwrap();
    let files = files::collect(
        &[read(1, 1, path.to_str().unwrap())],
        dir.path(),
        &[memory.path().to_path_buf()],
        &mut Vec::new(),
    );
    assert_eq!(files[0].bytes, Some(4));
    assert!(files[0].sha256.is_some());
}

#[cfg(unix)]
#[test]
fn named_pipes_and_symlink_escapes_are_not_opened() {
    use std::os::unix::fs::symlink;
    let dir = Temp::new();
    let outside = Temp::new();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    symlink(outside.path(), dir.path().join("escape")).unwrap();
    let status = std::process::Command::new("mkfifo").arg(dir.path().join("pipe")).status().unwrap();
    assert!(status.success());
    let files =
        files::collect(&[read(1, 1, "pipe"), read(2, 1, "escape/secret.txt")], dir.path(), &[], &mut Vec::new());
    assert!(files.iter().all(|f| f.sha256.is_none() && f.bytes.is_none()));
}

#[test]
fn every_collection_cap_accounts_for_its_overflow() {
    let dir = Temp::new();
    let mut omitted = Vec::new();
    let entries: Vec<_> = (0..20).map(|i| read(i, 1, &format!("{i}.txt"))).collect();
    assert_eq!(files::collect(&entries, dir.path(), &[], &mut omitted).len(), FILES);
    assert_eq!(omitted.len(), 8);
    assert!(omitted.iter().all(|line| line.contains(".txt") && line.contains("bytes") && line.contains("read")));
    let entries: Vec<_> = (0..20)
        .map(|i| {
            entry(
                i,
                1,
                EntryKind::Tool,
                json!({"title":"cargo test","toolKind":"execute","status":"failed","output":"[exit code: 1]"}),
            )
        })
        .collect();
    assert_eq!(commands(&entries, &mut omitted).len(), COMMANDS);
    assert_eq!(omitted.len(), 20);
    let entries: Vec<_> =
        (0..20).map(|i| entry(i, 1, EntryKind::Permission, json!({"title":"Run","status":"pending"}))).collect();
    assert_eq!(waiting(&entries, &mut omitted).len(), WAITING);
    assert_eq!(omitted.len(), 35);
}

#[test]
fn servers_use_the_real_ports_array_and_only_active_runs_are_named() {
    let servers = json!([
        {"name":"web","status":"running","ports":[{"port":5173,"kind":"http"},{"port":9222,"kind":"tcp"}]},
        {"name":"gone","status":"stopped","ports":[{"port":9000}]}
    ]);
    let active = json!({"routines":[{"id":"r1","name":"daily"}],"runs":[{"routineId":"r1","status":"pending"}]});
    assert_eq!(
        running(&servers, &active, &mut Vec::new()),
        ["dev server web (ports: 5173, 9222)", "routine daily (pending)"]
    );
    // A finished run is not what the next agent has to wait for, and a run whose definition is gone keeps its id.
    let finished = json!({"routines":[{"id":"r1","name":"daily"}],"runs":[{"routineId":"r1","status":"succeeded"}]});
    assert_eq!(running(&servers, &finished, &mut Vec::new()), ["dev server web (ports: 5173, 9222)"]);
    let unnamed = json!({"routines":[],"runs":[{"routineId":"r2","status":"running"}]});
    assert_eq!(running(&json!([]), &unnamed, &mut Vec::new()), ["routine r2 (running)"]);
    let many = Value::Array((0..20).map(|_| json!({"name":"server","status":"starting","ports":[]})).collect());
    let mut omitted = Vec::new();
    assert_eq!(running(&many, &json!({}), &mut omitted).len(), RUNNING);
    assert_eq!(omitted.len(), 8);
}

#[test]
fn older_active_routine_runs_past_200_finished_runs_are_still_collected() {
    let store = crate::store::Store::open(Path::new(":memory:")).unwrap();
    let mut runs = vec![json!({
        "id": "run-old-active",
        "routineId": "r-active",
        "botId": "b1",
        "status": "pending",
        "createdAt": 1,
        "finishedAt": null,
        "detail": null,
        "event": null,
        "deliveryId": null,
        "notBefore": 9_999_999_999_999_i64
    })];
    for i in 0..210 {
        runs.push(json!({
            "id": format!("run-done-{i}"),
            "routineId": "r-done",
            "botId": "b1",
            "status": "succeeded",
            "createdAt": 100 + i,
            "finishedAt": 101 + i,
            "detail": null,
            "event": null,
            "deliveryId": null
        }));
    }
    let state = json!({
        "routines": [
            {"id": "r-active", "botId": "b1", "name": "long-retry", "instruction": "do", "triggers": [], "enabled": true, "createdAt": 1, "updatedAt": 1, "nextRuns": [], "webhookKey": "k1", "deleted": false},
            {"id": "r-done", "botId": "b1", "name": "frequent", "instruction": "do", "triggers": [], "enabled": true, "createdAt": 1, "updatedAt": 1, "nextRuns": [], "webhookKey": "k2", "deleted": false}
        ],
        "runs": runs,
        "followUps": []
    });
    store.kv_set("routines.v1", &state.to_string()).unwrap();
    let routines = crate::routines::Routines::default();
    routines.restore(&store).unwrap();
    // The list's newest 200 runs are all finished ones, so it shows no active run at all.
    assert_eq!(running(&json!([]), &routines.list("b1"), &mut Vec::new()), Vec::<String>::new());
    let mut omitted = Vec::new();
    let collected = running(&json!([]), &routines.active_runs("b1"), &mut omitted);
    assert_eq!(collected, ["routine long-retry (pending)"]);
    assert_eq!(omitted, Vec::<String>::new());
}

#[test]
fn commands_preserve_the_reported_exit_code_and_tool_state() {
    let entries = [entry(
        1,
        1,
        EntryKind::Tool,
        json!({"title":"cargo test","toolKind":"execute","status":"failed","output":"Process exited with code 2"}),
    )];
    let commands = commands(&entries, &mut Vec::new());
    assert_eq!(commands[0].status, Some(2));
    assert_eq!(commands[0].state, "failed");
    assert_eq!(exit_status("response code: 200"), None);
    assert_eq!(exit_status("exit status -1"), Some(-1));
}

#[test]
fn rendering_obeys_every_budget_without_reinserting_dropped_content() {
    let ledger = Ledger {
        constraints: vec![format!("Never {}", "private-content".repeat(800))],
        files: vec![Touched {
            path: "src/changed.rs".into(),
            bytes: Some(123),
            sha256: Some("a".repeat(64)),
            why: "modified",
            unavailable: None,
        }],
        waiting: vec!["pending-secret".repeat(800)],
        omitted: vec!["collection overflow (9 B; collection limit)".into()],
        ..Ledger::default()
    };
    for budget in 0..1500 {
        if let Ok(text) = render(&ledger, budget) {
            assert!(text.len() <= budget, "budget={budget} actual={}", text.len());
            assert!(!text.contains("private-content") && !text.contains("pending-secret"));
            assert!(text.contains("Omitted:") && text.contains("collection limit"));
        } else {
            // The wrapper and four individual labels alone need 382 bytes; a budget below that
            // cannot disclose every item and must fail instead of sending a truncated handoff.
            assert!(budget < 400, "an ordinary budget must name every omitted item: {budget}");
        }
    }
    let text = render(&ledger, 500).expect("an itemized bounded manifest");
    assert!(text.contains("Omitted:") && text.contains("collection limit"));
    assert!(text.contains("src/changed.rs") && text.contains("123 B"));
    assert!(!text.contains("private-content"), "the long user rule is named, not copied: {text}");
}

#[test]
fn an_empty_ledger_adds_nothing() {
    assert_eq!(render(&Ledger::default(), 1000).expect("an empty manifest"), "");
}

#[test]
fn a_budget_too_small_for_each_item_label_fails_the_handoff() {
    let ledger = Ledger {
        files: (0..4)
            .map(|_| Touched {
                path: format!("src/file-{}.rs", "x".repeat(80)),
                bytes: Some(123),
                sha256: None,
                why: "modified",
                unavailable: None,
            })
            .collect(),
        omitted: vec!["a collection-stage omission (123 B; collection limit)".into()],
        ..Ledger::default()
    };
    assert!(render(&ledger, 20).is_err(), "no truncated or silently dropped item labels");
}

#[test]
fn thread_memory_roots_use_parent_bot_folder() {
    let cfg: crate::store::BotConfig =
        serde_json::from_value(json!({"id": "thread-123", "name": "Thread", "threadOf": "parent-bot"})).unwrap();
    let roots = memory_roots(&cfg);
    assert!(roots.contains(&crate::chat::memory::files::folder(Some("parent-bot"))));
    assert!(!roots.contains(&crate::chat::memory::files::folder(Some("thread-123"))));
}

#[test]
fn cancelled_messages_do_not_hide_the_latest_work_turn() {
    let entries = vec![
        read(1, 1, "worked.txt"),
        entry(2, 1, EntryKind::Agent, json!({"text":"done","final":true})),
        entry(3, 2, EntryKind::User, json!({"text":"cancelled follow-up","status":"cancelled","removed":true})),
        entry(4, 3, EntryKind::User, json!({"text":"current prompt","status":"sent"})),
        entry(5, 4, EntryKind::User, json!({"text":"queued next","status":"queued"})),
    ];
    let (_, work) = scoped(entries, 0, &["e4".into()]);
    assert_eq!(work.len(), 2);
    assert!(work.iter().all(|e| e.turn == 1));
}

#[test]
fn open_regular_rejects_paths_resolving_outside_allowed_root() {
    let root = Temp::new();
    let outside = Temp::new();
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();

    let canonical_root = root.path().canonicalize().unwrap();
    let canonical_secret = secret.canonicalize().unwrap();
    assert_eq!(files::open_regular(&canonical_root, &canonical_secret).unwrap_err(), "outside allowed folders");

    #[cfg(windows)]
    {
        // Simulate an ancestor directory being swapped with an NTFS junction after `resolve()`.
        let junction = root.path().join("sub");
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(outside.path())
            .status()
            .unwrap();
        assert!(status.success());
        let swapped = canonical_root.join("sub").join("secret.txt");
        assert_eq!(files::open_regular(&canonical_root, &swapped).unwrap_err(), "outside allowed folders");
        let _ = std::fs::remove_dir(&junction);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link_dir = root.path().join("sub");
        symlink(outside.path(), &link_dir).unwrap();
        let swapped = canonical_root.join("sub").join("secret.txt");
        assert_eq!(files::open_regular(&canonical_root, &swapped).unwrap_err(), "outside allowed folders");
    }
}
