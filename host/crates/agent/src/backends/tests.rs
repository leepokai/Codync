use super::search_path::TestSearchPath;
use super::*;
use std::collections::HashSet;
use std::path::PathBuf;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("codync-bin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create the fixture directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Cleanup must not hide the test result when the directory is already gone.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An executable `name` the lookup can find. Windows only tries `.exe`, `.cmd`, and `.bat`.
fn write_cli(dir: &Path, name: &str) {
    let path = dir.join(if cfg!(windows) { format!("{name}.cmd") } else { name.to_owned() });
    std::fs::write(&path, "#!/bin/sh\n").expect("write the fixture CLI");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mark the fixture executable");
    }
}

#[test]
fn an_installed_cli_comes_before_the_registry_build() {
    let dir = TempDir::new();
    for name in ["codebuddy", "mcode", "cursor-agent"] {
        write_cli(&dir.0, name);
    }
    let _path = TestSearchPath::set(dir.0.clone());
    let builds = ["codebuddy-code", "minimax-code", "cursor"].map(|id| json!({"id": id}));
    registry::set_cache(Some(json!({"agents": builds})));
    let found = [("codebuddy", "codebuddy"), ("minimax", "mcode"), ("cursor", "cursor-agent")]
        .map(|(id, bin)| (id, bin, launch_candidates(id)));
    registry::set_cache(None);
    for (id, bin, candidates) in found {
        let candidates = candidates.expect("an installed CLI can start");
        assert!(
            matches!(&candidates[..], [registry::Candidate::Ready(cmd), registry::Candidate::Build(_)] if cmd.program.contains(bin)),
            "{id}: the installed CLI first, the registry build kept for a CLI too old for ACP"
        );
    }
}

#[test]
fn a_harness_the_registry_also_lists_shows_once() {
    let dir = TempDir::new();
    let _path = TestSearchPath::set(dir.0.clone());
    let agents: Vec<_> =
        HARNESSES.iter().map(|h| h.id).chain(["unknown-agent"]).map(|id| json!({"id": id, "name": id})).collect();
    registry::set_cache(Some(json!({"agents": agents})));
    let listed = list();
    registry::set_cache(None);
    let ids: Vec<_> = listed.iter().filter_map(|b| b["id"].as_str()).collect();
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len(), "clients key their lists by id: {ids:?}");
    assert!(ids.contains(&"unknown-agent"), "a registry agent we don't know by name is still offered");
}

#[test]
fn every_harness_is_unique() {
    let ids: HashSet<_> = HARNESSES.iter().map(|h| h.id).collect();
    assert_eq!(ids.len(), HARNESSES.len());
    let bins: Vec<_> = HARNESSES.iter().flat_map(|h| h.bins).collect();
    assert_eq!(bins.iter().collect::<HashSet<_>>().len(), bins.len(), "a CLI belongs to one harness");
}

#[test]
fn workbuddy_session_does_not_prove_sign_in() {
    assert!(!session_proves_sign_in("workbuddy"));
    assert!(session_proves_sign_in("codebuddy"));
}

#[test]
fn workbuddy_starts_its_lite_bundle() {
    let workbuddy = harness("workbuddy").unwrap();
    assert_eq!(workbuddy.local, Some("{bin} --serve --acp"));
    assert!(workbuddy.login.is_none());
    let mut env = vec![("CODEBUDDY_FORCE_LITE_WB_BUNDLE".into(), "0".into())];
    apply_process_env("workbuddy", &mut env);
    assert_eq!(env, vec![("CODEBUDDY_FORCE_LITE_WB_BUNDLE".into(), "0".into())]);
    env.clear();
    apply_process_env("workbuddy", &mut env);
    assert_eq!(env, vec![("CODEBUDDY_FORCE_LITE_WB_BUNDLE".into(), "1".into())]);
    apply_process_env("codebuddy", &mut env);
    assert_eq!(env.len(), 1);
}

#[test]
fn workbuddy_cli_is_not_codebuddy_code() {
    let bundled = "/Applications/WorkBuddy.app/Contents/Resources/app.asar.unpacked/cli/bin/codebuddy";
    assert!(!belongs_to_other("workbuddy", bundled));
    assert!(belongs_to_other("codebuddy", bundled), "the app's CLI is WorkBuddy's, not CodeBuddy Code's");
    assert!(belongs_to_other("workbuddy", "/opt/homebrew/bin/codebuddy"));
}

#[test]
fn a_codebuddy_on_path_is_not_workbuddy() {
    let dir = TempDir::new();
    write_cli(&dir.0, "codebuddy");
    let _path = TestSearchPath::set(dir.0.clone());
    let on_path = dir.0.to_string_lossy().into_owned();
    let from_path =
        |c: &registry::Candidate| matches!(c, registry::Candidate::Ready(cmd) if cmd.program.contains(&on_path));
    assert!(launch_candidates("codebuddy").unwrap().iter().any(from_path), "CodeBuddy Code's own CLI");
    assert!(!launch_candidates("workbuddy").is_ok_and(|c| c.iter().any(from_path)), "never WorkBuddy's");
}

#[test]
fn hermes_uses_native_acp() {
    let h = harness("hermes").unwrap();
    assert_eq!((h.local, h.registry, h.login), (Some("{bin} acp"), None, Some("{bin} acp --setup")));
}

#[test]
fn an_outdated_cli_is_told_to_update() {
    let why = exit_reason("dsh", "Error: profile acp does not exist").expect("a reason");
    assert!(why.starts_with("The DeepSeek Harness on this computer is too old to run in Codync. Update it (run `"));
    // Codync's own npm, which works without one on the computer.
    let shell = if cfg!(windows) { " in Command Prompt" } else { "" };
    let suffix = format!(" npm install -g @deepseek-ai/dsh@latest`{shell} on this computer) and try again.");
    assert!(why.ends_with(&suffix), "{why}");
    // Another harness saying the same isn't outdated for it.
    assert_eq!(exit_reason("kiro", "profile acp does not exist").as_deref(), Some("profile acp does not exist"));
    assert_eq!(exit_reason("dsh", "Error: no API key").as_deref(), Some("Error: no API key"));
}

#[test]
fn doubao_uses_its_own_acp_server() {
    let h = harness("doubao").unwrap();
    assert_eq!(
        (h.local, h.registry, h.login, h.bins),
        (Some("{bin} acp serve"), None, Some("{bin} login"), &["doubao"][..])
    );
    assert!(matches!(h.install, Some(Install::Npm("@bytedance/doubao-cli"))));
    assert_eq!(h.signed_in.map(|check| check.args), Some("auth status"));
}

#[test]
fn doubao_is_offered_only_where_its_cli_ships() {
    let offered = list().iter().any(|agent| agent["id"] == "doubao");
    let ships = cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
    ));
    assert_eq!(offered, ships);
}

#[test]
fn an_outdated_doubao_is_told_to_update() {
    let why = exit_reason("doubao", "未知命令或选项: acp").expect("a reason");
    assert!(why.starts_with("The Doubao on this computer is too old to run in Codync."));
    assert!(why.contains("@bytedance/doubao-cli@latest"), "{why}");
    assert_eq!(exit_reason("kilo", "未知命令或选项").as_deref(), Some("未知命令或选项"));
}
