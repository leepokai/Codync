//! Resume the suspended ACP shell only after its job owns future descendants.

use anyhow::{Context, Result, bail};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use tokio::process::Child;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

pub(super) fn resume(child: &Child) -> Result<()> {
    let pid = child.id().context("agent exited before it could be resumed")?;
    // SAFETY: this creates an owned thread snapshot; no pointers are supplied.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error()).context("finding suspended agent thread");
    }
    // SAFETY: a successful snapshot transfers a unique valid handle to us.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry = THREADENTRY32::default();
    entry.dwSize = u32::try_from(size_of_val(&entry))?;
    // SAFETY: the snapshot is valid and entry has the required initialized size.
    let mut present = unsafe { Thread32First(snapshot.as_raw_handle(), &raw mut entry) };
    while present != 0 {
        if entry.th32OwnerProcessID == pid {
            // The owned shell was created suspended and cannot have started more threads.
            // SAFETY: this opens that thread with only permission to resume it.
            let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if raw.is_null() {
                return Err(std::io::Error::last_os_error()).context("opening suspended agent thread");
            }
            // SAFETY: OpenThread transferred a unique valid handle to us.
            let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
            // SAFETY: we own a valid handle with THREAD_SUSPEND_RESUME rights.
            let count = unsafe { ResumeThread(thread.as_raw_handle()) };
            if count == u32::MAX {
                return Err(std::io::Error::last_os_error()).context("resuming agent thread");
            }
            if count != 1 {
                bail!("agent thread had an unexpected suspension count");
            }
            return Ok(());
        }
        // SAFETY: the snapshot and writable entry remain valid during iteration.
        present = unsafe { Thread32Next(snapshot.as_raw_handle(), &raw mut entry) };
    }
    bail!("couldn't find the suspended agent's initial thread")
}
