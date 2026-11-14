//! The export file: a zip whose entries are listed and checked before anything is read. Paths that leave the
//! archive, links, secrets, nested archives and version-control or package folders are never extracted.

use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read as _;
use std::ops::Bound;
use std::path::{Component, Path};
use zip::ZipArchive;

/// More entries than this, or more bytes once inflated, and the file is refused.
const MAX_ENTRIES: usize = 5_000;
const MAX_BYTES: u64 = 500 * 1024 * 1024;

pub struct Archive {
    zip: ZipArchive<File>,
    /// Each safe file's path (below the archive's root folder, if it has one) → its zip index and size.
    files: BTreeMap<String, (usize, u64)>,
    /// Entries left out because of what they are (a secret, a nested archive…).
    pub skipped: Vec<String>,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mut zip = ZipArchive::new(file).context("this isn't a zip file")?;
        if zip.len() > MAX_ENTRIES {
            bail!("the file has more than {MAX_ENTRIES} entries");
        }
        let (mut files, mut skipped, mut total) = (BTreeMap::new(), Vec::new(), 0u64);
        for index in 0..zip.len() {
            let entry = zip.by_index(index)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name()?.into_owned();
            let Some(path) = entry.enclosed_name().filter(|_| !entry.is_symlink()) else {
                skipped.push(name);
                continue;
            };
            let path = path.to_string_lossy().replace('\\', "/");
            if is_noise(&path) {
                continue;
            }
            if is_refused(&path) {
                skipped.push(path);
                continue;
            }
            total = total.saturating_add(entry.size());
            if total > MAX_BYTES {
                bail!("the file holds more than 500 MB");
            }
            files.insert(path, (index, entry.size()));
        }
        Ok(Self { zip, files, skipped })
    }

    /// Whether the archive holds `manifest`; from then on every path is relative to the folder that holds it
    /// (the root, or the one top-level folder an export zipped with its folder has).
    pub fn rooted_at(&mut self, manifest: &str) -> bool {
        if self.files.contains_key(manifest) {
            return true;
        }
        let prefix =
            self.files.keys().find_map(|p| p.strip_suffix(manifest).filter(|r| is_folder(r))).map(str::to_owned);
        let Some(prefix) = prefix else { return false };
        self.files = std::mem::take(&mut self.files)
            .into_iter()
            .filter_map(|(path, at)| path.strip_prefix(&prefix).map(|p| (p.to_owned(), at)))
            .collect();
        true
    }

    pub fn has(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// One file, at most `max` bytes.
    pub fn read(&mut self, path: &str, max: u64) -> Result<Vec<u8>> {
        let &(index, size) = self.files.get(path).with_context(|| format!("the file has no {path}"))?;
        if size > max {
            bail!("{path} is larger than {} KB", max / 1024);
        }
        let mut bytes = Vec::new();
        self.zip.by_index(index)?.take(max + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max {
            bail!("{path} is larger than {} KB", max / 1024);
        }
        Ok(bytes)
    }

    /// The paths of the files in folder `dir`, below it.
    pub fn under(&self, dir: &str) -> Vec<String> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        self.files.keys().filter_map(|p| p.strip_prefix(&prefix)).map(str::to_owned).collect()
    }

    /// Copies folder `dir` into `dest`, keeping the paths below it; returns how many files it copied.
    pub fn extract(&mut self, dir: &str, dest: &Path) -> Result<usize> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        let mut copied = 0;
        let from = (Bound::Included(prefix.as_str()), Bound::Unbounded);
        let below = self.files.range::<str, _>(from).take_while(|(path, _)| path.starts_with(&prefix));
        for (path, &(index, size)) in below {
            let target = dest.join(&path[prefix.len()..]);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target).with_context(|| format!("writing {}", target.display()))?;
            let written = std::io::copy(&mut self.zip.by_index(index)?.take(size), &mut out)?;
            if written != size {
                bail!("{path} is cut short");
            }
            copied += 1;
        }
        Ok(copied)
    }
}

/// `rest` (what's before a manifest's name) is one top-level folder.
fn is_folder(rest: &str) -> bool {
    rest.strip_suffix('/').is_some_and(|f| !f.is_empty() && !f.contains('/'))
}

/// What a Mac's zip adds, left out without a word.
fn is_noise(path: &str) -> bool {
    path.starts_with("__MACOSX/") || path.rsplit('/').next() == Some(".DS_Store")
}

/// Secrets, nested archives and version-control or package folders: never copied onto this computer.
fn is_refused(path: &str) -> bool {
    let parts: Vec<&str> = Path::new(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => p.to_str(),
            _ => None,
        })
        .collect();
    let file = parts.last().copied().unwrap_or_default().to_lowercase();
    let archive = [".zip", ".tar", ".gz", ".tgz", ".7z", ".rar"].iter().any(|ext| file.ends_with(ext));
    // Case-insensitive file systems (APFS, NTFS) treat `.GIT` as `.git`.
    parts.iter().any(|p| p.eq_ignore_ascii_case(".git") || p.eq_ignore_ascii_case("node_modules"))
        || file.starts_with(".env")
        || (file.contains("private") && file.contains("key"))
        || archive
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_nested_archives_are_refused() {
        for path in [
            "a/.env",
            "a/.env.local",
            "a/id_private_key",
            "a/repo/.git/HEAD",
            "a/.GIT/config",
            "a/node_modules/x/i.js",
            "a/b.zip",
        ] {
            assert!(is_refused(path), "{path}");
        }
        for path in ["a/notes.md", "a/keys.md", "a/environment.txt"] {
            assert!(!is_refused(path), "{path}");
        }
        assert!(is_noise("__MACOSX/a/._b") && is_noise("a/.DS_Store"));
    }

    #[test]
    fn a_manifest_in_one_folder_counts() {
        assert!(is_folder("export/"));
        assert!(!is_folder("") && !is_folder("a/b/") && !is_folder("export"));
    }
}
