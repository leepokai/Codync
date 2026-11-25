//! File evidence is inspected only on the blocking worker, within the bot's allowed folders.
use super::{Entry, EntryKind, FILES, Touched};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const HASH_BYTES: u64 = 64 * 1024 * 1024;
const HASH_TIME: Duration = Duration::from_secs(1);

pub(super) fn collect(entries: &[Entry], cwd: &Path, extra: &[PathBuf], omitted: &mut Vec<String>) -> Vec<Touched> {
    let roots: Vec<_> = std::iter::once(cwd).chain(extra.iter().map(PathBuf::as_path)).map(Path::to_path_buf).collect();
    let deadline = Instant::now() + HASH_TIME;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries.iter().rev() {
        for (path, why) in paths(entry) {
            // Keep a failed proposal separate from an earlier completed change of the same file.
            if !seen.insert((path.clone(), why)) {
                continue;
            }
            let mut file = Touched { path, why, bytes: None, sha256: None, unavailable: None };
            inspect(&mut file, cwd, &roots, out.len() < FILES, deadline);
            if out.len() < FILES {
                out.push(file);
            } else {
                omitted.push(format!("{} (collection limit)", super::render::file_label(&file)));
            }
        }
    }
    out
}

fn paths(entry: &Entry) -> Vec<(String, &'static str)> {
    let tool = entry.kind == EntryKind::Tool.as_str();
    let permission = entry.kind == EntryKind::Permission.as_str();
    if !tool && !permission {
        return Vec::new();
    }
    let completed = tool && entry.data["status"] == "completed";
    let state = entry.data["status"].as_str().unwrap_or("unknown");
    let proposed = match state {
        "failed" => "failed change",
        "denied" | "rejected" => "denied proposal",
        "pending" | "in_progress" => "pending proposal",
        _ => "unconfirmed proposal",
    };
    let mut paths: Vec<_> = entry.data["diffs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|diff| {
            let why = if !completed {
                proposed
            } else if diff["isNew"] == true {
                "created"
            } else {
                "modified"
            };
            Some((diff["path"].as_str()?.to_owned(), why))
        })
        .collect();
    // Locations alone do not prove a read: edit tools also report locations.
    if completed && entry.data["toolKind"] == "read" {
        paths.extend(
            entry.data["locations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| Some((l["path"].as_str()?.to_owned(), "read"))),
        );
        if paths.is_empty()
            && let Some(path) = entry.data["title"].as_str().and_then(|s| s.strip_prefix("Read "))
        {
            paths.push((path.trim_matches(['"', '\'', char::from(96)]).to_owned(), "read"));
        }
    }
    paths
}

/// Refuse traversal and Windows device/stream syntax before touching the filesystem.
fn resolve(path: &str, cwd: &Path, roots: &[PathBuf]) -> Result<PathBuf, &'static str> {
    let path = Path::new(path);
    if path.components().any(|c| c == Component::ParentDir) {
        return Err("outside allowed folders");
    }
    #[cfg(windows)]
    {
        let text = path.as_os_str().to_string_lossy();
        if text.starts_with(r"\\") || text.chars().skip(2).any(|c| c == ':') {
            return Err("device, network or stream path");
        }
    }
    let candidate = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
    // The lexical boundary rejects outside paths before metadata (including pipes/devices).
    let lexical =
        std::iter::once(cwd).chain(roots.iter().map(PathBuf::as_path)).any(|root| candidate.starts_with(root));
    if !lexical {
        return Err("outside allowed folders");
    }
    let meta = std::fs::symlink_metadata(&candidate).map_err(|_| "file unavailable")?;
    if !meta.file_type().is_file() {
        return Err("not a regular file");
    }
    let canonical = candidate.canonicalize().map_err(|_| "file unavailable")?;
    if !roots.iter().any(|root| root.canonicalize().is_ok_and(|root| canonical.starts_with(root))) {
        return Err("outside allowed folders");
    }
    Ok(canonical)
}

fn inspect(file: &mut Touched, cwd: &Path, roots: &[PathBuf], hash: bool, deadline: Instant) {
    match inspect_inner(file, cwd, roots, hash, deadline) {
        Ok(()) => {}
        Err(reason) => file.unavailable = Some(reason),
    }
}

fn inspect_inner(
    file: &mut Touched,
    cwd: &Path,
    roots: &[PathBuf],
    hash: bool,
    deadline: Instant,
) -> Result<(), &'static str> {
    if Instant::now() >= deadline {
        return Err("inspection skipped: time limit");
    }
    let path = resolve(&file.path, cwd, roots)?;
    let meta = path.metadata().map_err(|_| "file unavailable")?;
    file.bytes = Some(meta.len());
    if !hash {
        return Ok(());
    }
    if meta.len() > HASH_BYTES {
        return Err("hash skipped: 64 MiB limit");
    }
    if Instant::now() >= deadline {
        return Err("hash skipped: time limit");
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // A replaced FIFO must not block open, even after the initial regular-file check.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut reader = options.open(&path).map_err(|_| "file unavailable")?;
    let before = reader.metadata().map_err(|_| "file unavailable")?;
    if !before.is_file() {
        return Err("not a regular file");
    }
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut read = 0u64;
    loop {
        if Instant::now() >= deadline {
            return Err("hash skipped: time limit");
        }
        let n = reader.read(&mut buf).map_err(|_| "file read failed")?;
        if n == 0 {
            break;
        }
        read += u64::try_from(n).unwrap_or(u64::MAX);
        if read > HASH_BYTES {
            return Err("hash skipped: 64 MiB limit");
        }
        hasher.update(&buf[..n]);
    }
    let after = reader.metadata().map_err(|_| "file unavailable")?;
    if read != before.len() || before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("file changed while hashing");
    }
    file.bytes = Some(read);
    let mut sha = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut sha, "{byte:02x}").map_err(|_| "hash formatting failed")?;
    }
    file.sha256 = Some(sha);
    Ok(())
}
