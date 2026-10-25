//! Command lines: the one place Unix and Windows differ in how agents, setup
//! terminals and wrapped status lines start, and in how a link opens. Unix runs a
//! line with `sh -c`, Windows with `cmd /c`.
//!
//! Windows hands the line to `cmd` through an environment variable: `cmd`
//! expands `%VAR%` before it parses, so the line's own quotes survive whatever
//! argument quoting the spawner (std, tokio, the PTY) applies to `cmd`'s arguments.

use std::fmt::Write as _;
use std::path::PathBuf;

/// A program, its arguments and extra environment that run one command line.
pub struct Shell {
    pub program: &'static str,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Shell {
    pub fn std(&self) -> std::process::Command {
        let mut command = std::process::Command::new(self.program);
        command.args(&self.args).envs(self.env.iter().map(|(k, v)| (k, v)));
        command
    }

    pub fn tokio(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(self.program);
        command.args(&self.args).envs(self.env.iter().map(|(k, v)| (k, v)));
        command
    }
}

#[cfg(unix)]
pub fn line(line: &str) -> Shell {
    Shell { program: "/bin/sh", args: vec!["-c".into(), line.into()], env: vec![] }
}

#[cfg(windows)]
const LINE_VAR: &str = "CODYNC_COMMAND_LINE";

/// `cmd` has no `KEY=value cmd` syntax, so leading assignments become the environment.
#[cfg(windows)]
pub fn line(line: &str) -> Shell {
    let (mut env, rest) = split_env(line);
    env.push((LINE_VAR.into(), rest.into()));
    Shell { program: "cmd.exe", args: vec!["/d".into(), "/c".into(), format!("%{LINE_VAR}%")], env }
}

/// Runs `command` in place of the shell where the shell can do that (`exec`), so
/// signals and exit codes reach the program itself.
pub fn exec(command: &str) -> Shell {
    if cfg!(windows) { line(command) } else { line(&format!("exec {command}")) }
}

/// The PATH a fresh login would get, for finding CLIs installed after this process started.
/// Unix: the login shell's, via `$SHELL -ilc` with a timeout (nvm/conda can make shells
/// slow); the shell writes it to a file, since a background job started by an rc file can hold
/// a stdout pipe open long after the shell exits.
#[cfg(unix)]
pub fn user_path() -> Option<String> {
    use std::time::{Duration, Instant};
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let file = std::env::temp_dir().join(format!("codync-path-{}", uuid::Uuid::new_v4()));
    let target = file.to_string_lossy().replace('\'', r"'\''");
    let mut child = std::process::Command::new(shell)
        .args(["-ilc", &format!("printf '%s' \"$PATH\" > '{target}'")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(8);
    let exited = loop {
        match child.try_wait() {
            Ok(Some(_)) => break true,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                break false;
            }
        }
    };
    let path = std::fs::read_to_string(&file).ok().filter(|p| exited && !p.is_empty());
    let _ = std::fs::remove_file(&file);
    path
}

/// Windows: the user's and the machine's PATH from the registry. A running process keeps the
/// PATH it started with, so a CLI whose installer just added its folder is only seen there.
#[cfg(windows)]
pub fn user_path() -> Option<String> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let read = "[Environment]::GetEnvironmentVariable('Path','User') + ';' + [Environment]::GetEnvironmentVariable('Path','Machine')";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", read])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !path.is_empty()).then_some(path)
}

/// Quotes one word for [`line`].
pub fn quote(s: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_./=@:+,".contains(c) || (cfg!(windows) && c == '\\');
    if !s.is_empty() && s.chars().all(plain) {
        s.to_owned()
    } else if cfg!(windows) {
        // `cmd` has no escape for a quote inside quotes; Windows paths can't hold one.
        format!("\"{}\"", s.replace('"', ""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// `KEY=value ` assignments before a command. Unix goes through `env` so the result
/// still works after `exec`; Windows' [`line`] lifts them into the environment.
pub fn env_prefix<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut out = String::new();
    for (k, v) in pairs {
        if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            let _ = write!(out, "{k}={} ", quote(v));
        }
    }
    let pairs = out;
    if pairs.is_empty() || cfg!(windows) { pairs } else { format!("env {pairs}") }
}

/// Opens a link or file with this computer's default app for it (a web page: the browser). Whether it started.
pub fn open(target: &std::ffi::OsStr) -> bool {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" })
    };
    let opener = command
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    // Waited for, so the host, which runs for days, keeps no exited opener around.
    opener.map(|mut opener| std::thread::spawn(move || opener.wait())).is_ok()
}

/// `program` as spawnable: its full path when it's on the search path. Windows
/// needs this for npm's `.cmd` shims, which `CreateProcess` won't find by name.
pub fn program(name: &str) -> PathBuf {
    crate::agent::backends::which(name).unwrap_or_else(|| name.into())
}

/// The command that runs a Python test fixture.
#[cfg(test)]
pub fn python(script: &std::path::Path) -> String {
    let python = if cfg!(windows) { "python" } else { "python3" };
    format!("{python} -u {}", quote(&script.to_string_lossy()))
}

/// Leading `KEY=value` words (an optional `env` first), and the command after them.
#[cfg(any(windows, test))]
fn split_env(line: &str) -> (Vec<(String, String)>, &str) {
    let mut env = vec![];
    let mut rest = line.trim_start();
    rest = rest.strip_prefix("env ").map_or(rest, str::trim_start);
    while let Some((key, after)) = rest.split_once('=') {
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            break;
        }
        let (value, after) = match after.strip_prefix(['"', '\'']) {
            Some(quoted) => {
                let close = after.chars().next().unwrap_or('"');
                let Some(end) = quoted.find(close) else { break };
                (&quoted[..end], &quoted[end + 1..])
            }
            None => after.split_once(' ').unwrap_or((after, "")),
        };
        env.push((key.to_owned(), value.to_owned()));
        rest = after.trim_start();
    }
    (env, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_leading_assignments() {
        let (env, rest) = split_env("env K=v Q=\"a b\" npx -y x --flag=1");
        assert_eq!(env, [("K".to_owned(), "v".to_owned()), ("Q".to_owned(), "a b".to_owned())]);
        assert_eq!(rest, "npx -y x --flag=1");
        let (env, rest) = split_env("NO_BROWSER=true \"C:\\a b\\gemini.cmd\"");
        assert_eq!(env, [("NO_BROWSER".to_owned(), "true".to_owned())]);
        assert_eq!(rest, "\"C:\\a b\\gemini.cmd\"");
        assert_eq!(split_env("claude --x=1").1, "claude --x=1");
    }

    #[cfg(unix)]
    #[test]
    fn quotes_for_sh() {
        assert_eq!(quote("/usr/bin/x"), "/usr/bin/x");
        assert_eq!(quote("a b'c"), "'a b'\\''c'");
        assert_eq!(env_prefix([("K", "v"), ("BAD;rm", "x")]), "env K=v ");
    }

    #[cfg(windows)]
    #[test]
    fn quotes_for_cmd() {
        assert_eq!(quote("C:\\x\\y.exe"), "C:\\x\\y.exe");
        assert_eq!(quote("C:\\Program Files\\y.exe"), "\"C:\\Program Files\\y.exe\"");
        assert_eq!(env_prefix([("K", "a b")]), "K=\"a b\" ");
    }

    #[tokio::test]
    async fn runs_a_line_with_its_environment() {
        let out = line(&format!("{}FOO=1 echo hi", env_prefix([("BAR", "two words")]))).tokio().output().await.unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("hi"));
    }
}
