use super::detected::{Detected, agent_programs, changes, servers_in, still_in_tree};
use super::logs::{last_lines, read_end};
use super::platform::same_boot;
use super::ports::{Listening, Port, sockets_of, urls_with};
use super::scan::SETTLE_MS;
use super::tools::tidied;
use super::{Live, Saved, Status};
use crate::agent::process::{JobProcess, descendants, parse_ps};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;

fn socket(text: &str) -> SocketAddr {
    text.parse().expect("a valid socket address in a test")
}

fn lan(port: u16) -> Vec<String> {
    vec![format!("http://192.168.1.20:{port}")]
}

#[test]
fn a_loopback_server_is_only_on_this_computer() {
    let ports = urls_with(&[socket("127.0.0.1:5173")], lan);
    assert_eq!(ports, [Port { port: 5173, local: "http://localhost:5173".into(), remote: vec![] }]);
    let v6 = urls_with(&[socket("[::1]:5173")], lan);
    assert_eq!(v6[0].remote, Vec::<String>::new());
}

#[test]
fn a_server_on_all_interfaces_is_reachable_from_other_devices() {
    let ports = urls_with(&[socket("0.0.0.0:3000")], lan);
    assert_eq!(ports[0].local, "http://localhost:3000");
    assert_eq!(ports[0].remote, ["http://192.168.1.20:3000"]);
}

#[test]
fn a_server_on_one_address_is_shown_at_that_address() {
    let ports = urls_with(&[socket("192.168.1.9:8080")], lan);
    assert_eq!(ports[0].local, "http://192.168.1.9:8080");
    assert_eq!(ports[0].remote, ["http://192.168.1.9:8080"]);
}

#[test]
fn one_port_on_two_families_is_one_port() {
    let ports = urls_with(&[socket("127.0.0.1:5173"), socket("[::1]:5173"), socket("127.0.0.1:8787")], lan);
    assert_eq!(ports.iter().map(|p| p.port).collect::<Vec<_>>(), [5173, 8787]);
}

fn row(pid: u32, text: &str) -> Listening {
    Listening { pid, name: "node".into(), socket: socket(text) }
}

#[test]
fn sockets_belong_to_their_processes_in_a_stable_order() {
    let listening = [row(1, "127.0.0.1:9000"), row(2, "127.0.0.1:3000"), row(2, "127.0.0.1:1000")];
    assert_eq!(sockets_of(&listening, &[2]), [socket("127.0.0.1:1000"), socket("127.0.0.1:3000")]);
}

fn process(pid: u32, parent: u32, name: &str) -> JobProcess {
    JobProcess { pid, parent, name: name.into() }
}

fn pids(servers: &[Detected]) -> Vec<Vec<u32>> {
    servers.iter().map(|server| server.pids.clone()).collect()
}

#[test]
fn what_a_helper_started_is_not_a_bots_server() {
    let tree = [
        process(10, 1, "claude"),
        process(11, 10, "/opt/codync-host"),
        process(12, 11, "node"),
        process(13, 10, "sh"),
        process(14, 13, "node"),
    ];
    let listening: Vec<Listening> = [12, 14].iter().map(|pid| row(*pid, &format!("127.0.0.1:{pid}00"))).collect();
    assert_eq!(pids(&servers_in(&tree, "codync-host", &[], &listening)), [[14]]);
    let windows = [process(20, 1, r"C:\bin\codync-hostw.exe"), process(21, 20, "node.exe"), process(22, 1, "node.exe")];
    let listening = [row(21, "127.0.0.1:2100"), row(22, "127.0.0.1:2200")];
    assert_eq!(pids(&servers_in(&windows, "codync-hostw.exe", &[], &listening)), [[22]]);
}

#[test]
fn parent_links_that_loop_end() {
    let tree = [process(1, 2, "a"), process(2, 1, "b")];
    let listening = [row(1, "127.0.0.1:1000"), row(2, "127.0.0.1:2000")];
    assert_eq!(pids(&servers_in(&tree, "codync-host", &[], &listening)), [[1], [2]]);
}

