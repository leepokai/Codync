//! Host-owned folder browsing and recent workspace selections.
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn listing(store: &Store, path: Option<PathBuf>) -> Result<Value> {
    let mut result = list_dirs(path)?;
    result["recent"] = serde_json::to_value(store.recent_workspaces()?)?;
    Ok(result)
}

pub(super) fn select(store: &Store, path: &str) -> Result<Value> {
    if path.trim().is_empty() {
        bail!("choose a folder first");
    }
    let path = resolve(Path::new(path))?;
    // Check readability before recording a selection; never remember invalid paths.
    std::fs::read_dir(&path).with_context(|| format!("opening {}", path.display()))?;
    let path = path.to_string_lossy().into_owned();
    store.remember_workspace(&path)?;
    Ok(json!({"path": path}))
}

fn resolve(path: &Path) -> Result<PathBuf> {
    let text = path.to_string_lossy();
    let expanded = if text == "~" {
        dirs::home_dir().context("no home directory")?
    } else if let Some(tail) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        dirs::home_dir().context("no home directory")?.join(tail)
    } else {
        path.to_path_buf()
    };
    if !expanded.is_absolute() {
        bail!("enter an absolute folder path");
    }
    // Preserve symlink names the user chose, while rejecting nonexistent folders.
    if !expanded.is_dir() {
        bail!("folder does not exist: {}", expanded.display());
    }
    Ok(expanded)
}

/// Blocking (`std::fs`): call through `spawn_blocking`. An empty path lists the drives (Windows),
/// which is also the parent a drive root reports, so pickers can move between drives.
fn list_dirs(path: Option<PathBuf>) -> Result<Value> {
    let dir = match path {
        Some(p) if p.as_os_str().is_empty() && cfg!(windows) => return Ok(list_drives()),
        Some(p) if !p.as_os_str().is_empty() => resolve(&p)?,
        _ => dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))?,
    };
    let mut dirs: Vec<Value> = std::fs::read_dir(&dir)
        .with_context(|| format!("listing {}", dir.display()))?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let p = e.path();
            // `is_dir` follows symlinks, junctions and mount points; `file_type` would drop them.
            if name.starts_with('.') || !p.is_dir() {
                return None;
            }
            Some(json!({"name": name, "path": p.to_string_lossy(), "isGit": p.join(".git").exists()}))
        })
        .collect();
    dirs.sort_by_key(|d| d["name"].as_str().unwrap_or_default().to_lowercase());
    let parent = match dir.parent() {
        Some(p) => Some(p.to_string_lossy().into_owned()),
        None if cfg!(windows) => Some(String::new()),
        None => None,
    };
    Ok(json!({
        "path": dir.to_string_lossy(),
        "parent": parent,
        "isGit": dir.join(".git").exists(),
        "dirs": dirs,
        "ancestors": dir.ancestors().collect::<Vec<_>>().into_iter().rev().map(|p| json!({
            "path": p.to_string_lossy(),
            "name": p.file_name().map_or_else(|| p.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned()),
            "isGit": false,
        })).collect::<Vec<_>>(),
    }))
}

/// The drives that are mounted (`C:\`, `D:\`, …), as a listing with no path of its own.
fn list_drives() -> Value {
    let dirs: Vec<Value> = (b'A'..=b'Z')
        .map(|letter| format!("{}:\\", char::from(letter)))
        .filter(|root| Path::new(root).is_dir())
        .map(|root| json!({"name": root.trim_end_matches('\\'), "path": root, "isGit": false}))
        .collect();
    json!({"path": "", "parent": null, "isGit": false, "dirs": dirs})
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn folder_listing_follows_linked_folders() {
        let dir = std::env::temp_dir().join(format!("codync-dirs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::os::unix::fs::symlink(dir.join("real"), dir.join("linked")).unwrap();
        std::fs::write(dir.join("file"), "").unwrap();
        let listing = super::list_dirs(Some(dir.clone())).unwrap();
        let names: Vec<&str> = listing["dirs"].as_array().unwrap().iter().filter_map(|d| d["name"].as_str()).collect();
        assert_eq!(names, ["linked", "real"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn selection_is_validated_and_survives_reopening_store() {
        let root = std::env::temp_dir().join(format!("codync-workspace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("project")).unwrap();
        let db = root.join("test.db");
        let store = Store::open(&db).unwrap();
        assert!(select(&store, "").is_err());
        assert!(select(&store, "relative/path").is_err());
        assert!(select(&store, root.join("missing").to_str().unwrap()).is_err());
        assert!(store.recent_workspaces().unwrap().is_empty());
        let path = root.join("project").to_string_lossy().into_owned();
        assert_eq!(select(&store, &path).unwrap()["path"], path);
        #[cfg(unix)]
        {
            let spaced = root.join("project ");
            std::fs::create_dir(&spaced).unwrap();
            let spaced = spaced.to_string_lossy().into_owned();
            assert_eq!(select(&store, &spaced).unwrap()["path"], spaced);
            select(&store, &path).unwrap();
        }
        drop(store);
        let reopened = Store::open(&db).unwrap();
        let result = listing(&reopened, Some(root.clone())).unwrap();
        assert_eq!(result["recent"][0]["path"], path);
        assert_eq!(result["ancestors"].as_array().unwrap().last().unwrap()["path"], root.to_string_lossy().as_ref());
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
}
