//! What differs per system: when it booted, whether a saved pid is still the same process, ending one process.

#[cfg(unix)]
use std::process::Command;

/// Boot times read twice can differ a little (Windows derives it from an uptime counter).
const SAME_BOOT_WITHIN_MS: i64 = 120_000;

pub fn same_boot(saved: Option<i64>, now: Option<i64>) -> bool {
    saved.zip(now).is_some_and(|(saved, now)| (saved - now).abs() <= SAME_BOOT_WITHIN_MS)
}

/// When this computer booted. Blocking.
#[cfg(target_os = "linux")]
pub fn boot_ms() -> Option<i64> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let seconds: i64 = stat.lines().find_map(|line| line.strip_prefix("btime "))?.trim().parse().ok()?;
    Some(seconds * 1000)
}

/// When this computer booted. Blocking.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn boot_ms() -> Option<i64> {
    let out = Command::new("sysctl").args(["-n", "kern.boottime"]).output().ok()?;
    // { sec = 1793887393, usec = 0 } Thu Nov  5 ...
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let seconds: i64 = text.split("sec = ").nth(1)?.split(',').next()?.trim().parse().ok()?;
    Some(seconds * 1000)
}

/// When this computer booted.
#[cfg(windows)]
pub fn boot_ms() -> Option<i64> {
    // SAFETY: takes nothing and only reads a counter.
    let uptime = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
    Some(crate::store::now_ms() - i64::try_from(uptime).ok()?)
}

/// What tells this process from a later one with the same pid. Blocking.
#[cfg(unix)]
pub fn process_start(pid: u32) -> Option<String> {
    let out = Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().ok()?;
    let start = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!start.is_empty()).then_some(start)
}

/// Windows ends a job with the host that owns it, so nothing outlives it to check.
#[cfg(windows)]
pub fn process_start(_pid: u32) -> Option<String> {
    None
}

/// Ends what a crashed host left running, if `pid` is still that process. Blocking.
#[cfg(unix)]
pub fn kill_if_same(pid: u32, start: &str) {
    if process_start(pid).as_deref() != Some(start) {
        return;
    }
    let Ok(group) = libc::pid_t::try_from(pid) else { return };
    // SAFETY: only sends a signal; the check above says the group is still the server's.
    unsafe { libc::kill(-group, libc::SIGKILL) };
}

#[cfg(windows)]
pub fn kill_if_same(_pid: u32, _start: &str) {}

/// Asks one process to end.
#[cfg(unix)]
pub fn end_process(pid: u32) {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return };
    // SAFETY: only sends a signal to a pid a scan just found in the agent's tree.
    unsafe { libc::kill(pid, libc::SIGTERM) };
}

/// Ends one process.
#[cfg(windows)]
pub fn end_process(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    // SAFETY: the handle is checked, used once and closed; the pid came from a scan just now.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}