#[test]
fn an_agents_own_listener_is_not_a_server_it_left_running() {
    // What Windows lists (the whole job) and what a wrapper script gives on Unix: the real agent under another program.
    let tree = [process(1, 900, "cmd.exe"), process(2, 1, r"C:\bin\opencode.exe"), process(3, 2, "node.exe")];
    let listening = [row(2, "127.0.0.1:4096"), row(3, "127.0.0.1:5173")];
    let programs = agent_programs("opencode", "opencode acp");
    assert_eq!(pids(&servers_in(&tree, "codync-host", &programs, &listening)), [[3]]);
    let unix = [process(2, 1, "/usr/local/bin/opencode")];
    assert_eq!(pids(&servers_in(&unix, "codync-host", &programs, &listening)), Vec::<Vec<u32>>::new());
}

#[test]
fn an_agents_programs_are_its_clis_and_its_commands_own() {
    assert_eq!(agent_programs("opencode", "'/opt/my tools/opencode' acp"), ["opencode"]);
    assert_eq!(agent_programs("custom", "/opt/my-agent --acp"), ["my-agent"]);
    assert_eq!(agent_programs("custom", "FOO=1 BAR=2 my-agent"), ["my-agent"]);
    assert_eq!(agent_programs("custom", r#""C:\tools\my-agent.exe" --acp"#), ["my-agent"]);
    // These run servers just as well, so a bot on them still gets its servers shown.
    for runtime in
        ["python3 -u agent.py", "python3.12 agent.py", "npx -y @x/acp", "node adapter.js", "/bin/sh wrapper.sh"]
    {
        assert_eq!(agent_programs("custom", runtime), Vec::<String>::new(), "{runtime}");
    }
}

#[test]
fn processes_sharing_a_socket_are_one_server() {
    // A server that forks after listening, and another on its own socket.
    let tree = [process(30, 1, "python3"), process(31, 30, "python3"), process(40, 1, "node")];
    let listening = [row(31, "127.0.0.1:21000"), row(30, "127.0.0.1:21000"), row(40, "127.0.0.1:5173")];
    let servers = servers_in(&tree, "codync-host", &[], &listening);
    assert_eq!(pids(&servers), [vec![30, 31], vec![40]]);
    assert_eq!(servers[0].view().id, "detected:30");
}

#[test]
fn ports_the_system_hands_out_are_not_a_servers() {
    let tree = [process(1, 900, "node")];
    assert_eq!(pids(&servers_in(&tree, "codync-host", &[], &[row(1, "127.0.0.1:49152")])), Vec::<Vec<u32>>::new());
}

fn found(pids: Vec<u32>) -> Detected {
    Detected { pids, name: "node".into(), sockets: Vec::new(), ports: Vec::new() }
}

#[test]
fn a_scan_tells_of_what_changed_and_counts_what_is_new() {
    let before: HashMap<String, Vec<Detected>> =
        [("a".to_owned(), vec![found(vec![1])]), ("b".to_owned(), vec![found(vec![2])])].into();
    let now: HashMap<String, Vec<Detected>> =
        [("a".to_owned(), vec![found(vec![1])]), ("c".to_owned(), vec![found(vec![3]), found(vec![4])])].into();
    let (mut bots, new) = changes(&before, &now);
    bots.sort();
    assert_eq!(bots, ["b", "c"]);
    assert_eq!(new, 2);
}

#[test]
fn ps_rows_keep_names_with_spaces() {
    let rows = parse_ps("  10     1 /Applications/Some App/run\n bad row\n 11 10 sh\n 12 11\n");
    assert_eq!(rows.iter().map(|r| (r.pid, r.parent)).collect::<Vec<_>>(), [(10, 1), (11, 10), (12, 11)]);
    assert_eq!(rows[0].name, "/Applications/Some App/run");
}

#[test]
fn a_tree_holds_only_what_its_leader_started() {
    let rows = [process(10, 1, "agent"), process(11, 10, "sh"), process(12, 11, "node"), process(13, 1, "other")];
    let pids: Vec<u32> = descendants(&rows, 10).iter().map(|p| p.pid).collect();
    assert_eq!(pids, [11, 12]);
}

#[test]
fn a_log_shows_its_last_lines() {
    assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
    assert_eq!(last_lines("a", 5), "a");
    assert_eq!(last_lines("", 5), "");
}

#[test]
fn a_boot_is_the_same_only_when_both_times_are_known_and_close() {
    assert!(same_boot(Some(1_000_000), Some(1_060_000)));
    assert!(!same_boot(Some(1_000_000), Some(1_200_001)));
    assert!(!same_boot(None, Some(1)));
    assert!(!same_boot(Some(1), None));
}

#[test]
fn one_folder_written_two_ways_is_one_folder() {
    let plain = Path::new("/site/web");
    for written in ["/site/web/", "/site/web/.", "/site/./web", "/site//web"] {
        assert_eq!(tidied(Path::new(written)), plain, "{written}");
    }
}

fn live() -> Live {
    Live::stopped(Saved {
        id: "a".into(),
        bot: "b".into(),
        name: "web".into(),
        command: "npm run dev".into(),
        folder: "/site".into(),
        created_at: 0,
        wanted: true,
        approved: false,
        leader: None,
        boot_ms: None,
    })
}

#[test]
fn a_new_run_starts_with_nothing_of_the_last_one() {
    let mut server = live();
    server.ended(false, Some(3));
    server.ports = vec![Port { port: 1, local: String::new(), remote: Vec::new() }];
    server.begin(5);
    assert_eq!((server.status, server.exit_code, server.started_at), (Status::Starting, None, Some(5)));
    assert_eq!(server.ports, []);
}

#[test]
fn how_a_run_ends_says_what_it_is_now() {
    let ended = |asked, code| {
        let mut server = live();
        server.ended(asked, code);
        (server.status, server.saved.wanted)
    };
    // Someone stopped it, whatever its code: it stays wanted until `stop` says otherwise.
    assert_eq!(ended(true, None), (Status::Stopped, true));
    assert_eq!(ended(true, Some(143)), (Status::Stopped, true));
    // By itself: done, or failed, and it isn't brought back.
    assert_eq!(ended(false, Some(0)), (Status::Stopped, false));
    assert_eq!(ended(false, Some(3)), (Status::Failed, false));
    assert_eq!(ended(false, None), (Status::Failed, false));
}

#[test]
fn a_server_settles_on_its_first_socket_or_after_a_while() {
    let mut server = live();
    server.begin(0);
    assert!(!server.observed(Vec::new(), None, SETTLE_MS - 1), "still starting");
    assert_eq!(server.status, Status::Starting);
    assert!(server.observed(Vec::new(), None, SETTLE_MS), "a watch build may never listen");
    assert_eq!(server.status, Status::Running);

    let mut server = live();
    server.begin(0);
    assert!(server.observed(vec![socket("127.0.0.1:1")], None, 1));
    assert_eq!(server.status, Status::Running);
}

#[test]
fn clients_hear_of_addresses_only_when_they_differ() {
    let at =
        |remote: &str| vec![Port { port: 3000, local: "http://localhost:3000".into(), remote: vec![remote.into()] }];
    let sockets = vec![socket("0.0.0.0:3000")];
    let mut server = live();
    server.begin(0);
    assert!(server.observed(sockets.clone(), Some(at("http://192.168.1.20:3000")), 1));
    assert!(!server.observed(sockets.clone(), Some(at("http://192.168.1.20:3000")), 2), "the same network");
    assert!(server.observed(sockets, Some(at("http://10.0.0.5:3000")), 3), "another network");
    assert_eq!(server.ports[0].remote, ["http://10.0.0.5:3000"]);
}

#[test]
fn a_clients_status_from_a_newer_host_reads_as_stopped() {
    assert_eq!(serde_json::from_str::<Status>(r#""sleeping""#).expect("any text decodes"), Status::Stopped);
    assert_eq!(serde_json::from_str::<Status>(r#""failed""#).expect("a known one decodes"), Status::Failed);
}

#[test]
fn stopping_a_detected_server_reaches_only_what_the_agent_still_holds() {
    let tree = [process(30, 1, "python3"), process(40, 1, "node")];
    assert_eq!(still_in_tree(&[30, 31], &tree), [30], "31 is gone, and its pid may be someone else's by now");
    assert_eq!(still_in_tree(&[31], &[]), Vec::<u32>::new());
}

#[test]
fn a_log_that_cant_be_read_says_so_and_a_missing_one_is_empty() {
    let folder = std::env::temp_dir().join(format!("codync-log-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).expect("a folder in a test");
    assert_eq!(read_end(&folder.join("none.log")).expect("no file yet"), "");
    assert!(read_end(&folder).is_err(), "a folder isn't a log");
    std::fs::remove_dir_all(&folder).expect("cleaning up a test's folder");
}
