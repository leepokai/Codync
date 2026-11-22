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
    let proposed = proposal(entry);
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

/// What an unfinished change's entry says happened to it. A real permission card is stored as `answered`, with the
/// chosen option in `selected` and its kind in `options`; older data wrote `denied`/`rejected` itself.
fn proposal(entry: &Entry) -> &'static str {
    match entry.data["status"].as_str().unwrap_or("unknown") {
        "failed" => "failed change",
        "denied" | "rejected" => "denied proposal",
        "pending" | "in_progress" => "pending proposal",
        "answered" => match chosen_kind(entry) {
            Some(kind) if kind.starts_with("reject") => "denied proposal",
            _ => "unconfirmed proposal",
        },
        _ => "unconfirmed proposal",
    }
}

/// The kind (`allow_once`, `allow_always`, `reject_once`, …) of the option the user chose on a permission card.
fn chosen_kind(entry: &Entry) -> Option<&str> {
    let selected = entry.data["selected"].as_str()?;
    let chosen =
        entry.data["options"].as_array()?.iter().find(|option| option["optionId"].as_str() == Some(selected))?;
    chosen["kind"].as_str()
}

/// Refuse traversal and Windows device/stream syntax before touching the filesystem.
fn resolve(path: &str, cwd: &Path, roots: &[PathBuf]) -> Result<(PathBuf, PathBuf), &'static str> {
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
    let canonical_root = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .find(|root| canonical.starts_with(root))
        .ok_or("outside allowed folders")?;
    Ok((canonical_root, canonical))
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
    let (root, path) = resolve(&file.path, cwd, roots)?;
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
    let (mut reader, before) = open_regular(&root, &path)?;
    let (read, sha) = hash_checked(&mut reader, &before, deadline)?;
    file.bytes = Some(read);
    file.sha256 = Some(sha);
    Ok(())
}

#[cfg(unix)]
fn unix_open_err() -> &'static str {
    if std::io::Error::last_os_error().raw_os_error() == Some(libc::ELOOP) {
        "outside allowed folders"
    } else {
        "file unavailable"
    }
}

#[cfg(unix)]
fn is_unix_symlink(parent: Option<&std::fs::File>, c_part: &std::ffi::CStr) -> bool {
    use std::os::unix::io::AsRawFd;

    let mut buf: [libc::c_char; 1] = [0];
    // SAFETY: `c_part` is NUL-terminated and `buf` holds 1 byte; `readlink`/`readlinkat` succeeds (`>= 0`) only on a symlink.
    let rc = unsafe {
        match parent {
            Some(dir) => libc::readlinkat(dir.as_raw_fd(), c_part.as_ptr(), buf.as_mut_ptr(), buf.len()),
            None => libc::readlink(c_part.as_ptr(), buf.as_mut_ptr(), buf.len()),
        }
    };
    rc >= 0
}

#[cfg(unix)]
fn open_unix_dir(parent: Option<&std::fs::File>, part: &std::ffi::OsStr) -> Result<std::fs::File, &'static str> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};

    let c_part = CString::new(part.as_bytes()).map_err(|_| "file unavailable")?;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: `c_part` is NUL-terminated and `parent` is an open directory fd when provided; `O_NOFOLLOW` blocks symlink traversal.
    let fd = unsafe {
        match parent {
            Some(dir) => libc::openat(dir.as_raw_fd(), c_part.as_ptr(), flags),
            None => libc::open(c_part.as_ptr(), flags),
        }
    };
    if fd < 0 {
        let err = std::io::Error::last_os_error().raw_os_error();
        // Linux returns `ENOTDIR` when `O_DIRECTORY | O_NOFOLLOW` meets a symlink instead of `ELOOP`.
        if err == Some(libc::ELOOP) || (err == Some(libc::ENOTDIR) && is_unix_symlink(parent, &c_part)) {
            return Err("outside allowed folders");
        }
        return Err("file unavailable");
    }
    // SAFETY: `fd` is a newly opened descriptor owned solely by this `File`.
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
pub(super) fn open_regular(root: &Path, path: &Path) -> Result<(std::fs::File, std::fs::Metadata), &'static str> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};

    let rel = path.strip_prefix(root).map_err(|_| "outside allowed folders")?;
    let mut components = rel.components();
    let Some(last) = components.next_back() else {
        return Err("not a regular file");
    };
    let mut dir = open_unix_dir(None, root.as_os_str())?;
    for comp in components {
        let Component::Normal(part) = comp else {
            return Err("outside allowed folders");
        };
        dir = open_unix_dir(Some(&dir), part)?;
    }
    let Component::Normal(leaf) = last else {
        return Err("outside allowed folders");
    };
    let leaf_c = CString::new(leaf.as_bytes()).map_err(|_| "file unavailable")?;
    // SAFETY: `dir` is an open directory fd inside `root`, `leaf_c` is NUL-terminated, and `O_NOFOLLOW | O_NONBLOCK` blocks leaf symlinks and FIFO stalls.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            leaf_c.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(unix_open_err());
    }
    // SAFETY: `fd` is a newly opened descriptor owned solely by this `File`.
    let reader = unsafe { std::fs::File::from_raw_fd(fd) };
    let before = reader.metadata().map_err(|_| "file unavailable")?;
    if !before.is_file() {
        return Err("not a regular file");
    }
    Ok((reader, before))
}

