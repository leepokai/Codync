//! The background service on Windows: the per-user Run key starts the host at
//! sign-in through `codync-hostw.exe`, which keeps it running without a console
//! window (`src/bin/codync-hostw.rs`).

use super::{current_exe, data_dir, run, wait_for_host_exit};
use anyhow::{Context, Result, bail, ensure};
use std::fmt::Write as _;
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

/// Whether the service runner (`codync-hostw.exe`) started this process; it sets `CODYNC_SERVICE`.
pub fn started_by_service() -> bool {
    std::env::var_os("CODYNC_SERVICE").is_some()
}

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
    // No inherited pipes: the supervisor never exits, and whoever ran `install` (the app) would
    // wait on them forever.
    let spawn = |flags| {
        Command::new(&program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
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
    // The Run key may carry arguments someone added themselves (`--bind 127.0.0.1`); an update
    // rewrites that line, so read them before overwriting it. Each word is quoted as Windows
    // expects, so an argument that holds a space survives the round trip.
    let carried = configured().map_or_else(|_| Vec::new(), |(_, args)| super::extra_args(&args));
    stop()?;
    let mut command = format!("\"{}\" serve --port {port}", supervisor.display());
    for arg in &carried {
        let _ = write!(command, " {}", quote_arg(arg));
    }
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
    Some((program.into(), split_args(args)))
}

/// The words of a command line, split the way Windows splits one for the process it starts
/// (`CommandLineToArgvW`): a quote groups words, and backslashes before a quote escape it.
/// A plain `split_whitespace` would cut an argument that holds a space into two.
fn split_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' => {
                let mut count = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    count += 1;
                }
                if chars.peek() == Some(&'"') {
                    // 2n backslashes and a quote: n backslashes, then the quote toggles quoting.
                    // 2n+1: n backslashes, then a literal quote.
                    for _ in 0..count / 2 {
                        word.push('\\');
                    }
                    if count % 2 == 1 {
                        word.push('"');
                        chars.next();
                    }
                } else {
                    for _ in 0..count {
                        word.push('\\');
                    }
                }
                started = true;
            }
            ' ' | '\t' if !quoted => {
                if started {
                    args.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(word);
    }
    args
}

/// One word, quoted so [`split_args`] reads it back unchanged.
fn quote_arg(arg: &str) -> String {
    let plain = |c: char| !matches!(c, ' ' | '\t' | '"');
    if !arg.is_empty() && arg.chars().all(plain) {
        return arg.to_owned();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut slashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            c => {
                out.extend(std::iter::repeat_n('\\', slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_arguments_someone_added() {
        let query = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    CodyncHost    REG_SZ    \"C:\\Program Files\\Codync\\codync-hostw.exe\" serve --port 19222 --bind 127.0.0.1\r\n\r\n";
        let (_, args) = parse_run_value(query).unwrap();
        assert_eq!(crate::service::extra_args(&args), ["--bind", "127.0.0.1"]);
    }

    #[test]
    fn reads_the_run_value() {
        let query = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    CodyncHost    REG_SZ    \"C:\\Program Files\\Codync\\codync-hostw.exe\" serve --port 19222\r\n\r\n";
        let (program, args) = parse_run_value(query).unwrap();
        assert_eq!(program, PathBuf::from("C:\\Program Files\\Codync\\codync-hostw.exe"));
        assert_eq!(args, ["serve", "--port", "19222"]);
    }

    #[test]
    fn keeps_an_argument_that_holds_a_space() {
        let query = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    CodyncHost    REG_SZ    \"C:\\Program Files\\Codync\\codync-hostw.exe\" serve --port 19222 --name \"My Bot\"\r\n\r\n";
        let (_, args) = parse_run_value(query).unwrap();
        assert_eq!(crate::service::extra_args(&args), ["--name", "My Bot"]);
    }

    #[test]
    fn writes_back_the_words_it_read() {
        let carried = ["--bind", "127.0.0.1", "--name", "My Bot", "C:\\a b\\", "quote\"inside", ""];
        let mut line = String::new();
        for arg in &carried {
            let _ = write!(line, " {}", quote_arg(arg));
        }
        assert_eq!(split_args(&line), carried);
    }

    #[test]
    fn splits_a_line_like_windows() {
        assert_eq!(split_args(" serve --port 19222"), ["serve", "--port", "19222"]);
        assert_eq!(split_args(r#" a "b c" d "#), ["a", "b c", "d"]);
        assert_eq!(split_args(r#"a "" b"#), ["a", "", "b"]);
        assert_eq!(split_args(r"C:\dir\file"), [r"C:\dir\file"]);
        // Two backslashes before the closing quote: one backslash, then the quote ends the word.
        assert_eq!(split_args(r#""C:\a b\\" x"#), [r"C:\a b\", "x"]);
        // A trailing run with no quote after it stays as written.
        assert_eq!(split_args(r"a\\ b"), [r"a\\", "b"]);
    }
}
