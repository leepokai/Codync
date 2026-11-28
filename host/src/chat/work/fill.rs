//! Filling a new worktree before its thread starts ([filling a new worktree](../../../../docs/features/work.md#filling-a-new-worktree)):
//! a worktree is a clean checkout, without the main checkout's ignored files. The files `.worktreeinclude` lists at
//! the repository's top are copied in, and the project's shared folders (`BotConfig::shared_folders`) linked, so the
//! first turn finds its `.env` and `node_modules`. Each link is listed in the repository's `info/exclude`, so git never
//! shows it and a thread can't commit it (merged, a link to the main checkout's own folder would replace it), and is
//! taken out before an untouched worktree goes ([`unlink`]). The user's uncommitted changes never are copied: the
//! worktree starts from the last commit, and the thread is told so (`Worktree::brief`). A file or folder that can't be copied or
//! linked is logged and counted (`worktree_fill_failed`), never fatal.

use super::worktree::{Worktree, top};
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::hub::projects::shared_folder_name;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The list of ignored files to copy, at the top of the main checkout.
const INCLUDE: &str = ".worktreeinclude";
/// How long one git look at a path may take.
const LIMIT: Duration = Duration::from_secs(30);
/// Where the repository's `info/exclude` is, from any of its checkouts (worktrees share it).
const EXCLUDE: [&str; 4] = ["rev-parse", "--path-format=absolute", "--git-path", "info/exclude"];

/// Where filling a worktree failed (`worktree_fill_failed`'s `stage`).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum Stage {
    /// Copying a file `.worktreeinclude` lists.
    Include,
    /// Linking a shared folder.
    Link,
}

/// Fills `worktree`, just made from the repository `folder` is in, for a thread of `bot`'s.
pub(super) async fn fill(hub: &Arc<Hub>, bot: &str, folder: &str, worktree: &Worktree) {
    let Some(main) = top(folder).map(Path::to_path_buf) else { return };
    let shared = hub.store.bot(bot).ok().flatten().map(|row| row.config.shared_folders).unwrap_or_default();
    let copies = included(&main).await;
    // No link where git can't be told to pass over it.
    let (links, exclude) = match crate::git::run(&main, &EXCLUDE, LIMIT).await {
        Ok(exclude) => (shared_in(&main, &shared), PathBuf::from(exclude)),
        Err(error) => {
            tracing::warn!(error = format!("{error:#}"), "no info/exclude: shared folders aren't linked");
            (Vec::new(), PathBuf::new())
        }
    };
    let new = PathBuf::from(&worktree.path);
    let failed = tokio::task::spawn_blocking(move || place(&main, &new, &copies, &links, &exclude)).await;
    let failed = failed.unwrap_or_else(|error| vec![(Stage::Include, std::io::Error::other(error))]);
    for (stage, error) in failed {
        tracing::warn!(worktree = worktree.path, ?stage, error = %error, "couldn't fill a new worktree");
        let properties = serde_json::json!({"stage": stage});
        analytics::capture(hub, Event::WorktreeFillFailed, properties.as_object().cloned().unwrap_or_default());
    }
}

/// What `.worktreeinclude` at the top of `main` lists that is there and ignored (git's `check-ignore` passes over
/// tracked paths too): one path a line, `#` for a comment. A path that is absolute, leaves the repository, is missing,
/// tracked or not ignored is passed over.
pub(super) async fn included(main: &Path) -> Vec<PathBuf> {
    let Ok(list) = tokio::fs::read_to_string(main.join(INCLUDE)).await else { return Vec::new() };
    let mut copies = Vec::new();
    for line in list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some(path) = shared_folder_name(line) else {
            tracing::info!(line, "a .worktreeinclude path outside the repository: passed over");
            continue;
        };
        let ignored = tokio::fs::symlink_metadata(main.join(&path)).await.is_ok()
            && crate::git::run(main, &["check-ignore", "-q", "--", &path], LIMIT).await.is_ok();
        if ignored {
            copies.push(PathBuf::from(path));
        } else {
            tracing::info!(line, "a .worktreeinclude path that is missing, tracked or not ignored: passed over");
        }
    }
    copies
}