#[cfg(windows)]
pub(super) fn open_regular(root: &Path, path: &Path) -> Result<(std::fs::File, std::fs::Metadata), &'static str> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, VOLUME_NAME_DOS};

    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // Open reparse points themselves so a swapped final component is rejected by `is_file`.
    options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    let reader = options.open(path).map_err(|_| "file unavailable")?;
    let before = reader.metadata().map_err(|_| "file unavailable")?;
    if !before.is_file() {
        return Err("not a regular file");
    }
    let handle = reader.as_raw_handle();
    // SAFETY: `handle` is an open file handle owned by `reader`; passing null/0 queries the required UTF-16 length.
    let needed = unsafe { GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, VOLUME_NAME_DOS) };
    if needed == 0 {
        return Err("file unavailable");
    }
    let len = usize::try_from(needed).map_err(|_| "file unavailable")?;
    let mut buf = vec![0u16; len];
    // SAFETY: `handle` is valid and `buf` can hold `needed` UTF-16 code units including the trailing NUL.
    let written = unsafe { GetFinalPathNameByHandleW(handle, buf.as_mut_ptr(), needed, VOLUME_NAME_DOS) };
    if written == 0 || written >= needed {
        return Err("file unavailable");
    }
    let written = usize::try_from(written).map_err(|_| "file unavailable")?;
    let final_path = PathBuf::from(std::ffi::OsString::from_wide(&buf[..written]));
    if !final_path.starts_with(root) {
        return Err("outside allowed folders");
    }
    Ok((reader, before))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn open_regular(root: &Path, path: &Path) -> Result<(std::fs::File, std::fs::Metadata), &'static str> {
    let reader = std::fs::OpenOptions::new().read(true).open(path).map_err(|_| "file unavailable")?;
    let before = reader.metadata().map_err(|_| "file unavailable")?;
    if !before.is_file() {
        return Err("not a regular file");
    }
    let canonical = path.canonicalize().map_err(|_| "file unavailable")?;
    if !canonical.starts_with(root) {
        return Err("outside allowed folders");
    }
    Ok((reader, before))
}

fn hash_checked(
    reader: &mut std::fs::File,
    before: &std::fs::Metadata,
    deadline: Instant,
) -> Result<(u64, String), &'static str> {
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
    let mut sha = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut sha, "{byte:02x}").map_err(|_| "hash formatting failed")?;
    }
    Ok((read, sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Entry, EntryKind};
    use serde_json::json;

    fn card(data: serde_json::Value) -> Entry {
        Entry {
            id: "e1".into(),
            seq: 1,
            bot_id: "b1".into(),
            rev: 1,
            kind: EntryKind::Permission.as_str().to_owned(),
            turn: 1,
            data,
            created_at: 0,
            updated_at: 0,
        }
    }

    /// The shape the host really stores: `answered`, the chosen option in `selected`, its kind in `options`.
    #[test]
    fn a_denied_edit_keeps_its_reason() {
        let denied = card(json!({
            "status": "answered",
            "selected": "no",
            "options": [
                {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                {"optionId": "no", "name": "Deny", "kind": "reject_once"},
            ],
            "diffs": [{"path": "src/lib.rs", "isNew": false}],
        }));
        assert_eq!(paths(&denied), [("src/lib.rs".to_owned(), "denied proposal")]);
        let allowed = card(json!({
            "status": "answered",
            "selected": "yes",
            "options": [
                {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                {"optionId": "no", "name": "Deny", "kind": "reject_once"},
            ],
            "diffs": [{"path": "src/lib.rs", "isNew": false}],
        }));
        assert_eq!(paths(&allowed), [("src/lib.rs".to_owned(), "unconfirmed proposal")]);
        // Older data wrote the refusal itself.
        let older = card(json!({"status": "denied", "diffs": [{"path": "src/lib.rs"}]}));
        assert_eq!(paths(&older), [("src/lib.rs".to_owned(), "denied proposal")]);
    }
}
