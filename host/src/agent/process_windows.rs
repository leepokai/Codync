//! Resume the suspended ACP shell only after its job owns future descendants; list what the job holds and end one of it.

use super::process::JobProcess;
use anyhow::{Context, Result, bail};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use tokio::process::Child;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD,
    THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    IsProcessInJob, JOBOBJECT_BASIC_PROCESS_ID_LIST, JobObjectBasicProcessIdList, QueryInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, ResumeThread, THREAD_SUSPEND_RESUME,
    TerminateProcess,
};

/// More processes than any agent runs; a job past it fails the listing, which counts as busy.
const MOST_PROCESSES: usize = 1024;

/// A snapshot of the system's `flags` (processes, threads).
fn snapshot(flags: u32) -> std::io::Result<OwnedHandle> {
    // SAFETY: this creates an owned snapshot; no pointers are supplied.
    let raw = unsafe { CreateToolhelp32Snapshot(flags, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a successful snapshot transfers a unique valid handle to us.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

/// The ids of the processes in `job`.
pub(super) fn job_members(job: &OwnedHandle) -> Result<Vec<u32>> {
    let header = size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>().div_ceil(size_of::<usize>());
    // usize: the list's own alignment.
    let mut buffer = vec![0_usize; header + MOST_PROCESSES];
    let size = u32::try_from(buffer.len() * size_of::<usize>())?;
    // SAFETY: the job handle is owned and valid, and the buffer is writable for `size` bytes.
    let queried = unsafe {
        QueryInformationJobObject(
            job.as_raw_handle(),
            JobObjectBasicProcessIdList,
            buffer.as_mut_ptr().cast(),
            size,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(std::io::Error::last_os_error()).context("listing the agent's job");
    }
    // SAFETY: the query wrote a whole list at the buffer's start, aligned for it.
    let list = unsafe { &*buffer.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
    if list.NumberOfProcessIdsInList != list.NumberOfAssignedProcesses {
        bail!("the agent's job holds more processes than its list");
    }
    let count = usize::try_from(list.NumberOfProcessIdsInList)?;
    let first = std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList) / size_of::<usize>();
    buffer[first..first + count].iter().map(|id| Ok(u32::try_from(*id)?)).collect()
}

/// `members` with their parents and program names; one that exited since is left out.
pub(super) fn named(members: &[u32]) -> Result<Vec<JobProcess>> {
    let snapshot = snapshot(TH32CS_SNAPPROCESS).context("listing processes")?;
    let mut entry = PROCESSENTRY32W::default();
    entry.dwSize = u32::try_from(size_of_val(&entry))?;
    let mut processes = Vec::new();
    // SAFETY: the snapshot is valid and entry has the required initialized size.
    let mut present = unsafe { Process32FirstW(snapshot.as_raw_handle(), &raw mut entry) };
    while present != 0 {
        if members.contains(&entry.th32ProcessID) {
            let end = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            processes.push(JobProcess {
                pid: entry.th32ProcessID,
                parent: entry.th32ParentProcessID,
                name: String::from_utf16_lossy(&entry.szExeFile[..end]),
            });
        }
        // SAFETY: the snapshot and writable entry remain valid during iteration.
        present = unsafe { Process32NextW(snapshot.as_raw_handle(), &raw mut entry) };
    }
    Ok(processes)
}

/// Ends `pid` if it is still in `job`. The handle held meanwhile keeps its id from going to another process.
pub(super) fn end(job: &OwnedHandle, pid: u32) -> Result<()> {
    // SAFETY: this opens the process with only the rights to check its job and end it.
    let raw = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error()).context("opening a process of the job");
    }
    // SAFETY: OpenProcess transferred a unique valid handle to us.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut inside = 0;
    // SAFETY: both handles are owned and valid, and `inside` is writable.
    if unsafe { IsProcessInJob(process.as_raw_handle(), job.as_raw_handle(), &raw mut inside) } == 0 {
        return Err(std::io::Error::last_os_error()).context("checking a process's job");
    }
    // SAFETY: the handle has PROCESS_TERMINATE rights and names a process of the job.
    if inside != 0 && unsafe { TerminateProcess(process.as_raw_handle(), 1) } == 0 {
        return Err(std::io::Error::last_os_error()).context("ending a process of the job");
    }
    Ok(())
}

pub(super) fn resume(child: &Child) -> Result<()> {
    let pid = child.id().context("agent exited before it could be resumed")?;
    let snapshot = snapshot(TH32CS_SNAPTHREAD).context("finding suspended agent thread")?;
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
