use super::*;
use crate::store::EntryKind;
use serde_json::{Value, json};

/// A tool call, newest first when listed by falling `seq`.
fn tool(seq: i64, data: Value) -> Entry {
    Entry {
        id: format!("e{seq}"),
        seq,
        bot_id: "b".into(),
        rev: seq,
        kind: EntryKind::Tool.as_str().into(),
        turn: seq,
        data,
        created_at: 1_790_000_000_000,
        updated_at: 0,
    }
}

fn edit(seq: i64, status: &str, path: &str, is_new: bool) -> Entry {
    tool(
        seq,
        json!({"title": format!("Edit {path}"), "toolKind": "edit", "status": status,
            "diffs": [{"path": path, "isNew": is_new, "patch": "+secret contents"}]}),
    )
}

fn run(seq: i64, status: &str, line: &str) -> Entry {
    tool(seq, json!({"title": line, "toolKind": "execute", "status": status, "output": "secret output"}))
}

#[test]
fn only_finished_edits_count_and_a_file_made_here_stays_new() {
    let tools = [
        edit(5, "completed", "/repo/src/a.rs", false),
        edit(4, "failed", "/repo/src/refused.rs", false),
        edit(3, "pending", "/repo/src/proposed.rs", true),
        edit(2, "completed", "/repo/src/a.rs", true),
        edit(1, "completed", "/repo/src/b.rs", false),
        tool(0, json!({"title": "Read /repo/src/c.rs", "toolKind": "read", "status": "completed"})),
    ];
    let ledger = Ledger::of(&tools, &[]);
    assert_eq!(
        ledger.files,
        [
            Changed { path: "/repo/src/a.rs".into(), change: Change::Created },
            Changed { path: "/repo/src/b.rs".into(), change: Change::Modified },
        ],
        "newest first, each once; a refused or proposed edit and a read changed nothing"
    );
}

#[test]
fn commands_are_listed_once_as_they_last_ended_with_failures_first() {
    let tools = [
        run(5, "completed", "cargo fmt"),
        run(4, "in_progress", "npm run dev"),
        run(3, "failed", "cargo test"),
        run(2, "completed", "cargo fmt"),
        run(1, "failed", "cargo fmt"),
        tool(0, json!({"title": "Read a.rs", "toolKind": "read", "status": "completed"})),
    ];
    let ledger = Ledger::of(&tools, &[]);
    let listed: Vec<(&str, Ended)> = ledger.commands.iter().map(|c| (c.line.as_str(), c.ended)).collect();
    assert_eq!(
        listed,
        [("cargo test", Ended::Failed), ("cargo fmt", Ended::Completed), ("npm run dev", Ended::Unfinished)],
        "the newest run of a line is how it stands"
    );
}

#[test]
fn a_command_of_several_lines_is_one_line_with_its_own_ending() {
    let tools = [
        run(2, "failed", "cat > config.py <<'EOF'\nAPI_URL = 1\nEOF"),
        run(1, "completed", "cat > config.py  <<'EOF'\n  API_URL = 1\nEOF\n"),
    ];
    let text = Ledger::of(&tools, &[]).render().expect("a ledger");
    assert!(
        text.contains("failed ones first:\n- cat > config.py <<'EOF' API_URL = 1 EOF (failed)\n</ledger>"),
        "{text}"
    );
}

#[test]
fn running_dev_servers_are_named_with_their_ports() {
    let servers = [("web".to_owned(), vec![5173]), ("api".to_owned(), vec![]), ("both".to_owned(), vec![3000, 3001])];
    assert_eq!(
        Ledger::of(&[], &servers).running,
        ["dev server web on port 5173", "dev server api", "dev server both on ports 3000, 3001"]
    );
}

#[test]
fn the_ledger_is_an_index_with_what_it_leaves_out_counted() {
    let mut tools: Vec<Entry> =
        (0..14).rev().map(|i| edit(100 + i, "completed", &format!("/repo/f{i}.rs"), false)).collect();
    tools.push(run(1, "failed", "cargo test"));
    let text = Ledger::of(&tools, &[("web".to_owned(), vec![5173])]).render().expect("a ledger");
    assert!(text.starts_with("<ledger>\n") && text.ends_with("\n</ledger>"), "{text}");
    for line in [
        "Files changed, newest first:\n- /repo/f13.rs (modified)",
        "- /repo/f4.rs (modified)\n- and 4 more",
        "Commands run, failed ones first:\n- cargo test (failed)",
        "Running now:\n- dev server web on port 5173",
    ] {
        assert!(text.contains(line), "{line} is in the ledger: {text}");
    }
    assert!(!text.contains("/repo/f3.rs"), "past ten files the rest is counted: {text}");
    assert!(!text.contains("secret"), "paths and lines only, never a diff or an output: {text}");
    assert!(text.len() < 2_500, "it stays small: {}", text.len());
}

#[test]
fn a_long_path_or_line_is_cut() {
    let long = format!("/repo/{}.rs", "x".repeat(400));
    let text = Ledger::of(&[edit(2, "completed", &long, false), run(1, "completed", &"y".repeat(400))], &[])
        .render()
        .expect("a ledger");
    assert!(text.lines().all(|line| line.chars().count() < 300), "{text}");
}

#[test]
fn nothing_left_behind_adds_nothing() {
    let read = tool(1, json!({"title": "Read a.rs", "toolKind": "read", "status": "completed"}));
    assert_eq!(Ledger::of(&[read], &[]).render(), None);
}
