use super::*;

fn want() -> Want {
    Want { exe: "/opt/codync/codync-host".into(), port: None }
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codync-setup-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn server(command: &str, args: &[&str]) -> Value {
    json!({"command": command, "args": args})
}

#[test]
fn only_this_hosts_network_server_counts_as_ours() {
    let w = want();
    assert_eq!(entry(None, &w), Entry::Absent);
    assert_eq!(entry(Some(&server("/opt/codync/codync-host", &["mcp", "network"])), &w), Entry::Current);
    assert_eq!(entry(Some(&server("/old/codync-host", &["mcp", "network"])), &w), Entry::Stale);
    assert_eq!(entry(Some(&server("/opt/codync/codync-host", &["mcp", "network", "--name", "x"])), &w), Entry::Stale);
    assert_eq!(entry(Some(&json!({"type": "http", "url": "https://mcp.codync.dev"})), &w), Entry::Foreign);
    assert_eq!(entry(Some(&server("npx", &["codync"])), &w), Entry::Foreign);
}

#[test]
fn another_port_goes_into_the_server_and_the_hook() {
    let w = Want { port: Some(19333), ..want() };
    assert_eq!(w.mcp_args(), ["mcp", "network", "--port", "19333"]);
    let exe = shell::quote("/opt/codync/codync-host");
    assert_eq!(w.hook_command(HookAgent::Copilot), format!("{exe} network hook copilot --port 19333"));
    assert_eq!(entry(Some(&server("/opt/codync/codync-host", &["mcp", "network"])), &w), Entry::Stale);
}

#[test]
fn a_file_that_doesnt_parse_is_never_written() {
    let dir = temp();
    let file = dir.join("hooks.json");
    std::fs::write(&file, "{ not json").unwrap();
    assert!(hooks::add(&file, HookAgent::Codex, "h").is_err());
    let err = read_server(&file, "mcpServers").unwrap_err();
    assert_eq!(err.downcast_ref::<Failed>().map(|f| f.0), Some(SetupFailure::UnreadableConfig));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn set_key_touches_only_our_key() {
    let dir = temp();
    let file = dir.join("mcp.json");
    std::fs::write(&file, r#"{"mcpServers": {"other": {"command": "x"}}}"#).unwrap();
    set_key(&file, "mcpServers", Some(json!({"command": "c"}))).unwrap();
    assert_eq!(json_file(&file).unwrap()["mcpServers"]["other"]["command"], "x");
    set_key(&file, "mcpServers", None).unwrap();
    assert_eq!(json_file(&file).unwrap(), json!({"mcpServers": {"other": {"command": "x"}}}));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn toml_and_opencode_entries_read_as_the_same_server() {
    let dir = temp();
    let toml = dir.join("config.toml");
    std::fs::write(
        &toml,
        "[mcp_servers.codync]\ncommand = \"/opt/codync/codync-host\"\nargs = [\"mcp\", \"network\"]\n",
    )
    .unwrap();
    assert_eq!(entry(read_server(&toml, "mcp_servers").unwrap().as_ref(), &want()), Entry::Current);
    let opencode = dir.join("opencode.json");
    std::fs::write(
        &opencode,
        r#"{"mcp": {"codync": {"type": "local", "command": ["/opt/codync/codync-host", "mcp", "network"]}}}"#,
    )
    .unwrap();
    assert_eq!(entry(read_server(&opencode, "mcp").unwrap().as_ref(), &want()), Entry::Current);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn someone_elses_codync_in_cursor_is_left_alone() {
    let home = temp();
    let file = home.join(".cursor/mcp.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let theirs = r#"{"mcpServers": {"codync": {"command": "npx", "args": ["codync"]}}}"#;
    std::fs::write(&file, theirs).unwrap();
    assert_eq!(add_one(SetupAgent::Cursor, &home, &want(), None).unwrap(), Done::Foreign);
    assert_eq!(remove_one(SetupAgent::Cursor, &home, &want(), None).unwrap(), Done::Current);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), theirs);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn claude_projects_with_an_older_server_of_ours_are_found() {
    let config = json!({"projects": {
        "/a": {"mcpServers": {"codync": server("codync-host", &["mcp", "network", "--name", "Claude Code"])}},
        "/b": {"mcpServers": {"codync": server("/opt/codync/codync-host", &["mcp", "network"])}},
        "/c": {"mcpServers": {"codync": server("npx", &["codync"])}},
        "/d": {},
    }});
    assert_eq!(claude_projects(&config, &want(), false), [PathBuf::from("/a")]);
    assert_eq!(claude_projects(&config, &want(), true), [PathBuf::from("/a"), PathBuf::from("/b")]);
}

#[cfg(unix)]
#[test]
fn a_homebrew_keg_is_run_through_its_link() {
    let prefix = temp();
    let keg = prefix.join("Cellar/codync-host/2.0.0/bin/codync-host");
    assert_eq!(linked(keg.clone()), keg, "no link: the keg itself");
    std::fs::create_dir_all(prefix.join("bin")).unwrap();
    std::fs::write(prefix.join("bin/codync-host"), "").unwrap();
    assert_eq!(linked(keg), prefix.join("bin/codync-host"));
    std::fs::remove_dir_all(prefix).unwrap();
}

#[cfg(unix)]
#[test]
fn a_write_follows_a_link_to_a_file_not_made_yet_and_stays_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("codync-link-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(dir.join("dotfiles")).unwrap();
    let link = dir.join("mcp.json");
    std::os::unix::fs::symlink(dir.join("dotfiles/mcp.json"), &link).unwrap();
    write(&link, &json!({"a": 1})).unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link stays");
    let made = dir.join("dotfiles/mcp.json");
    assert_eq!(json_file(&made).unwrap(), json!({"a": 1}));
    assert_eq!(std::fs::metadata(&made).unwrap().permissions().mode() & 0o777, 0o600);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_server_of_ours_turned_off_is_set_up_again() {
    let w = want();
    let off = json!({"command": "/opt/codync/codync-host", "args": ["mcp", "network"], "enabled": false});
    assert_eq!(entry(Some(&off), &w), Entry::Stale);
    let disabled = json!({"command": "/opt/codync/codync-host", "args": ["mcp", "network"], "disabled": true});
    assert_eq!(entry(Some(&disabled), &w), Entry::Stale);
}
