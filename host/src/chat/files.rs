//! Immutable snapshots explicitly published by a bot, independent of its workspace.
use crate::chat::uploads::MAX_FILE;
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const CHUNK_SIZE: usize = 384 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SharedFile {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

/// Removes an unpublished snapshot on every failure, including a dropped actor.
pub struct Snapshot {
    pub meta: SharedFile,
    path: PathBuf,
    committed: bool,
}

impl Snapshot {
    pub fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn root(bot: &str) -> PathBuf {
    crate::service::data_dir().join("bots").join(bot).join("files")
}

fn path(root: &Path, id: &str) -> Result<PathBuf> {
    let id = uuid::Uuid::parse_str(id).context("invalid file id")?;
    Ok(root.join(id.to_string()))
}

/// Display names never control storage paths; hidden and extensionless files are valid.
pub fn name(value: &str) -> Result<String> {
    ensure!(!value.is_empty() && value.len() <= 200, "file name must be 1–200 bytes");
    ensure!(
        value != "." && value != ".." && !value.contains(['/', '\\', ':']) && !value.chars().any(char::is_control),
        "invalid file name"
    );
    Ok(value.to_owned())
}

fn open_regular(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    ensure!(std::fs::symlink_metadata(path)?.file_type().is_file(), "only regular files can be shared");
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path).context("opening the file")?;
    ensure!(file.metadata()?.is_file(), "only regular files can be shared");
    Ok(file)
}

/// Blocking, bounded-memory copy. Call on a blocking worker, never the bot actor.
pub fn snapshot(
    root: &Path,
    cwd: &Path,
    source: &str,
    display: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<Snapshot> {
    let original = cwd.join(source);
    let source = original.canonicalize().context("finding the file")?;
    let display = match display {
        Some(value) => name(value)?,
        None => name(original.file_name().and_then(|v| v.to_str()).context("file name is not UTF-8; provide name")?)?,
    };
    let mut input = open_regular(&source)?;
    let before = input.metadata()?;
    ensure!(before.len() <= MAX_FILE, "file exceeds 100 MiB");
    std::fs::create_dir_all(root).context("creating the shared file folder")?;
    let id = uuid::Uuid::new_v4().to_string();
    let final_path = path(root, &id)?;
    let partial = final_path.with_extension("partial");
    let mut owned = Snapshot {
        meta: SharedFile { id, name: display, size: 0, sha256: String::new() },
        path: partial.clone(),
        committed: false,
    };
    let mut output = OpenOptions::new().write(true).create_new(true).open(&partial)?;
    let (size, digest) = copy(&mut input, &mut output, cancelled)?;
    owned.meta.size = size;
    let after = input.metadata()?;
    ensure!(
        owned.meta.size == before.len() && after.len() == before.len() && after.modified()? == before.modified()?,
        "source file changed while sharing; retry"
    );
    output.sync_all()?;
    drop(output);
    ensure!(!cancelled.load(Ordering::Acquire), "file sharing cancelled");
    std::fs::rename(&partial, &final_path)?;
    owned.path = final_path;
    owned.meta.sha256 = digest;
    Ok(owned)
}

fn copy(
    input: &mut impl std::io::Read,
    output: &mut impl std::io::Write,
    cancelled: &AtomicBool,
) -> Result<(u64, String)> {
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = vec![0; CHUNK_SIZE];
    loop {
        ensure!(!cancelled.load(Ordering::Acquire), "file sharing cancelled");
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        ensure!(size <= MAX_FILE, "file exceeds 100 MiB");
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
    }
    Ok((size, digest_hex(&hash.finalize())))
}

pub fn digest_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Formatting into a String cannot fail.
        let _ = write!(value, "{byte:02x}");
    }
    value
}

pub fn read(root: &Path, meta: &SharedFile, offset: u64) -> Result<Vec<u8>> {
    ensure!(meta.size <= MAX_FILE && offset <= meta.size, "invalid file offset or size");
    let mut file = open_regular(&path(root, &meta.id)?)?;
    ensure!(file.metadata()?.len() == meta.size, "shared file is missing or damaged");
    file.seek(SeekFrom::Start(offset))?;
    let length = usize::try_from((meta.size - offset).min(CHUNK_SIZE as u64))?;
    let mut data = vec![0; length];
    file.read_exact(&mut data)?;
    Ok(data)
}

#[cfg(test)]
mod tests;
