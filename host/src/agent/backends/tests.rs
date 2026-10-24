use super::search_path::TestSearchPath;
use super::*;
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
fn every_harness_is_unique() {
    let ids: HashSet<_> = HARNESSES.iter().map(|h| h.id).collect();
    assert_eq!(ids.len(), HARNESSES.len());
    let bins: Vec<_> = HARNESSES.iter().flat_map(|h| h.bins).collect();
    assert_eq!(bins.iter().collect::<HashSet<_>>().len(), bins.len(), "a CLI belongs to one harness");
}

#[test]
fn hermes_uses_native_acp() {
    let h = harness("hermes").unwrap();
    assert_eq!((h.local, h.registry, h.login), (Some("{bin} acp"), None, Some("{bin} acp --setup")));
}
