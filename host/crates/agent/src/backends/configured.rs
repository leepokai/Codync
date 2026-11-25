//! CLI paths a person sets for agents Codync can't find automatically.

use super::search_path::is_executable;
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

/// Agents whose setup sheet lets the user choose the command Codync runs over ACP.
const OVERRIDABLE: &[&str] = &["codex"];

/// Whether a person can save a CLI path for `id` and Codync will use it.
pub fn overridable(id: &str) -> bool {
    OVERRIDABLE.contains(&id)
}

/// The configured CLI for `id`, or the one found automatically.
pub fn effective(id: &str, detected: Option<PathBuf>) -> Option<PathBuf> {
    path(id).or(detected)
}

/// The saved CLI for `id`, if it's still runnable.
pub fn path(id: &str) -> Option<PathBuf> {
    path_from(&file(id))
}

/// Saves an explicit `path` for `id`, returning the absolute path written.
pub fn set_path(id: &str, path: &Path) -> Result<PathBuf> {
    ensure!(overridable(id), "{id} doesn't support a saved CLI path");
    let path = std::path::absolute(path).with_context(|| format!("resolving {}", path.display()))?;
    ensure!(is_executable(&path), "{} is not an executable file", path.display());
    let file = file(id);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&file, format!("{}\n", path.display()))?;
    Ok(path)
}

/// Goes back to automatic detection.
pub fn clear(id: &str) -> Result<bool> {
    ensure!(overridable(id), "{id} doesn't support a saved CLI path");
    match std::fs::remove_file(file(id)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn file(id: &str) -> PathBuf {
    crate::setup::get().data_dir.join("agent-paths").join(id)
}

fn path_from(file: &Path) -> Option<PathBuf> {
    let path = PathBuf::from(std::fs::read_to_string(file).ok()?.trim());
    (path.is_absolute() && is_executable(&path)).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    #[cfg(unix)]
    #[test]
    fn saves_only_a_runnable_path_and_clears_it() {
        let root = std::env::temp_dir().join(format!("codync-agent-path-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let cli = root.join("codex");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = root.join("saved");
        std::fs::write(&file, format!("{}\n", cli.display())).unwrap();
        assert_eq!(path_from(&file).or(Some(root.join("detected"))), Some(cli));

        std::fs::write(&file, "/missing/codex\n").unwrap();
        let detected = root.join("detected");
        assert_eq!(path_from(&file).or(Some(detected.clone())), Some(detected));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn effective_uses_detection_without_a_saved_path() {
        let detected = PathBuf::from("/opt/codex");
        assert_eq!(path_from(Path::new("/missing/saved")).or(Some(detected.clone())), Some(detected));
    }

    #[test]
    fn only_agents_whose_launch_accepts_a_cli_path_can_have_one() {
        assert!(overridable("codex"));
        assert!(!overridable("workbuddy"));
        assert!(clear("workbuddy").is_err());
    }
}
