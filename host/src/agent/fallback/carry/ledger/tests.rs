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
fn collection_overflow_keeps_full_user_quotes_and_labels_the_limit() {
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
    assert!(rules.iter().all(|r| r.chars().count() > QUOTE_CHARS && !r.contains("[truncated;")));
    assert!(omitted.iter().all(|r| !r.contains("Never") && !r.contains('界')));
}

#[test]
fn references_cover_quoted_paths_file_names_and_common_identifiers() {
    let entries = [entry(
        1,
        1,
        EntryKind::User,
        json!({"text":"Read \"C:\\My Project\\a.rs\", README.md, issue 409 and ABC-123."}),
    )];
    let refs = references(&entries, &mut Vec::new());
    assert_eq!(refs, [r"C:\My Project\a.rs", "README.md", "issue 409", "ABC-123"]);
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
fn omitted_file_labels_keep_the_complete_path() {
    let path = format!("src/{}/file.rs", "a".repeat(120));
    let file = Touched { path: path.clone(), bytes: Some(1), sha256: None, why: "modified", unavailable: None };
    assert_eq!(render::file_label(&file), format!("{path} (1 B; modified)"));
    let file = Touched { unavailable: Some("hash skipped: 64 MiB limit"), ..file };
    assert_eq!(render::file_label(&file), format!("{path} (1 B; modified; hash skipped: 64 MiB limit)"));
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
fn servers_and_active_routine_runs_are_the_only_running_items() {
    let servers = json!([
        {"name":"web","status":"running","ports":[{"port":5173,"kind":"http"},{"port":9222,"kind":"tcp"}]},
        {"name":"gone","status":"stopped","ports":[{"port":9000}]}
    ]);
    let routines = json!({
        "routines":[{"id":"r1","name":"daily","enabled":true},{"id":"r2","name":"idle","enabled":true}],
        "runs":[{"routineId":"r1","status":"running"},{"routineId":"r1","status":"succeeded"},
                {"routineId":"r2","status":"succeeded"}]
    });
    assert_eq!(
        running(&servers, &routines, &mut Vec::new()),
        ["dev server web (ports: 5173, 9222)", "routine daily (running)"]
    );
    let many = Value::Array((0..20).map(|_| json!({"name":"server","status":"starting","ports":[]})).collect());
    let mut omitted = Vec::new();
    assert_eq!(running(&many, &json!({}), &mut omitted).len(), RUNNING);
    assert_eq!(omitted.len(), 8);
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
