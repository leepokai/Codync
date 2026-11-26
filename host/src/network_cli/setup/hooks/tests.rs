use super::*;

const CMD: &str = "/opt/codync/codync-host network hook codex";

#[test]
fn codex_hooks_go_last_once_and_come_out_leaving_the_users_own() {
    let theirs = json!({"hooks": {"PostToolUse": [{"hooks": [{"type": "command", "command": "lint"}]}]}});
    let once = with_ours(theirs.clone(), CMD).unwrap();
    assert_eq!(once["hooks"]["PostToolUse"][0], theirs["hooks"]["PostToolUse"][0], "the user's hook keeps its place");
    assert_eq!(once["hooks"]["PostToolUse"][1]["hooks"][0]["command"], CMD);
    assert_eq!(once["hooks"]["Stop"].as_array().map(Vec::len), Some(1));
    assert_eq!(with_ours(once.clone(), CMD).unwrap(), once, "a second run changes nothing");
    let moved = with_ours(once.clone(), "/new/codync-host network hook codex").unwrap();
    assert_eq!(
        moved["hooks"]["PostToolUse"].as_array().map(Vec::len),
        Some(2),
        "an older one of ours is replaced in place"
    );
    let back = without_ours(once);
    assert_eq!(back["hooks"]["PostToolUse"], theirs["hooks"]["PostToolUse"]);
    assert_eq!(back["hooks"]["Stop"], json!([]));
}

#[test]
fn a_codex_file_of_another_shape_is_refused_and_removing_from_it_changes_nothing() {
    for odd in [json!([]), json!({"hooks": []}), json!({"hooks": {"Stop": "x"}})] {
        assert!(with_ours(odd.clone(), CMD).is_err(), "{odd}");
        assert_eq!(without_ours(odd.clone()), odd);
    }
    assert_eq!(without_ours(json!({})), json!({}), "no `hooks` key is added");
}

#[test]
fn files_of_our_own_hold_both_events_in_each_agents_shape() {
    let copilot = own(HookAgent::Copilot, "h");
    assert_eq!(copilot["version"], 1);
    assert_eq!(copilot["hooks"]["Stop"][0]["bash"], "h");
    assert_eq!(own(HookAgent::Grok, "h")["hooks"]["PostToolUse"][0]["hooks"][0]["command"], "h");
}

#[test]
fn a_users_hooks_beside_ours_in_one_group_stay() {
    let mixed = json!({"hooks": {"PostToolUse": [{"matcher": "Bash", "hooks": [
        {"type": "command", "command": "lint"},
        {"type": "command", "command": "/old/codync-host network hook codex"},
    ]}]}});
    let moved = with_ours(mixed.clone(), "/new/codync-host network hook codex").unwrap();
    let group = &moved["hooks"]["PostToolUse"][0];
    assert_eq!(moved["hooks"]["PostToolUse"].as_array().map(Vec::len), Some(1));
    assert_eq!(group["matcher"], "Bash");
    assert_eq!(group["hooks"][0]["command"], "lint");
    assert_eq!(group["hooks"][1]["command"], "/new/codync-host network hook codex");
    let back = without_ours(moved);
    assert_eq!(
        back["hooks"]["PostToolUse"],
        json!([{"matcher": "Bash", "hooks": [{"type": "command", "command": "lint"}]}])
    );
}

#[test]
fn a_file_of_our_own_with_a_users_hook_in_it_is_left_alone() {
    let dir = std::env::temp_dir().join(format!("codync-hooks-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("codync.json");
    let theirs = r#"{"version": 1, "hooks": {"Stop": [{"type": "command", "bash": "say done"}]}}"#;
    std::fs::write(&file, theirs).unwrap();
    assert!(add(&file, HookAgent::Copilot, "/x/codync-host network hook copilot").is_err());
    assert!(remove(&file, HookAgent::Copilot).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
    std::fs::remove_file(&file).unwrap();
    assert!(add(&file, HookAgent::Copilot, "/x/codync-host network hook copilot").unwrap());
    assert!(remove(&file, HookAgent::Copilot).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}
