//! The background service on Windows: the per-user Run key starts the host at
//! sign-in through `codync-hostw.exe`, which keeps it running without a console
//! window (`src/bin/codync-hostw.rs`).

use super::{current_exe, data_dir, run, wait_for_host_exit};
use anyhow::{Context, Result, bail, ensure};
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = crate::environment::Environment::current().windows_value();
const SUPERVISOR: &str = crate::environment::Environment::current().supervisor();
const HOST: &str = "codync-host.exe";

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
/// Out of the job of whoever ran `install` (Node puts its children in a job that
/// dies with it, and the service must outlive the app that installed it).
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// Stops the running service. Its configuration stays for a later start; a
/// manually started `codync-host serve` is left alone.
pub fn stop() -> Result<()> {
    // Not running is fine; `/T` takes the host and its agents with the supervisor.
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/IM", SUPERVISOR])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    wait_for_host_exit()
}

pub fn start() -> Result<()> {
    let (program, args) = configured()?;
    let spawn = |flags| Command::new(&program).args(&args).creation_flags(flags).spawn();
    let detached = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    spawn(detached | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(detached))
        .with_context(|| format!("starting {}", program.display()))?;
    Ok(())
}

/// Refuse to replace one installation while restarting a service owned by another.
pub fn require_executable(expected: &Path) -> Result<()> {
    let (supervisor, _) = configured()?;
    if supervisor.with_file_name(HOST).canonicalize()? != expected.canonicalize()? {
        bail!("the installed service runs another host binary; run that binary's update command");
    }
    Ok(())
}

/// Registers the host to start at sign-in and starts it now.
pub fn install(port: u16) -> Result<()> {
    let supervisor = current_exe().context("locating the codync-host binary")?.with_file_name(SUPERVISOR);
    ensure!(supervisor.is_file(), "{} is missing; reinstall Codync", supervisor.display());
    std::fs::create_dir_all(data_dir()).context("creating the data directory")?;
    stop()?;
    let command = format!("\"{}\" serve --port {port}", supervisor.display());
    run("reg", &["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &command, "/f"])?;
    start()
}

/// Best effort: every step tolerates "already gone".
pub fn uninstall() {
    let _ = Command::new("reg").args(["delete", RUN_KEY, "/v", VALUE, "/f"]).stderr(Stdio::null()).status();
    let _ = stop();
}

pub fn installed() -> bool {
    configured().is_ok()
}

/// The supervisor and arguments the Run key starts.
fn configured() -> Result<(PathBuf, Vec<String>)> {
    let output = Command::new("reg").args(["query", RUN_KEY, "/v", VALUE]).output().context("running reg")?;
    ensure!(output.status.success(), "the host service isn't installed");
    parse_run_value(&String::from_utf8_lossy(&output.stdout)).context("cannot read the installed host service")
}

/// `    CodyncHost    REG_SZ    "C:\…\codync-hostw.exe" serve --port 19222` → its parts.
fn parse_run_value(query: &str) -> Option<(PathBuf, Vec<String>)> {
    let value = query.lines().find_map(|line| Some(line.split_once("REG_SZ")?.1.trim()))?;
    let (program, args) = value.strip_prefix('"')?.split_once('"')?;
    Some((program.into(), args.split_whitespace().map(str::to_owned).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_run_value() {
        let query = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    CodyncHost    REG_SZ    \"C:\\Program Files\\Codync\\codync-hostw.exe\" serve --port 19222\r\n\r\n";
        let (program, args) = parse_run_value(query).unwrap();
        assert_eq!(program, PathBuf::from("C:\\Program Files\\Codync\\codync-hostw.exe"));
        assert_eq!(args, ["serve", "--port", "19222"]);
    }
}
