//! Where harness CLIs are looked up: the login shell's PATH plus the usual install dirs.

use crate::LockExt;
use crate::agent::node;
use crate::shell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SEARCH_PATH: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// `…/v22.1.0` → `[22, 1, 0]` (unparseable parts count as 0).
fn node_version(dir: &Path) -> Vec<u32> {
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect())
        .unwrap_or_default()
}

fn well_known_dirs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    well_known_dirs_for(&home, dirs::data_local_dir().as_deref())
}

fn well_known_dirs_for(home: &Path, data_local: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        ["/opt/homebrew/bin", "/usr/local/bin", "/home/linuxbrew/.linuxbrew/bin", "/usr/bin", "/bin"]
            .iter()
            .map(PathBuf::from)
            .collect();
    if cfg!(windows) {
        // Global npm CLIs (%APPDATA%\npm), and Node itself.
        dirs.extend(dirs::data_dir().map(|d| d.join("npm")));
        dirs.extend(std::env::var_os("ProgramFiles").map(|d| PathBuf::from(d).join("nodejs")));
        // oh-my-pi's binary install (%LOCALAPPDATA%\omp); a Bun install lands in ~/.bun/bin below.
        dirs.extend(data_local.map(|d| d.join("omp")));
        // Hermes Agent's Windows launchers, including older source installs.
        dirs.extend(data_local.map(|d| d.join("hermes").join("bin")));
        dirs.extend(data_local.map(|d| d.join("hermes").join("hermes-agent").join(".hermes").join("bin")));
    }
    for rel in [
        // Hermes Agent's Unix installer also puts its launcher here.
        ".local/bin",
        ".bun/bin",
        ".cargo/bin",
        ".volta/bin",
        ".asdf/shims",
        ".local/share/mise/shims",
        ".fnm/aliases/default/bin",
        ".local/share/pnpm",
        "Library/pnpm",
        ".npm-global/bin",
        ".opencode/bin",
        ".deno/bin",
        "go/bin",
    ] {
        dirs.push(home.join(rel));
    }
    // Every nvm-installed node, newest first (global npm CLIs live next to node).
    if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
        let mut versions: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        // Numeric, not lexicographic: v22.1.0 is newer than v9.11.2.
        versions.sort_by_key(|p| std::cmp::Reverse(node_version(p)));
        dirs.extend(versions.into_iter().map(|p| p.join("bin")));
    }
    dirs
}

/// Re-detects the search path and exports it as this process's PATH, so every
/// agent we spawn (and the `npx`/`node` they need) resolves the same way.
pub fn hydrate_path() {
    let mut seen = HashSet::new();
    let mut path: Vec<PathBuf> = vec![];
    let current = std::env::var("PATH").unwrap_or_default();
    let login = shell::user_path().unwrap_or_default();
    for p in std::env::split_paths(&current).chain(std::env::split_paths(&login)).chain(well_known_dirs()) {
        if p.as_os_str().is_empty() || !p.is_dir() {
            continue;
        }
        if seen.insert(p.clone()) {
            path.push(p);
        }
    }
    // Codync's Node last, so the computer's own comes first; listed even before it exists, so
    // adding it (a separate `codync-host node` process) counts without another refresh.
    for p in node::search_dirs() {
        if seen.insert(p.clone()) {
            path.push(p);
        }
    }
    if let Ok(joined) = std::env::join_paths(&path) {
        // SAFETY: called at startup and from the single refresh command, before/between spawns.
        unsafe { std::env::set_var("PATH", joined) };
    }
    *SEARCH_PATH.locked() = path;
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let dirs = {
        let cached = SEARCH_PATH.locked();
        if cached.is_empty() {
            std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default()
        } else {
            cached.clone()
        }
    };
    dirs.into_iter().flat_map(|d| executable_names(bin).map(move |name| d.join(name))).find(|p| is_executable(p))
}

/// What `bin` can be called on disk: Windows adds the extensions it runs.
fn executable_names(bin: &str) -> impl Iterator<Item = String> + '_ {
    let extensions: &[&str] =
        if cfg!(windows) && Path::new(bin).extension().is_none() { &[".exe", ".cmd", ".bat"] } else { &[""] };
    extensions.iter().map(move |e| format!("{bin}{e}"))
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

pub fn on_path(bin: &str) -> bool {
    which(bin).is_some()
}

/// Serializes tests that replace the search path: `cargo test` runs them at once.
#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Holds the search path on `dir` until dropped, one test at a time.
#[cfg(test)]
#[must_use]
pub(super) struct TestSearchPath {
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl TestSearchPath {
    pub(super) fn set(dir: PathBuf) -> Self {
        let lock = TEST_LOCK.locked();
        *SEARCH_PATH.locked() = vec![dir];
        Self { _lock: lock }
    }
}

#[cfg(test)]
impl Drop for TestSearchPath {
    fn drop(&mut self) {
        SEARCH_PATH.locked().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn detects_executables_on_search_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("codync-bin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("fake-harness");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let _path = TestSearchPath::set(dir.clone());
        assert!(which("fake-harness").is_none(), "not executable yet");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(which("fake-harness"), Some(bin));
    }

    #[test]
    fn hermes_install_dirs_are_searched() {
        let root = std::env::temp_dir().join(format!("codync-paths-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        let data_local = root.join("local");
        let dirs = well_known_dirs_for(&home, Some(&data_local));
        assert!(dirs.contains(&home.join(".local/bin")));
        if cfg!(windows) {
            assert!(dirs.contains(&data_local.join("hermes").join("bin")));
            assert!(dirs.contains(&data_local.join("hermes").join("hermes-agent").join(".hermes").join("bin")));
        }
    }
}
