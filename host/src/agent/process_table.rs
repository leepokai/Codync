//! Native process snapshots for Unix agent cleanup and background-work admission.

use std::collections::HashMap;
use std::io;

pub(super) type Groups = HashMap<libc::pid_t, libc::pid_t>;

struct ProcessInfo {
    pid: libc::pid_t,
    parent: libc::pid_t,
    running: bool,
}

pub(super) fn session_groups(session: libc::pid_t) -> io::Result<Groups> {
    let mut groups = Groups::new();
    for process in snapshot(Some(session))? {
        let pid = process.pid;
        // SAFETY: getsid only reads the process table; an exited or inaccessible PID returns -1.
        if pid <= 0 || !process.running || unsafe { libc::getsid(pid) } != session {
            continue;
        }
        // SAFETY: getpgid only reads the process table.
        let group = unsafe { libc::getpgid(pid) };
        if group > 0 {
            groups.insert(group, pid);
        }
    }
    Ok(groups)
}

pub(super) fn parent_links() -> io::Result<Vec<(u32, u32)>> {
    let processes = snapshot(None)?;
    Ok(processes
        .into_iter()
        .filter_map(|p| Some((u32::try_from(p.pid).ok()?, u32::try_from(p.parent).ok()?)))
        .collect())
}

#[cfg(target_os = "linux")]
fn snapshot(session: Option<libc::pid_t>) -> io::Result<Vec<ProcessInfo>> {
    let mut processes = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse().ok()) else { continue };
        // Cleanup need not read another user's protected /proc files outside its session.
        // SAFETY: getsid only reads the process table.
        if session.is_some_and(|session| unsafe { libc::getsid(pid) } != session) {
            continue;
        }
        let stat = match std::fs::read(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH) => {
                continue;
            }
            Err(error) => return Err(error),
        };
        let (running, parent) = stat_fields(&stat)?;
        processes.push(ProcessInfo { pid, parent, running });
    }
    Ok(processes)
}

#[cfg(any(target_os = "linux", test))]
fn stat_fields(stat: &[u8]) -> io::Result<(bool, libc::pid_t)> {
    // comm may contain spaces, parentheses, and non-UTF-8 bytes; the last ')' ends that field.
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "process stat has no state or parent");
    let end = stat.iter().rposition(|b| *b == b')').ok_or_else(invalid)?;
    let mut fields = stat[end + 1..].split(u8::is_ascii_whitespace).filter(|s| !s.is_empty());
    let state = fields.next().and_then(|s| s.first()).ok_or_else(invalid)?;
    let parent =
        fields.next().and_then(|s| std::str::from_utf8(s).ok()).and_then(|s| s.parse().ok()).ok_or_else(invalid)?;
    Ok((!matches!(state, b'Z' | b'X' | b'x'), parent))
}

#[cfg(target_os = "macos")]
fn snapshot(session: Option<libc::pid_t>) -> io::Result<Vec<ProcessInfo>> {
    let mut processes = Vec::new();
    for pid in pids()? {
        // SAFETY: getsid only reads the process table.
        if pid <= 0 || session.is_some_and(|session| unsafe { libc::getsid(pid) } != session) {
            continue;
        }
        // The short BSD flavor exposes just process identity and status without requiring the
        // caller to own unrelated processes; the full BSD flavor requires matching credentials.
        // SAFETY: zero is valid for every field of this plain C output structure.
        let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
        let bytes = i32::try_from(size_of_val(&info)).map_err(io::Error::other)?;
        // SAFETY: info is writable and its exact structure size is supplied for this flavor.
        // arg=1 also includes zombies, preserving parent links while ignoring them for cleanup.
        let read = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDT_SHORTBSDINFO, 1, std::ptr::from_mut(&mut info).cast(), bytes)
        };
        if read != bytes {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        processes.push(ProcessInfo {
            pid,
            parent: libc::pid_t::try_from(info.pbsi_ppid).map_err(io::Error::other)?,
            running: info.pbsi_status != libc::SZOMB,
        });
    }
    Ok(processes)
}

#[cfg(target_os = "macos")]
fn pids() -> io::Result<Vec<libc::pid_t>> {
    // SAFETY: null with zero size asks libproc for the number of process IDs needed.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return Err(io::Error::last_os_error());
    }
    let mut capacity = usize::try_from(count).map_err(io::Error::other)? + 128;
    for _ in 0..4 {
        let mut pids = vec![0; capacity];
        let bytes = i32::try_from(size_of_val(pids.as_slice())).map_err(io::Error::other)?;
        // SAFETY: pids is an initialized, aligned buffer of exactly bytes writable bytes.
        let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        if count <= 0 {
            return Err(io::Error::last_os_error());
        }
        let count = usize::try_from(count).map_err(io::Error::other)?;
        if count < capacity {
            pids.truncate(count);
            return Ok(pids);
        }
        capacity *= 2;
    }
    Err(io::Error::other("process list kept growing during enumeration"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_enumeration_finds_the_current_process_and_parent() {
        // SAFETY: these calls only read the current process identity.
        let (pid, parent, session) = unsafe { (libc::getpid(), libc::getppid(), libc::getsid(0)) };
        let listed = snapshot(Some(session)).unwrap();
        assert!(listed.iter().any(|p| p.pid == pid && p.parent == parent && p.running));
        assert!(!session_groups(session).unwrap().is_empty());
        assert!(parent_links().unwrap().contains(&(u32::try_from(pid).unwrap(), u32::try_from(parent).unwrap())));
    }

    #[test]
    fn linux_process_states_handle_complex_names_and_ignore_zombies() {
        assert_eq!(stat_fields(b"123 (name with ) and \xff) S 42 2 3").unwrap(), (true, 42));
        assert_eq!(stat_fields(b"123 (zombie) Z 1 2 3").unwrap(), (false, 1));
        assert_eq!(stat_fields(b"123 (dead) X 1 2 3").unwrap(), (false, 1));
        assert!(stat_fields(b"invalid").is_err());
    }
}
