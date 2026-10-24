use super::*;
use std::sync::MutexGuard;

/// Serializes tests that replace the process search path. `cargo test` runs them
/// together, so each would otherwise search the other's directory.
static SEARCH_LOCK: Mutex<()> = Mutex::new(());

/// Holds the process search path on one directory until dropped.
#[must_use]
struct SearchPath {
    _guard: MutexGuard<'static, ()>,
}

impl SearchPath {
    fn set(dir: PathBuf) -> Self {
        let guard = SEARCH_LOCK.locked();
        *SEARCH_PATH.locked() = vec![dir];
        Self { _guard: guard }
    }
}

impl Drop for SearchPath {
    fn drop(&mut self) {
        SEARCH_PATH.locked().clear();
    }
}

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

/// `name` as the lookup can find it. Windows only tries `.exe`, `.cmd`, and `.bat`.
fn cli_path(dir: &Path, name: &str) -> PathBuf {
    let file = if cfg!(windows) { format!("{name}.cmd") } else { name.to_owned() };
    dir.join(file)
}

fn write_cli(dir: &Path, name: &str, executable: bool) -> PathBuf {
    let path = cli_path(dir, name);
    std::fs::write(&path, "#!/bin/sh\n").expect("write the fixture CLI");
    if executable {
        mark_executable(&path);
    }
    path
}

fn mark_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("mark the fixture executable");
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(unix)]
#[test]
fn detects_executables_on_search_path() {
    let dir = TempDir::new();
    let bin = write_cli(&dir.0, "fake-harness", false);
    let _path = SearchPath::set(dir.0.clone());
    assert!(which("fake-harness").is_none(), "not executable yet");
    mark_executable(&bin);
    assert_eq!(which("fake-harness"), Some(bin));
}

#[test]
fn an_installed_cli_is_started_without_a_second_copy() {
    let dir = TempDir::new();
    for name in ["codebuddy", "mcode", "cursor-agent"] {
        write_cli(&dir.0, name, true);
    }
    let _path = SearchPath::set(dir.0.clone());
    tokio::runtime::Builder::new_current_thread().build().expect("test runtime").block_on(async {
        for (id, bin) in [("codebuddy", "codebuddy"), ("minimax", "mcode"), ("cursor", "cursor-agent")] {
            let cmds = launch_candidates(id, |_| panic!("{id} is installed and should not be downloaded"))
                .await
                .expect("an installed CLI starts without a download");
            assert_eq!(cmds.len(), 1, "{id}");
            assert!(cmds[0].program.contains(bin), "{id}: {}", cmds[0].program);
            assert!(!cmds[0].program.contains("npx"), "{id}: {}", cmds[0].program);
        }
    });
}

#[test]
fn every_harness_is_unique() {
    let ids: HashSet<_> = HARNESSES.iter().map(|h| h.id).collect();
    assert_eq!(ids.len(), HARNESSES.len());
}