/// The shared folders of `shared`'s projects in the repository at `main`, relative to its top.
pub(super) fn shared_in(main: &Path, shared: &BTreeMap<String, Vec<String>>) -> Vec<PathBuf> {
    let in_repo = shared.iter().filter(|(project, _)| top(project) == Some(main));
    in_repo
        .filter_map(|(project, names)| Some((Path::new(project).strip_prefix(main).ok()?, names)))
        .flat_map(|(within, names)| names.iter().filter_map(|name| Some(within.join(shared_folder_name(name)?))))
        .collect()
}

/// Copies `copies` and links `links` (paths relative to the checkouts' tops) from the main checkout `main` into the
/// worktree `new`, where nothing is there yet, each link listed in the repository's `exclude` file. What failed, each
/// with why.
pub(super) fn place(
    main: &Path,
    new: &Path,
    copies: &[PathBuf],
    links: &[PathBuf],
    exclude: &Path,
) -> Vec<(Stage, std::io::Error)> {
    let mut failed = Vec::new();
    // Already there (a tracked folder of the same name): it stays as the commit has it.
    let open = |path: &&PathBuf| new.join(path).symlink_metadata().is_err();
    let links: Vec<_> = links.iter().filter(open).filter(|path| main.join(path).is_dir()).collect();
    // Listed before they are made: a link git would show could be committed.
    let links = match excluded(exclude, &links) {
        Ok(()) => links,
        Err(error) => {
            failed.push((Stage::Link, error));
            Vec::new()
        }
    };
    let copies = copies.iter().filter(open).map(|path| (Stage::Include, path));
    for (stage, path) in copies.chain(links.into_iter().map(|path| (Stage::Link, path))) {
        let (from, to) = (main.join(path), new.join(path));
        let made = to.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| match stage {
            Stage::Include => copy_all(&from, &to),
            Stage::Link => link(&from, &to),
        });
        if let Err(error) = made {
            failed.push((stage, error));
        }
    }
    failed
}

/// Lists `links` in the repository's `exclude` file as paths from the checkout's top (`/node_modules`: no trailing
/// slash, which a link wouldn't match), those it doesn't list yet.
fn excluded(exclude: &Path, links: &[&PathBuf]) -> std::io::Result<()> {
    let listed = std::fs::read_to_string(exclude).unwrap_or_default();
    let missing: String = links
        .iter()
        .map(|link| format!("/{}", link.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join("/")))
        .filter(|line| !listed.lines().any(|l| l == line))
        .map(|line| line + "\n")
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    exclude.parent().map_or(Ok(()), std::fs::create_dir_all)?;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(exclude)?;
    let newline = if listed.is_empty() || listed.ends_with('\n') { "" } else { "\n" };
    file.write_all(format!("{newline}{missing}").as_bytes())
}

/// Takes the shared folders [`place`] linked out of worktree `dir` of the repository whose main checkout is `main`
/// before it goes (what they point to is the main checkout's and stays): each path the repository's `info/exclude`
/// lists that is a link to the same path in `main`, as [`place`] makes them.
pub(super) async fn unlink(dir: &Path, main: &Path) -> anyhow::Result<()> {
    let exclude = crate::git::run(dir, &EXCLUDE, LIMIT).await?;
    let listed = tokio::fs::read_to_string(exclude).await.unwrap_or_default();
    for name in listed.lines().filter_map(|line| line.strip_prefix('/')) {
        let path = dir.join(name);
        // Compared as real paths: the project's folder may have been named through a link (`/var`, `/private/var`).
        let ours = match tokio::fs::read_link(&path).await {
            Ok(to) if to.is_absolute() => {
                let to = tokio::fs::canonicalize(to).await.ok();
                to.is_some() && to == tokio::fs::canonicalize(main.join(name)).await.ok()
            }
            _ => false,
        };
        // A link to a folder is a file on Unix, a folder on Windows.
        if ours && tokio::fs::remove_file(&path).await.is_err() {
            tokio::fs::remove_dir(&path).await?;
        }
    }
    Ok(())
}

/// Copies the file or folder `from` to `to`.
fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.is_dir() {
        return std::fs::copy(from, to).map(drop);
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy_all(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}

/// Needs Developer Mode or the right to make links; without it, a failure counted like any other.
#[cfg(windows)]
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(from, to)
}
